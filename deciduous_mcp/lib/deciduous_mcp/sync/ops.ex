defmodule DeciduousMcp.Sync.Ops do
  @moduledoc """
  Applies the operations a CLI queued in its local write-ahead log.

  Every write the Rust CLI makes to its local database also appends an op to
  `.deciduous/remote-log.jsonl`, and the unacknowledged tail of that log is
  replayed here in order. This replaced sending touched nodes through
  `POST /import`, which replaces title, status, description and metadata
  wholesale: `deciduous status 2 completed` sent the whole of node 2 and put
  back a title an agent had changed through MCP a minute earlier.

  Two properties carry the design:

    * **Field level.** An op names only what the CLI changed. `update_node`
      carries `set` (title, description, status) and `metadata` (keys merged
      into the server's map); nothing else on the row is touched.

    * **At most once.** The op id is inserted into `applied_ops` in the same
      transaction as the change. The insert comes first and uses
      `ON CONFLICT DO NOTHING`, so two replays racing each other serialise on
      the primary key and the second one sees "duplicate", not a second
      application of an edit someone may already have changed back.

  Each op is its own transaction and gets its own answer:

    * `applied`   – the change was made
    * `exists`    – a create for a node or edge the server already holds; the
                    server's copy is left as it is
    * `absent`    – a delete for something the server does not hold (or
                    already deleted); the goal state already holds
    * `duplicate` – this op id was applied before
    * `rejected`  – with a `reason`; nothing changed and the op id is not
                    recorded, so it can be sent again once the cause is fixed

  A rejection is an answer, not a failure of the batch: the ops after it are
  still applied. The CLI keeps rejected ops in its log and prints them until
  someone deals with them, which is what keeps them from being silent.
  """
  import Ecto.Query

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Edge, Node}
  alias DeciduousMcp.Sync.Import

  @max_ops 5_000
  @settable ~w(title description status)

  def run(%{"ops" => ops} = payload) when is_list(ops) do
    with {:ok, name} <- Workspaces.normalize_name(payload["workspace"] || ""),
         :ok <- check_batch(ops),
         {:ok, workspace} <- Workspaces.find_or_create(name),
         {:ok, _claim} <- Workspaces.claim(workspace, payload["repo_roots"], false) do
      {:ok,
       %{
         workspace: workspace.name,
         results: Enum.map(ops, &apply_one(workspace, &1))
       }}
    end
  end

  def run(_), do: {:error, "payload must contain an \"ops\" list"}

  defp check_batch(ops) do
    cond do
      length(ops) > @max_ops ->
        {:error, "at most #{@max_ops} ops per request; the CLI sends them in batches"}

      Enum.any?(ops, &(not is_map(&1))) ->
        {:error, "every op must be an object"}

      Enum.any?(ops, fn op -> not (is_binary(op["op_id"]) and op["op_id"] != "") end) ->
        {:error, "every op needs an op_id; without one it cannot be applied at most once"}

      true ->
        :ok
    end
  end

  defp apply_one(workspace, %{"op_id" => op_id} = op) do
    kind = op["kind"]

    result =
      Repo.transaction(fn ->
        {recorded, _} =
          Repo.insert_all(
            "applied_ops",
            [
              %{
                workspace_id: Ecto.UUID.dump!(workspace.id),
                op_id: op_id,
                kind: to_string(kind),
                applied_at: DateTime.utc_now()
              }
            ],
            on_conflict: :nothing,
            conflict_target: [:workspace_id, :op_id]
          )

        if recorded == 0 do
          "duplicate"
        else
          case apply_op(workspace, kind, op) do
            {:ok, outcome} -> outcome
            {:rejected, reason} -> Repo.rollback({:rejected, reason})
          end
        end
      end)

    case result do
      {:ok, outcome} -> %{op_id: op_id, result: outcome}
      {:error, {:rejected, reason}} -> %{op_id: op_id, result: "rejected", reason: reason}
      {:error, other} -> %{op_id: op_id, result: "rejected", reason: describe(other)}
    end
  end

  # --- Nodes ------------------------------------------------------------------

  defp apply_op(ws, "create_node", op) do
    with {:ok, cid} <- change_id(op, "change_id") do
      case any_node(ws, cid) do
        %Node{deleted_at: nil} ->
          {:ok, "exists"}

        %Node{} ->
          {:rejected, "node #{cid} was deleted on the server; it is not recreated"}

        nil ->
          now = DateTime.utc_now()

          attrs = %{
            workspace_id: ws.id,
            change_id: cid,
            node_type: op["node_type"],
            title: op["title"],
            description: op["description"],
            status: op["status"] || "pending",
            metadata: op["metadata"] || %{}
          }

          %Node{}
          |> Node.changeset(attrs)
          # Backdated archaeology nodes (`deciduous add --date`) keep their
          # date; the CLI's timestamp is the fact, the arrival time is not.
          |> Ecto.Changeset.put_change(:inserted_at, Import.parse_time(op["created_at"], now))
          |> Ecto.Changeset.put_change(:updated_at, Import.parse_time(op["updated_at"], now))
          |> Repo.insert()
          |> case do
            {:ok, _} -> {:ok, "applied"}
            {:error, cs} -> {:rejected, "create_node #{cid}: #{errors(cs)}"}
          end
      end
    end
  end

  # Compare-and-set, field by field. The op carries, beside each value it
  # sets, the value it replaced (`was` for columns, `was_metadata` for
  # metadata keys, null for a key that was absent). A field is written only
  # while the row still holds that value; if it holds the new value already
  # there is nothing to do; if it holds anything else, someone changed it
  # after this edit was made and the op is refused, whole, with both values.
  #
  # Comparing the op's `at` with the row's `updated_at` looks simpler and is
  # wrong twice: an agent retitling a node would make a queued status edit
  # to it look stale (updated_at is per row, not per field), and it trusts
  # the laptop's clock against the server's.
  #
  # The row is locked for the read-merge-write, so two ops merging different
  # metadata keys into one node at the same moment cannot lose either.
  defp apply_op(ws, "update_node", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         {:ok, set} <- settable(op["set"]),
         {:ok, meta} <- metadata(op["metadata"]),
         :ok <- nonempty(cid, set, meta),
         {:ok, was} <- previous(op, "was", Map.keys(set)),
         {:ok, was_meta} <- previous(op, "was_metadata", Map.keys(meta)),
         {:ok, node} <- live_node(ws, cid, lock: true),
         {:ok, set} <- compare(cid, set, was, &Map.get(node, String.to_existing_atom(&1))),
         {:ok, meta} <-
           compare(cid, meta, was_meta, &Map.get(node.metadata || %{}, &1), "metadata.") do
      attrs =
        if meta == %{},
          do: set,
          else: Map.put(set, "metadata", Map.merge(node.metadata || %{}, meta))

      if attrs == %{} do
        # Every field already held the value this op sets.
        {:ok, "exists"}
      else
        case Nodes.update_node(node.id, attrs) do
          {:ok, _} -> {:ok, "applied"}
          {:error, %Ecto.Changeset{} = cs} -> {:rejected, "update_node #{cid}: #{errors(cs)}"}
          {:error, other} -> {:rejected, "update_node #{cid}: #{describe(other)}"}
        end
      end
    end
  end

  defp apply_op(ws, "delete_node", op) do
    with {:ok, cid} <- change_id(op, "change_id") do
      case any_node(ws, cid) do
        nil ->
          {:ok, "absent"}

        %Node{deleted_at: nil} = node ->
          case Nodes.delete_node(node.id) do
            {:ok, _} -> {:ok, "applied"}
            {:error, other} -> {:rejected, "delete_node #{cid}: #{describe(other)}"}
          end

        %Node{} ->
          {:ok, "absent"}
      end
    end
  end

  # --- Edges ------------------------------------------------------------------

  defp apply_op(ws, "create_edge", op) do
    with {:ok, from_cid} <- change_id(op, "from_change_id"),
         {:ok, to_cid} <- change_id(op, "to_change_id"),
         {:ok, from} <- live_node(ws, from_cid),
         {:ok, to} <- live_node(ws, to_cid) do
      type = op["edge_type"] || "leads_to"

      if edge(from.id, to.id, type) do
        {:ok, "exists"}
      else
        attrs = %{
          from_node_id: from.id,
          to_node_id: to.id,
          edge_type: type,
          rationale: op["rationale"]
        }

        attrs = if is_number(op["weight"]), do: Map.put(attrs, :weight, op["weight"]), else: attrs

        case Edges.create_edge(ws.id, attrs) do
          {:ok, _} ->
            {:ok, "applied"}

          {:error, %Ecto.Changeset{} = cs} ->
            {:rejected, "create_edge #{from_cid} -> #{to_cid}: #{errors(cs)}"}

          {:error, other} ->
            {:rejected, "create_edge #{from_cid} -> #{to_cid}: #{describe(other)}"}
        end
      end
    end
  end

  defp apply_op(ws, "delete_edge", op) do
    with {:ok, from_cid} <- change_id(op, "from_change_id"),
         {:ok, to_cid} <- change_id(op, "to_change_id") do
      type = op["edge_type"] || "leads_to"

      with %Node{} = from <- any_node(ws, from_cid),
           %Node{} = to <- any_node(ws, to_cid),
           %Edge{} <- edge(from.id, to.id, type) do
        case Edges.delete_edge(from.id, to.id, type) do
          {:ok, _} -> {:ok, "applied"}
          {:error, :not_found} -> {:ok, "absent"}
        end
      else
        nil -> {:ok, "absent"}
      end
    end
  end

  defp apply_op(_ws, kind, _op) do
    {:rejected,
     "unknown op kind #{inspect(kind)}; this server applies create_node, update_node, " <>
       "delete_node, create_edge and delete_edge. A newer CLI than this server?"}
  end

  # --- Helpers ----------------------------------------------------------------

  defp change_id(op, key) do
    case op[key] do
      cid when is_binary(cid) and cid != "" -> {:ok, cid}
      other -> {:rejected, "#{op["kind"]} needs #{key}, got #{inspect(other)}"}
    end
  end

  defp settable(nil), do: {:ok, %{}}

  defp settable(set) when is_map(set) do
    case Map.keys(set) -- @settable do
      [] ->
        {:ok, set}

      unknown ->
        {:rejected,
         "update_node cannot set #{Enum.join(unknown, ", ")}; settable: #{Enum.join(@settable, ", ")}"}
    end
  end

  defp settable(other),
    do: {:rejected, "update_node set must be an object, got #{inspect(other)}"}

  defp metadata(nil), do: {:ok, %{}}
  defp metadata(m) when is_map(m), do: {:ok, m}

  defp metadata(other),
    do: {:rejected, "update_node metadata must be an object, got #{inspect(other)}"}

  defp nonempty(cid, set, meta) do
    if set == %{} and meta == %{},
      do: {:rejected, "update_node #{cid} changes nothing (no set, no metadata)"},
      else: :ok
  end

  # Every field an op sets must say what it replaced. An op without it
  # cannot be checked against a newer edit, and applying it blind is the
  # overwrite this exists to stop.
  defp previous(op, key, fields) do
    was = op[key] || %{}

    cond do
      not is_map(was) ->
        {:rejected, "update_node #{key} must be an object, got #{inspect(was)}"}

      (missing = Enum.reject(fields, &Map.has_key?(was, &1))) != [] ->
        {:rejected,
         "update_node #{op["change_id"]} does not say what it replaced for " <>
           "#{Enum.join(missing, ", ")} (#{key}); without it a newer edit on the server " <>
           "would be overwritten unseen. A CLI older than this server?"}

      true ->
        {:ok, was}
    end
  end

  # Drops fields that already hold the new value; refuses if any field holds
  # neither the new value nor the one the edit replaced.
  defp compare(cid, fields, was, current, prefix \\ "") do
    {conflicts, todo} =
      Enum.reduce(fields, {[], %{}}, fn {k, v}, {bad, keep} ->
        now = current.(k)

        cond do
          now == v -> {bad, keep}
          now == was[k] -> {bad, Map.put(keep, k, v)}
          true -> {[{prefix <> k, now, was[k], v} | bad], keep}
        end
      end)

    case conflicts do
      [] ->
        {:ok, todo}

      _ ->
        detail =
          conflicts
          |> Enum.reverse()
          |> Enum.map_join("; ", fn {k, now, was, v} ->
            "#{k}: the server has #{inspect(now)}, this edit changed #{inspect(was)} to #{inspect(v)}"
          end)

        {:rejected,
         "node #{cid} changed on the server after this edit was made (#{detail}). " <>
           "`deciduous remote pull` takes the server's value; " <>
           "`deciduous remote push --repair` sends this copy's"}
    end
  end

  defp any_node(ws, cid, opts \\ []) do
    q = from n in Node, where: n.workspace_id == ^ws.id and n.change_id == ^cid
    q = if opts[:lock], do: lock(q, "FOR UPDATE"), else: q
    Repo.one(q)
  end

  defp live_node(ws, cid, opts \\ []) do
    case any_node(ws, cid, opts) do
      nil -> {:rejected, "no node #{cid} on the server"}
      %Node{deleted_at: nil} = node -> {:ok, node}
      %Node{} -> {:rejected, "node #{cid} was deleted on the server"}
    end
  end

  defp edge(from_id, to_id, type) do
    Repo.one(
      from e in Edge,
        where: e.from_node_id == ^from_id and e.to_node_id == ^to_id and e.edge_type == ^type
    )
  end

  defp errors(%Ecto.Changeset{} = cs) do
    cs
    |> Ecto.Changeset.traverse_errors(fn {msg, opts} ->
      Enum.reduce(opts, msg, fn {k, v}, acc ->
        String.replace(acc, "%{#{k}}", to_string(inspect(v)))
      end)
    end)
    |> Enum.map_join("; ", fn {field, msgs} -> "#{field} #{Enum.join(msgs, ", ")}" end)
  end

  defp describe(reason) when is_binary(reason), do: reason
  defp describe(reason), do: inspect(reason)
end

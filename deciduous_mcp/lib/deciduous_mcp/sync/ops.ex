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

  defp apply_op(ws, "update_node", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         {:ok, set} <- settable(op["set"]),
         {:ok, meta} <- metadata(op["metadata"]),
         {:ok, node} <- live_node(ws, cid) do
      attrs =
        if meta == %{},
          do: set,
          else: Map.put(set, "metadata", Map.merge(node.metadata || %{}, meta))

      if attrs == %{} do
        {:rejected, "update_node #{cid} changes nothing (no set, no metadata)"}
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

  defp any_node(ws, cid) do
    Repo.one(from n in Node, where: n.workspace_id == ^ws.id and n.change_id == ^cid)
  end

  defp live_node(ws, cid) do
    case any_node(ws, cid) do
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

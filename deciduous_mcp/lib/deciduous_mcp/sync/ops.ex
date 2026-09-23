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

  alias DeciduousMcp.Activity
  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.MCP.ArgCheck
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Document, Edge, Node}
  alias DeciduousMcp.Sync.Import

  @max_ops 5_000
  @settable ~w(title description status)

  # What an op may carry, in the JSON Schema terms MCP's tools use, checked
  # by the same DeciduousMcp.MCP.ArgCheck (with_limits adds the sizes: a
  # title 10,000 characters and not blank, a branch 512, any other string
  # 262,144, an array 1,000 items, 1 Mi of text in all). Before, /ops held
  # an op to none of it: a 1,000,000-character title was applied and
  # query_nodes served it back whole (SERVER-N3).
  #
  # create_node takes the node schema's vocabulary, which keeps the two
  # legacy values (`feedback`, `done`) that exist in graphs on disk: the
  # CLI refuses them for a new node, so a create carrying one is a seed of
  # a node it already held, and refusing it would leave that op rejected
  # in the log for good. An update sets a status the CLI chose now, and
  # holds to the current vocabulary, as MCP does.
  #
  # The metadata keys MCP's add_node writes are held to its types (files
  # "notalist" and prompt {"a": 1} were applied; add_node refuses both).
  # Other keys are the node's own and pass, bounded like every string.
  @metadata_schema %{
    type: "object",
    properties: %{
      branch: %{type: "string"},
      confidence: %{type: "number", minimum: 0, maximum: 100},
      prompt: %{type: "string"},
      commit: %{type: "string"},
      files: %{type: "array", items: %{type: "string"}}
    }
  }

  # Only creates and updates of nodes went through held_to/3; a create_edge
  # with a 2,000,000-character rationale was applied (add_edge refuses it
  # at 262,144). The weight is a number and not negative, as the edge
  # changeset has it; no upper bound, since a finite float is stored and
  # read back exactly and no client writes anything but 1.0.
  @edge_schema %{
    type: "object",
    properties: %{
      edge_type: %{type: "string", enum: Edge.edge_types()},
      rationale: %{type: "string"},
      weight: %{type: "number", minimum: 0}
    }
  }

  @kinds ~w(create_node update_node delete_node create_edge delete_edge
             attach_document detach_document describe_document)

  @create_schema %{
    type: "object",
    required: ["node_type", "title"],
    properties: %{
      node_type: %{type: "string", enum: Node.node_types()},
      title: %{type: "string"},
      description: %{type: "string"},
      status: %{type: "string", enum: Node.statuses()},
      metadata: @metadata_schema
    }
  }

  @update_schema %{
    type: "object",
    properties: %{
      set: %{
        type: "object",
        properties: %{
          title: %{type: "string"},
          description: %{type: "string"},
          status: %{type: "string", enum: Node.statuses() -- ["done"]}
        }
      },
      metadata: @metadata_schema
    }
  }

  # A workspace is created here only when the batch holds a create_node
  # that would be applied. Before, find_or_create ran first, so an empty
  # batch, or one whose every op was refused, left a workspace behind
  # (SERVER-N6), against chapter 22's rule that a failed write creates
  # none. Against a workspace that does not exist every other op has its
  # answer already: an update or an edge names a node the server does not
  # hold, and a delete finds nothing to delete.
  def run(%{"ops" => ops} = payload) when is_list(ops) do
    raw = payload["workspace"] || ""

    with {:ok, name} <- workspace_name(raw),
         :ok <- check_batch(ops),
         {:ok, workspace, claim} <- workspace_for(name, ops, payload["repo_roots"]) do
      {results, _claim} = Enum.map_reduce(ops, claim, &apply_one(workspace, &1, &2))
      record_activity(workspace, ops, results, payload["repo_roots"])
      {:ok, %{workspace: name, results: results}}
    end
  end

  def run(_), do: {:error, "payload must contain an \"ops\" list"}

  # The CLI shows up in check_activity beside the MCP sessions (team probe
  # T10: its writes never appeared). One entry per branch a batch wrote a
  # node on, or the no-branch entry when what it applied names none (an
  # update or an edge op carries no branch). A CLI has no session; a
  # repository's root commits name it across runs, and all 1.0.7-style
  # clients that send none share one entry.
  defp record_activity(nil, _ops, _results, _roots), do: :ok

  defp record_activity(workspace, ops, results, roots) do
    applied =
      ops
      |> Enum.zip(results)
      |> Enum.filter(fn {_op, r} -> r.result == "applied" end)
      |> Enum.map(fn {op, _} -> get_in(op, ["metadata", "branch"]) end)
      |> Enum.map(&if(is_binary(&1), do: &1, else: ""))
      |> Enum.uniq()

    session =
      case roots do
        [root | _] when is_binary(root) -> "cli:" <> String.slice(root, 0, 12)
        _ -> "cli"
      end

    Enum.each(applied, &Activity.record(workspace.id, &1, session, "deciduous CLI", nil))
  end

  defp workspace_name(raw) do
    case Workspaces.normalize_name(raw) do
      {:ok, "*"} -> {:error, Workspaces.describe_name_error(raw, :global)}
      {:ok, name} -> {:ok, name}
      {:error, reason} -> {:error, Workspaces.describe_name_error(raw, reason)}
    end
  end

  # The claim is checked before anything is applied (a workspace another
  # repository holds is refused whole, as before) and recorded only by the
  # first op that writes, in that op's transaction. It used to be recorded
  # up front: on a workspace an agent had made over MCP, which no
  # repository has claimed, an /ops batch that wrote nothing (empty, every
  # op rejected, a delete of an absent node) claimed it for its roots, and
  # the real repository's next push was 409 claimed_by_other_repository
  # (verification of SERVER-N6). Recording it in the writing op's own
  # transaction, rather than after the batch, means two repositories
  # racing for an unclaimed workspace cannot both write: the second one's
  # op is refused and rolled back.
  #
  # Returns {:ok, workspace or nil, roots still to record or nil}.
  defp workspace_for(name, ops, roots) do
    case Workspaces.get_by_name(name) do
      {:ok, workspace} ->
        with {:ok, _status} <- Workspaces.check_claim(workspace, roots),
             {:ok, valid} <- Workspaces.validate_roots(roots) do
          {:ok, workspace, if(is_list(valid) and valid != [], do: roots)}
        end

      {:error, :not_found} ->
        if Enum.any?(ops, &would_create?/1) do
          with {:ok, workspace} <- Workspaces.find_or_create(name),
               {:ok, _claim} <- Workspaces.claim(workspace, roots, false),
               do: {:ok, workspace, nil}
        else
          # Checked all the same: a malformed repo_roots is an error
          # whether or not anything is written.
          with {:ok, _} <- Workspaces.validate_roots(roots), do: {:ok, nil, nil}
        end
    end
  end

  defp would_create?(%{"kind" => "create_node"} = op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         :ok <- held_to(@create_schema, op, cid),
         {:ok, _} <- time(op, "created_at", cid),
         {:ok, _} <- time(op, "updated_at", cid) do
      true
    else
      _ -> false
    end
  end

  defp would_create?(_op), do: false

  defp check_batch(ops) do
    cond do
      length(ops) > @max_ops ->
        {:error, "at most #{@max_ops} ops per request; the CLI sends them in batches"}

      Enum.any?(ops, &(not is_map(&1))) ->
        {:error, "every op must be an object"}

      Enum.any?(ops, fn op -> not (is_binary(op["op_id"]) and op["op_id"] != "") end) ->
        {:error, "every op needs an op_id; without one it cannot be applied at most once"}

      # applied_ops.op_id is varchar(255) and text cannot hold a NUL; both
      # were an empty 500 from the insert.
      (i = Enum.find_index(ops, &String.contains?(&1["op_id"], <<0>>))) != nil ->
        {:error, "ops[#{i}].op_id contains a NUL character (U+0000); nothing was applied"}

      (i = Enum.find_index(ops, &(String.length(&1["op_id"]) > 255))) != nil ->
        {:error,
         "ops[#{i}].op_id is #{String.length(Enum.at(ops, i)["op_id"])} characters; " <>
           "the limit is 255; nothing was applied"}

      true ->
        :ok
    end
  end

  # No workspace: nothing is recorded, since there is nowhere to record it,
  # and nothing is created (see run/1). The answers are the ones an empty
  # workspace would give, after the same malformed-op checks.
  defp apply_one(nil, %{"op_id" => op_id} = op, claim) do
    with nil <- malformed(op),
         {:ok, outcome} <- apply_op(nil, op["kind"], op) do
      {%{op_id: op_id, result: outcome}, claim}
    else
      {:rejected, reason} -> {%{op_id: op_id, result: "rejected", reason: reason}, claim}
      reason when is_binary(reason) -> {%{op_id: op_id, result: "rejected", reason: reason}, claim}
    end
  end

  # An unknown kind is answered before anything is recorded: its name went
  # into applied_ops.kind (varchar(255)), and a 300-character one was an
  # empty 500.
  defp apply_one(_workspace, %{"op_id" => op_id, "kind" => kind} = op, claim)
       when kind not in @kinds do
    {:rejected, reason} = apply_op(nil, kind, op)
    {%{op_id: op_id, result: "rejected", reason: reason}, claim}
  end

  # An op the database cannot store is an answer about that op, not a
  # failure of the request. A NUL (Postgres text refuses it), an id or
  # change_id longer than the varchar(255) it goes into, or a kind that is not
  # a string used to raise inside the transaction, and the whole request
  # answered an empty 500. The CLI resent that batch on every write, got the
  # same 500, and nothing after the bad op reached the server again
  # (SERVER-N1). They are refused here, by name, before anything is written.
  defp apply_one(workspace, %{"op_id" => op_id} = op, claim) do
    case malformed(op) do
      nil -> apply_checked(workspace, op, claim)
      reason -> {%{op_id: op_id, result: "rejected", reason: reason}, claim}
    end
  end

  @max_id 255
  @max_float 1.7976931348623157e308

  defp malformed(op) do
    cond do
      path = nul_path(op, []) ->
        "the op contains a NUL character (at #{path}), which the server cannot store; " <>
          "nothing was written"

      String.length(op["op_id"]) > @max_id ->
        "op_id is #{String.length(op["op_id"])} characters; the limit is #{@max_id}"

      not is_binary(op["kind"]) ->
        "kind must be a string, got #{inspect(op["kind"])}"

      String.length(op["kind"]) > @max_id ->
        "kind is #{String.length(op["kind"])} characters; the limit is #{@max_id}"

      # Ecto casts the weight with :erlang.float/1, which raises (an empty
      # 500 for the whole request) on an integer past the float range.
      is_integer(op["weight"]) and abs(op["weight"]) > @max_float ->
        "weight #{op["weight"] |> Integer.to_string() |> String.slice(0, 20)}... " <>
          "(#{op["weight"] |> Integer.to_string() |> String.length()} digits) is too large for a float"

      key =
          Enum.find(~w(change_id from_change_id to_change_id), fn k ->
            is_binary(op[k]) and String.length(op[k]) > @max_id
          end) ->
        "#{key} is #{String.length(op[key])} characters; the limit is #{@max_id}"

      true ->
        nil
    end
  end

  defp nul_path(v, path) when is_binary(v),
    do: if(String.contains?(v, <<0>>), do: render_path(path))

  defp nul_path(v, path) when is_map(v) do
    Enum.find_value(v, fn {k, x} ->
      if is_binary(k) and String.contains?(k, <<0>>),
        # As text ("a\0b"), not as an Elixir binary (<<97, 0, 98>>).
        do: render_path([inspect(k, binaries: :as_strings) | path]),
        else: nul_path(x, [k | path])
    end)
  end

  defp nul_path(v, path) when is_list(v) do
    v |> Enum.with_index() |> Enum.find_value(fn {x, i} -> nul_path(x, [i | path]) end)
  end

  defp nul_path(_, _), do: nil

  defp render_path([]), do: "the top level"
  defp render_path(path), do: path |> Enum.reverse() |> Enum.map_join(".", &to_string/1)

  defp apply_checked(workspace, %{"op_id" => op_id} = op, claim) do
    kind = op["kind"]

    result =
      try do
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
              {:ok, "applied"} when claim != nil -> record_claim(workspace, claim)
              {:ok, outcome} -> outcome
              {:rejected, reason} -> Repo.rollback({:rejected, reason})
            end
          end
        end)
      rescue
        # The database refusing this op's data (class 22, data exception, or
        # 23, integrity) is about the op, and the checks above missed it: an
        # answer, so the ops after it still apply. Anything else (a lost
        # connection, a bug) is the server's failure and stays a 500, which
        # the CLI retries rather than setting the op aside.
        e in Postgrex.Error ->
          case e.postgres do
            %{code: code, message: message} when is_atom(code) ->
              if data_error?(e.postgres),
                do: {:error, {:rejected, "the database refused this op (#{code}): #{message}"}},
                else: reraise(e, __STACKTRACE__)

            _ ->
              reraise(e, __STACKTRACE__)
          end
      end

    case result do
      {:ok, "applied"} ->
        {%{op_id: op_id, result: "applied"}, nil}

      {:ok, outcome} ->
        {%{op_id: op_id, result: outcome}, claim}

      {:error, {:rejected, reason}} ->
        {%{op_id: op_id, result: "rejected", reason: reason}, claim}

      {:error, other} ->
        {%{op_id: op_id, result: "rejected", reason: describe(other)}, claim}
    end
  end

  defp record_claim(workspace, roots) do
    case Workspaces.claim(workspace, roots, false) do
      {:ok, _} ->
        "applied"

      {:error, {:claimed_by_other_repository, held}} ->
        Repo.rollback(
          {:rejected,
           "workspace #{workspace.name} was claimed by another repository (root commits " <>
             "#{Enum.join(Enum.take(held, 5), ", ")}) while this batch ran; nothing was written"}
        )

      {:error, reason} ->
        Repo.rollback({:rejected, describe(reason)})
    end
  end

  # --- Nodes ------------------------------------------------------------------

  defp apply_op(ws, "create_node", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         :ok <- held_to(@create_schema, op, "create_node #{cid}"),
         {:ok, inserted_at} <- time(op, "created_at", cid),
         {:ok, updated_at} <- time(op, "updated_at", cid) do
      Nodes.lock_change_id(ws.id, cid)

      wanted_type = op["node_type"]

      case any_node(ws, cid) do
        # A node of another type is another node. Before add_node took a
        # change_id, only the CLI and the server made them, and whatever
        # was under one was this op's node; an agent can now choose one,
        # and `exists` for an agent's goal answered the CLI's action under
        # the same id: the CLI believed its action was on the server
        # (verification of chapter 30). The type is what can be compared:
        # no path changes a node's type. The title cannot: a seed or a
        # replayed create carries the title the CLI had, and a retitle on
        # either side since then is the normal case, not another node.
        %Node{deleted_at: nil, node_type: type} = node when type != wanted_type ->
          {:rejected,
           "create_node #{cid}: change_id #{cid} is already #{article(type)} #{type} " <>
             "#{inspect(node.title)} on the server; this op creates " <>
             "#{article(op["node_type"])} #{op["node_type"]} #{inspect(op["title"])}. " <>
             "Nothing was written. Two nodes cannot share a change_id: one of the two " <>
             "writers reused it"}

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
          |> Ecto.Changeset.put_change(:inserted_at, inserted_at || now)
          |> Ecto.Changeset.put_change(:updated_at, updated_at || now)
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
         :ok <- held_to(@update_schema, op, "update_node #{cid}"),
         :ok <- nonempty(cid, set, meta),
         {:ok, was} <- previous(op, "was", Map.keys(set)),
         {:ok, was_meta} <- previous(op, "was_metadata", Map.keys(meta)),
         {:ok, node} <- editable_node(ws, cid, op),
         {:ok, set} <- compare(cid, set, was, &Map.get(node, String.to_existing_atom(&1))),
         {:ok, meta} <-
           compare(cid, meta, was_meta, &Map.get(node.metadata || %{}, &1), "metadata.") do
      attrs =
        if meta == %{},
          do: set,
          else: Map.put(set, "metadata", Map.merge(node.metadata || %{}, meta))

      if attrs == %{} and is_nil(node.deleted_at) do
        # Every field already held the value this op sets.
        {:ok, "exists"}
      else
        case Nodes.update_node(node.id, attrs, revive: true) do
          {:ok, _} -> {:ok, "applied"}
          {:error, %Ecto.Changeset{} = cs} -> {:rejected, "update_node #{cid}: #{errors(cs)}"}
          {:error, other} -> {:rejected, "update_node #{cid}: #{describe(other)}"}
        end
      end
    end
  end

  # A delete carries what the node held when it was deleted (`was`: title,
  # description and status; `was_metadata`: the whole map) and applies only
  # while the server still holds exactly that. Without it a delete queued on
  # a laptop won over an edit made after it anywhere else, and `remote pull`
  # then deleted the edited node on every clone (round-2 NEW-2). git's merge
  # driver keeps the edit in that race ("an edit after a delete brings the
  # node back").
  #
  # Content settles it only when the edit reaches the server first. When
  # the delete is first, the edit meets a tombstone, and content cannot say
  # whether it was made before the delete or after it; that takes clocks,
  # as git's driver uses. The tombstone is dated when the delete was made
  # (the op's `at`, not its arrival), and `editable_node/3` lets an edit
  # made after it bring the node back.
  #
  # A delete of a node this server never had leaves a tombstone. The node
  # reached the deleter through git, and its create is still queued on the
  # laptop that made it; without the tombstone that create, replayed later,
  # put the node back for good (NEW-3).
  defp apply_op(ws, "delete_node", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         {:ok, was} <- deleted_state(op),
         {:ok, made} <- made_at(op) do
      case any_node(ws, cid, lock: true) do
        nil ->
          bury(ws, cid, op)
          {:ok, "absent"}

        %Node{deleted_at: nil} = node ->
          case delete_conflicts(node, was) do
            [] ->
              case Nodes.delete_node(node.id, at: made) do
                {:ok, _} -> {:ok, "applied"}
                {:error, other} -> {:rejected, "delete_node #{cid}: #{describe(other)}"}
              end

            conflicts ->
              {:rejected,
               "node #{cid} changed on the server after this delete was made (" <>
                 Enum.map_join(conflicts, "; ", fn {k, now, then} ->
                   "#{k}: the server has #{inspect(now)}, this copy deleted it at #{inspect(then)}"
                 end) <>
                 "). Nothing was deleted. `deciduous remote pull` brings the newer node " <>
                 "back here; delete it again if it should still go"}
          end

        %Node{} ->
          {:ok, "absent"}
      end
    end
  end

  # --- Edges ------------------------------------------------------------------
  #
  # An edge has no fields to compare-and-set; what can go stale is whether it
  # exists. The server keeps a tombstone for every unlink (see the
  # edge_tombstones migration) and orders a link and an unlink of the same
  # edge by when each was made on its machine: `created_at` of the link,
  # `deleted_at` (else `at`) of the unlink. That is the clock git's merge
  # driver already uses for the same pair in graph.json, so the server and
  # git settle a link/unlink race the same way. A link older than the
  # server's tombstone is refused, and so is an unlink older than the edge
  # the server holds (a relink made after it).

  defp apply_op(ws, "create_edge", op) do
    with {:ok, from_cid} <- change_id(op, "from_change_id"),
         {:ok, to_cid} <- change_id(op, "to_change_id"),
         {:ok, made} <- instant(op, "created_at"),
         :ok <- held_to(@edge_schema, op, "create_edge #{from_cid} -> #{to_cid}"),
         {:ok, from} <- live_node(ws, from_cid),
         {:ok, to} <- live_node(ws, to_cid) do
      type = op["edge_type"] || "leads_to"
      # The lock Edges.create_edge takes too, on the pair in either
      # direction: taken here first so the check below and the insert are
      # one step against every other writer of an edge between these two.
      Edges.lock_pair(ws.id, from.id, to.id)

      cond do
        edge(from.id, to.id, type) ->
          {:ok, "exists"}

        (dead = tombstone(ws, from_cid, to_cid, type)) &&
            DateTime.compare(dead.deleted_at, made) == :gt ->
          {:rejected,
           "the edge #{from_cid} -> #{to_cid} (#{type}) was unlinked on the server at " <>
             "#{DateTime.to_iso8601(dead.deleted_at)}, after this link was made at " <>
             "#{DateTime.to_iso8601(made)}; it is not linked again. " <>
             "`deciduous remote pull` removes it here; link it again if it should stay"}

        true ->
          attrs = %{
            from_node_id: from.id,
            to_node_id: to.id,
            edge_type: type,
            rationale: op["rationale"],
            inserted_at: made
          }

          attrs =
            if is_number(op["weight"]), do: Map.put(attrs, :weight, op["weight"]), else: attrs

          case Edges.create_edge(ws.id, attrs) do
            {:ok, _} ->
              {:ok, "applied"}

            {:error, {:edge_exists, _}} ->
              {:ok, "exists"}

            # A 2-cycle, refused as MCP's add_edge refuses it (T6: MCP
            # add_edge A -> B, then this op B -> A, was applied). Said by
            # change_id, which is what the CLI knows the nodes by.
            {:error, {:reverse_exists, rev}} ->
              {:rejected,
               "create_edge #{from_cid} -> #{to_cid}: #{to_cid} -> #{from_cid} " <>
                 "(#{rev.edge_type}) already exists, and the two nodes cannot be each " <>
                 "other's parent; nothing was written. `deciduous unlink` the local one, then " <>
                 "`deciduous remote push --drop-rejected`"}

            {:error, %Ecto.Changeset{} = cs} ->
              {:rejected, "create_edge #{from_cid} -> #{to_cid}: #{errors(cs)}"}

            # Edges.create_edge reads both ends again, FOR SHARE, and finds
            # one gone when a delete committed after the check above. Said
            # as the check above says it, by change_id; it was the inspected
            # tuple with a server UUID the CLI has never seen (SERVER-N8).
            {:error, {:node_not_found, id}} ->
              cid = if id == from.id, do: from_cid, else: to_cid

              {:rejected,
               "create_edge #{from_cid} -> #{to_cid}: node #{cid} was deleted on the server"}

            {:error, other} ->
              {:rejected, "create_edge #{from_cid} -> #{to_cid}: #{describe(other)}"}
          end
      end
    end
  end

  defp apply_op(ws, "delete_edge", op) do
    with {:ok, from_cid} <- change_id(op, "from_change_id"),
         {:ok, to_cid} <- change_id(op, "to_change_id"),
         {:ok, unlinked} <- instant(op, if(op["deleted_at"], do: "deleted_at", else: "at")) do
      type = op["edge_type"] || "leads_to"
      stamp(unlinked)

      live =
        with %Node{} = from <- any_node(ws, from_cid),
             %Node{} = to <- any_node(ws, to_cid) do
          edge(from.id, to.id, type)
        end

      case live do
        %Edge{} = e ->
          if DateTime.compare(e.inserted_at, unlinked) == :gt do
            {:rejected,
             "the edge #{from_cid} -> #{to_cid} (#{type}) was linked again on the server at " <>
               "#{DateTime.to_iso8601(e.inserted_at)}, after this unlink was made at " <>
               "#{DateTime.to_iso8601(unlinked)}; it is kept. `deciduous remote pull` " <>
               "brings it back here; unlink it again if it should go"}
          else
            case Edges.delete_edge(e.from_node_id, e.to_node_id, type) do
              {:ok, _} -> {:ok, "applied"}
              {:error, :not_found} -> {:ok, "absent"}
            end
          end

        _ ->
          # Never here, or already removed: the tombstone is what a link
          # replayed later is checked against.
          Repo.query!(
            """
            INSERT INTO edge_tombstones (workspace_id, from_change_id, to_change_id, edge_type, deleted_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (workspace_id, from_change_id, to_change_id, edge_type)
            DO UPDATE SET deleted_at = greatest(edge_tombstones.deleted_at, EXCLUDED.deleted_at)
            """,
            [Ecto.UUID.dump!(ws.id), from_cid, to_cid, type, DateTime.to_naive(unlinked)]
          )

          {:ok, "absent"}
      end
    end
  end

  # --- Documents --------------------------------------------------------------
  #
  # Attachments went to the server only through `/import` (`remote push
  # --seed`), and a detach or a new description never did: a document
  # detached to take a pasted secret out of the graph stayed on the shared
  # server with nothing to say so (round-2 BRIDGE-N4). They are ops now,
  # like every other write. The bytes go first, by `PUT /blob/:hash`; an
  # attach whose bytes have not arrived is kept with `content_missing`, as
  # /import keeps one, and the upload that follows marks them found.

  defp apply_op(ws, "attach_document", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         {:ok, node_cid} <- change_id(op, "node_change_id"),
         {:ok, hash} <- content_hash(op),
         {:ok, node} <- live_node(ws, node_cid) do
      case document(ws, cid) do
        %Document{detached_at: nil} ->
          {:ok, "exists"}

        %Document{} ->
          {:rejected, "document #{cid} was detached on the server; it is not attached again"}

        nil ->
          now = DateTime.utc_now()

          %Document{}
          |> Document.changeset(%{
            change_id: cid,
            node_id: node.id,
            workspace_id: ws.id,
            content_hash: hash,
            original_filename: op["original_filename"],
            storage_filename: op["storage_filename"],
            mime_type: op["mime_type"] || "application/octet-stream",
            file_size: op["file_size"],
            description: op["description"],
            description_source: op["description_source"] || "none",
            attached_by: op["attached_by"],
            content_missing: not DeciduousMcp.Storage.exists?(hash)
          })
          |> Ecto.Changeset.put_change(:inserted_at, Import.parse_time(op["attached_at"], now))
          |> Repo.insert()
          |> case do
            {:ok, _} -> {:ok, "applied"}
            {:error, cs} -> {:rejected, "attach_document #{cid}: #{errors(cs)}"}
          end
      end
    end
  end

  defp apply_op(ws, "detach_document", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         {:ok, at} <- instant(op, "at") do
      case document(ws, cid, lock: true) do
        %Document{detached_at: nil} = d ->
          d
          |> Ecto.Changeset.change(detached_at: at)
          |> Repo.update()
          |> case do
            {:ok, _} -> {:ok, "applied"}
            {:error, cs} -> {:rejected, "detach_document #{cid}: #{errors(cs)}"}
          end

        _ ->
          {:ok, "absent"}
      end
    end
  end

  # Compare-and-set on the description, like update_node on a field.
  defp apply_op(ws, "describe_document", op) do
    with {:ok, cid} <- change_id(op, "change_id"),
         :ok <- has_key(op, "was_description") do
      new = op["description"]

      case document(ws, cid, lock: true) do
        nil ->
          {:rejected, "no document #{cid} on the server"}

        %Document{detached_at: %DateTime{}} ->
          {:rejected, "document #{cid} was detached on the server"}

        %Document{description: ^new} ->
          {:ok, "exists"}

        %Document{description: now} = d ->
          if now == op["was_description"] do
            d
            |> Document.changeset(%{
              description: new,
              description_source: op["description_source"] || "user"
            })
            |> Repo.update()
            |> case do
              {:ok, _} -> {:ok, "applied"}
              {:error, cs} -> {:rejected, "describe_document #{cid}: #{errors(cs)}"}
            end
          else
            {:rejected,
             "document #{cid} changed on the server after this edit was made (description: " <>
               "the server has #{inspect(now)}, this edit changed #{inspect(op["was_description"])} " <>
               "to #{inspect(new)})"}
          end
      end
    end
  end

  defp apply_op(_ws, kind, _op) do
    {:rejected,
     "unknown op kind #{inspect(kind, printable_limit: 60)}; this server applies create_node, update_node, " <>
       "delete_node, create_edge, delete_edge, attach_document, detach_document and " <>
       "describe_document. A newer CLI than this server?"}
  end

  defp document(ws, cid, opts \\ []) do
    q = from d in Document, where: d.workspace_id == ^ws.id and d.change_id == ^cid
    q = if opts[:lock], do: lock(q, "FOR UPDATE"), else: q
    Repo.one(q)
  end

  defp content_hash(op) do
    case op["content_hash"] do
      h when is_binary(h) ->
        if String.match?(h, ~r/\A[0-9a-fA-F]{64}\z/),
          do: {:ok, String.downcase(h)},
          else: {:rejected, "attach_document needs a sha256 content_hash, got #{inspect(h)}"}

      other ->
        {:rejected, "attach_document needs a sha256 content_hash, got #{inspect(other)}"}
    end
  end

  defp has_key(op, key) do
    if Map.has_key?(op, key),
      do: :ok,
      else:
        {:rejected,
         "#{op["kind"]} #{op["change_id"]} does not say what it replaced (#{key}); without it " <>
           "a newer edit on the server would be overwritten unseen"}
  end

  # --- Helpers ----------------------------------------------------------------

  @node_columns ~w(title description status)

  # What a delete says the node held. Every column and the whole metadata
  # map: a delete removes all of it, so a change to any of it since is a
  # newer edit the delete did not see.
  defp deleted_state(op) do
    was = op["was"]
    was_meta = op["was_metadata"]

    cond do
      not is_map(was) or Enum.any?(@node_columns, &(not Map.has_key?(was, &1))) ->
        {:rejected,
         "delete_node #{op["change_id"]} does not say what the node held when it was " <>
           "deleted (was: #{Enum.join(@node_columns, ", ")}; was_metadata); without it a " <>
           "newer edit on the server would be deleted unseen. A CLI older than this server?"}

      not is_map(was_meta) ->
        {:rejected,
         "delete_node #{op["change_id"]} does not say what metadata the node held " <>
           "(was_metadata must be an object, got #{inspect(was_meta)})"}

      true ->
        {:ok, {was, was_meta}}
    end
  end

  defp delete_conflicts(node, {was, was_meta}) do
    blank = fn
      "" -> nil
      v -> v
    end

    columns =
      for k <- @node_columns,
          now = Map.get(node, String.to_existing_atom(k)),
          blank.(now) != blank.(was[k]),
          do: {k, now, was[k]}

    held = node.metadata || %{}

    meta =
      for k <- Enum.uniq(Map.keys(held) ++ Map.keys(was_meta)),
          Map.get(held, k) != Map.get(was_meta, k),
          do: {"metadata." <> k, Map.get(held, k), Map.get(was_meta, k)}

    columns ++ Enum.sort(meta)
  end

  # A tombstone for a node this server never had.
  defp bury(ws, cid, op) do
    now = DateTime.utc_now()
    type = if op["node_type"] in Node.node_types(), do: op["node_type"], else: "observation"

    Repo.insert_all(
      Node,
      [
        %{
          id: Ecto.UUID.generate(),
          workspace_id: ws.id,
          change_id: cid,
          node_type: type,
          title: "",
          status: "pending",
          metadata: %{},
          inserted_at: now,
          updated_at: now,
          deleted_at: now
        }
      ],
      on_conflict: :nothing,
      conflict_target: [:workspace_id, :change_id]
    )
  end

  defp tombstone(ws, from_cid, to_cid, type) do
    case Repo.query!(
           """
           SELECT deleted_at FROM edge_tombstones
           WHERE workspace_id = $1 AND from_change_id = $2 AND to_change_id = $3 AND edge_type = $4
           """,
           [Ecto.UUID.dump!(ws.id), from_cid, to_cid, type]
         ) do
      %{rows: [[at]]} -> %{deleted_at: DateTime.from_naive!(at, "Etc/UTC")}
      %{rows: []} -> nil
    end
  end

  # Dates the tombstone the trigger writes for this transaction's unlink.
  defp stamp(at) do
    Repo.query!("SELECT set_config('deciduous.unlinked_at', $1, true)", [
      DateTime.to_iso8601(at)
    ])
  end

  defp instant(op, key) do
    with v when is_binary(v) <- op[key],
         {:ok, dt, _} <- DateTime.from_iso8601(v) do
      # UTC, microseconds, and 6-digit precision whatever the input had:
      # the columns are :utc_datetime_usec (see Import.parse_time/2).
      dt = DateTime.truncate(dt, :microsecond)
      {:ok, %{dt | microsecond: {elem(dt.microsecond, 0), 6}}}
    else
      _ ->
        {:rejected,
         "#{op["kind"]} needs #{key} as an ISO 8601 time, got #{inspect(op[key])}; it orders " <>
           "this op against a link or unlink of the same edge made elsewhere"}
    end
  end

  defp held_to(schema, op, what) do
    case ArgCheck.check(ArgCheck.with_limits(schema), op) do
      :ok -> :ok
      {:error, message} -> {:rejected, "#{what}: #{message}"}
    end
  end

  # The CLI sends RFC 3339 with an offset; a database from before it may
  # hold a naive "YYYY-MM-DD HH:MM:SS", which is UTC. Absent is the arrival
  # time. Anything else is refused: "99999-01-01T00:00:00Z" and 12345 used
  # to become the arrival time without a word, and so did the naive form.
  defp time(op, key, cid) do
    case op[key] do
      nil ->
        {:ok, nil}

      value when is_binary(value) ->
        case DateTime.from_iso8601(value) do
          {:ok, dt, _offset} ->
            {:ok, %{dt | microsecond: {elem(dt.microsecond, 0), 6}}}

          _ ->
            case NaiveDateTime.from_iso8601(value) do
              {:ok, naive} ->
                dt = DateTime.from_naive!(naive, "Etc/UTC")
                {:ok, %{dt | microsecond: {elem(dt.microsecond, 0), 6}}}

              _ ->
                {:rejected,
                 "create_node #{cid}: #{key} #{inspect(value)} is not an ISO 8601 time"}
            end
        end

      other ->
        {:rejected,
         "create_node #{cid}: #{key} must be an ISO 8601 string, got #{inspect(other)}"}
    end
  end

  # The column is varchar(255), and Postgres cannot hold a NUL in text: a
  # 300-character change_id was an empty HTTP 500 (and, before the
  # workspace was created only for a create that would apply, an empty
  # workspace left behind).
  defp change_id(op, key) do
    case op[key] do
      cid when is_binary(cid) and cid != "" ->
        cond do
          String.contains?(cid, <<0>>) ->
            {:rejected,
             "#{op["kind"]} #{key} contains a NUL character (U+0000); no change_id can hold one"}

          String.length(cid) > 255 ->
            {:rejected,
             "#{op["kind"]} #{key} is #{String.length(cid)} characters; the limit is 255"}

          true ->
            {:ok, cid}
        end

      other ->
        {:rejected, "#{op["kind"]} needs #{key}, got #{inspect(other, printable_limit: 60)}"}
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
           "`deciduous remote push --repair --overwrite-server` sends this copy's over it"}
    end
  end

  defp any_node(ws, cid, opts \\ [])
  defp any_node(nil, _cid, _opts), do: nil

  defp any_node(ws, cid, opts) do
    q = from n in Node, where: n.workspace_id == ^ws.id and n.change_id == ^cid
    q = if opts[:lock], do: lock(q, "FOR UPDATE"), else: q
    Repo.one(q)
  end

  # The node an update_node op edits: a live one, or a deleted one when the
  # edit was made after the delete (see the delete_node clause). A row
  # `bury/3` left for a node this server never held is not brought back: it
  # has no title or type to bring.
  defp editable_node(ws, cid, op) do
    case any_node(ws, cid, lock: true) do
      nil ->
        {:rejected, "no node #{cid} on the server"}

      %Node{deleted_at: nil} = node ->
        {:ok, node}

      %Node{deleted_at: dead} = node ->
        cond do
          node.title == "" and node.inserted_at == dead ->
            {:rejected,
             "node #{cid} was deleted here, and this server never held it; an edit does not " <>
               "bring it back"}

          not is_binary(op["at"]) ->
            {:rejected,
             "node #{cid} was deleted on the server, and this edit has no `at` to say " <>
               "whether it was made after the delete"}

          true ->
            with {:ok, made} <- instant(op, "at") do
              if DateTime.compare(made, dead) == :gt do
                {:ok, node}
              else
                {:rejected,
                 "node #{cid} was deleted on the server at #{DateTime.to_iso8601(dead)}, " <>
                   "after this edit was made at #{DateTime.to_iso8601(made)}"}
              end
            end
        end
    end
  end

  # When a CLI op was made, for dating what it does; nil (now) for an op
  # that does not say. An `at` that is not a time is refused by name.
  defp made_at(%{"at" => nil}), do: {:ok, nil}
  defp made_at(%{"at" => _} = op), do: instant(op, "at")
  defp made_at(_), do: {:ok, nil}

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

  defp data_error?(%{pg_code: "22" <> _}), do: true
  defp data_error?(%{pg_code: "23" <> _}), do: true
  defp data_error?(_), do: false

  defp article(<<first, _::binary>>) when first in ~c"aeiou", do: "an"
  defp article(_), do: "a"

  defp describe(reason) when is_binary(reason), do: reason
  defp describe(reason), do: inspect(reason)
end

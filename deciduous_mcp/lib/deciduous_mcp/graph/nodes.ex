defmodule DeciduousMcp.Graph.Nodes do
  @moduledoc """
  Context module for decision graph node operations.
  All queries are scoped to a workspace for team isolation.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.{Node, AuditLog}

  @doc """
  Creates a new decision node in the given workspace.
  Generates a change_id (UUID) for sync compatibility with the Rust CLI.
  """
  def create_node(workspace_id, attrs) do
    change_id = Map.get(attrs, :change_id) || Map.get(attrs, "change_id") || UUID.uuid4()

    node_attrs =
      attrs
      |> Map.put(:workspace_id, workspace_id)
      |> Map.put(:change_id, change_id)

    Repo.transaction(fn ->
      case %Node{} |> Node.changeset(node_attrs) |> Repo.insert() do
        {:ok, node} ->
          audit_change(workspace_id, node, "create")
          broadcast(workspace_id, {:node_created, node})
          node

        {:error, changeset} ->
          Repo.rollback(changeset)
      end
    end)
  end

  @doc """
  Makes creates of one change_id in one workspace take turns, until the
  calling transaction ends. POST /ops and add_node with a change_id both
  take it before they look for the node, so whichever comes second finds
  the first one's node instead of losing on the unique index.
  """
  def lock_change_id(workspace_id, change_id) do
    # POST /import writes rows by change_id too, and takes this
    # workspace's lock exclusively (SERVER-N4 through /import).
    :ok = DeciduousMcp.Graph.Workspaces.lock_shared(workspace_id)

    Repo.query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [
      Enum.join(["node", workspace_id, change_id], "|")
    ])

    :ok
  end

  @doc "The node with this change_id in the workspace, deleted or not, or nil."
  def any_by_change_id(workspace_id, change_id) do
    Repo.one(from n in Node, where: n.workspace_id == ^workspace_id and n.change_id == ^change_id)
  end

  @doc """
  Updates an existing node. Only provided fields are changed.

  A soft-deleted node is refused with `{:error, :deleted}`: the tools check
  first, and this is the belt for every other caller. `revive: true` (a
  CLI's edit made after the delete, see `Sync.Ops`) clears `deleted_at`
  instead and applies the edit.

  `merge_metadata: true` treats `attrs.metadata` as a patch on the stored
  map (JSON merge patch, top level): keys given are set, keys given as nil
  are removed, keys not given are kept. It is applied here, under the row
  lock, rather than by the caller reading the row first, so two patches to
  one node cannot each start from the same old map and lose the other's key.
  Without it, metadata is replaced whole, which is what the sync processor
  replaying a CLI node wants.
  """
  def update_node(node_id, attrs, opts \\ []) do
    revive = opts[:revive] == true

    Repo.transaction(fn ->
      case Node |> lock("FOR UPDATE") |> Repo.get(node_id) do
        nil ->
          Repo.rollback(:not_found)

        %Node{deleted_at: %DateTime{}} when not revive ->
          Repo.rollback(:deleted)

        node ->
          attrs = if opts[:merge_metadata], do: merge_metadata(node, attrs), else: attrs
          revived = revive and node.deleted_at != nil
          key = if Enum.any?(Map.keys(attrs), &is_binary/1), do: "deleted_at", else: :deleted_at
          attrs = if revived, do: Map.put(attrs, key, nil), else: attrs

          case node |> Node.update_changeset(attrs) |> Repo.update() do
            {:ok, updated} ->
              audit_change(
                node.workspace_id,
                updated,
                if(revived, do: "restore", else: "update"),
                attrs
              )

              broadcast(
                node.workspace_id,
                if(revived, do: {:node_created, updated}, else: {:node_updated, updated})
              )

              updated

            {:error, changeset} ->
              Repo.rollback(changeset)
          end
      end
    end)
  end

  @doc """
  Soft-deletes a node by setting deleted_at.

  Deleting a node twice is `{:error, :already_deleted}`, not a second
  success: the second call used to overwrite deleted_at with a later time,
  so the tombstone's timestamp said the node died when someone last asked.

  The row is locked `FOR UPDATE` for the duration, and `Edges.create_edge`
  reads its endpoints `FOR SHARE`, so a delete and a link racing on the same
  node serialise instead of both committing against a node the other one
  had already changed.
  """
  def delete_node(node_id, opts \\ []) do
    # `at:` dates the tombstone when the delete was made (a CLI's queued
    # delete), never later than now.
    at =
      case opts[:at] do
        %DateTime{} = t -> Enum.min([t, DateTime.utc_now()], DateTime)
        nil -> DateTime.utc_now()
      end

    Repo.transaction(fn ->
      case Node |> lock("FOR UPDATE") |> Repo.get(node_id) do
        nil ->
          Repo.rollback(:not_found)

        %Node{deleted_at: %DateTime{}} ->
          Repo.rollback(:already_deleted)

        node ->
          case node
               |> Node.update_changeset(%{deleted_at: at})
               |> Repo.update() do
            {:ok, deleted} ->
              audit_change(node.workspace_id, deleted, "delete")
              broadcast(node.workspace_id, {:node_deleted, deleted})
              deleted

            {:error, changeset} ->
              Repo.rollback(changeset)
          end
      end
    end)
  end

  @doc """
  Gets a single node by ID, with optional preloads.
  """
  def get_node(node_id, preloads \\ []) do
    # Repo.get raises Ecto.Query.CastError on a non-UUID; not found is the
    # honest answer for an id that cannot name a row.
    with {:ok, _} <- if(is_binary(node_id), do: Ecto.UUID.cast(node_id), else: :error),
         %Node{} = node <- Repo.get(Node, node_id) do
      {:ok, Repo.preload(node, preloads)}
    else
      _ -> {:error, :not_found}
    end
  end

  @doc """
  Gets a node by its change_id within a workspace.
  Used for sync operations where the CLI references nodes by change_id.
  """
  def get_node_by_change_id(workspace_id, change_id) do
    Node
    |> where([n], n.workspace_id == ^workspace_id and n.change_id == ^change_id)
    |> where([n], is_nil(n.deleted_at))
    |> Repo.one()
    |> case do
      nil -> {:error, :not_found}
      node -> {:ok, node}
    end
  end

  @doc """
  The most recent node on every branch of a workspace, newest branch first.

  This is the read the agents in the first arena actually wanted from
  `check_activity`: not "who holds a lock" but "what did everyone just do."
  They were polling `query_nodes` for decisions because it was the only way
  to see what was new. One `DISTINCT ON` over the branch key answers it in
  a single query, and a node with no branch in its metadata lands under
  `""` rather than being dropped, so an agent that forgot to pass one still
  shows up.

  Returns `{rows, total}`: the `limit` most recently written branches
  (default 20) and how many branches the workspace has in all.
  """
  def latest_per_branch(workspace_id, opts \\ []) do
    limit = Keyword.get(opts, :limit, 20)

    # A loose index scan, hand-written because Postgres 17 has no skip scan
    # and Ecto has no LATERAL-in-recursive-CTE. The DISTINCT ON this replaces
    # read every live row in the workspace and sorted them to disk before
    # keeping one per branch: 7,805 rows, a 9,424 kB external merge, 36.9 ms
    # for epstein, on every check_activity call. This walks
    # idx_nodes_ws_branchkey_latest instead: the first row of the index is
    # the newest node on the lowest branch key; each recursion step asks for
    # the first row with a strictly greater branch key, which is the newest
    # node on the next branch. One index probe per branch, 0.065 ms for the
    # same workspace. Same result set as the DISTINCT ON, checked with EXCEPT
    # in both directions.
    #
    # `coalesce(metadata ->> 'branch', '')` must be spelled exactly as it is
    # in the index expression or the planner will not match it.
    sql = """
    WITH RECURSIVE per_branch AS (
      (SELECT n.*
         FROM decision_nodes n
        WHERE n.workspace_id = $1 AND n.deleted_at IS NULL
        ORDER BY coalesce(n.metadata ->> 'branch', ''), n.inserted_at DESC, n.id DESC
        LIMIT 1)
      UNION ALL
      SELECT nx.*
        FROM per_branch pb,
             LATERAL (SELECT n.*
                        FROM decision_nodes n
                       WHERE n.workspace_id = $1 AND n.deleted_at IS NULL
                         AND coalesce(n.metadata ->> 'branch', '') > coalesce(pb.metadata ->> 'branch', '')
                       ORDER BY coalesce(n.metadata ->> 'branch', ''), n.inserted_at DESC, n.id DESC
                       LIMIT 1) nx
    )
    SELECT * FROM per_branch
    """

    {:ok, %{rows: raw_rows, columns: cols}} =
      Ecto.Adapters.SQL.query(Repo, sql, [Ecto.UUID.dump!(workspace_id)])

    rows =
      raw_rows
      |> Enum.map(&Repo.load(Node, {cols, &1}))
      |> Enum.sort_by(& &1.inserted_at, {:desc, DateTime})

    # One row per branch is already the cheap part; the expensive part is a
    # workspace that has had sixty branches since 2025 handing all sixty to
    # an agent that asked what is happening right now. Most recent first,
    # then cut, and say how many there were.
    {Enum.take(rows, limit), length(rows)}
  end

  @doc """
  Lists nodes in a workspace with optional filters.

  Options:
  - `:type` — filter by node_type
  - `:status` — filter by status
  - `:branch` — filter by metadata.branch
  - `:search` — text search in title/description
  - `:limit` — max results (default 100)
  - `:offset` — pagination offset
  """
  def list_nodes(scope, opts \\ []) do
    Node
    |> scope_workspace(scope)
    |> where([n], is_nil(n.deleted_at))
    |> maybe_filter_type(opts[:type])
    |> maybe_filter_status(opts[:status])
    |> maybe_filter_branch(opts[:branch])
    |> maybe_search(opts[:search])
    |> order_by([n], desc: n.inserted_at)
    |> limit(^(opts[:limit] || 100))
    |> offset(^(opts[:offset] || 0))
    |> Repo.all()
    |> Repo.preload(:workspace)
  end

  @doc """
  Counts nodes by type in a workspace. Useful for dashboard/pulse.
  """
  def count_by_type(scope) do
    Node
    |> scope_workspace(scope)
    |> where([n], is_nil(n.deleted_at))
    |> group_by([n], n.node_type)
    |> select([n], {n.node_type, count(n.id)})
    |> Repo.all()
    |> Map.new()
  end

  # --- Private helpers ---

  defp merge_metadata(node, %{metadata: patch} = attrs) when is_map(patch) do
    merged =
      Enum.reduce(patch, node.metadata || %{}, fn
        {key, nil}, acc -> Map.delete(acc, key)
        {key, value}, acc -> Map.put(acc, key, value)
      end)

    %{attrs | metadata: merged}
  end

  # Not a map: left as it is, for the changeset to refuse by name.
  defp merge_metadata(_node, attrs), do: attrs

  # `:global` is the cross-project view: no workspace predicate at all. It is a
  # distinct atom rather than a nil workspace_id so that an unresolved
  # workspace can never silently widen a query to every project on the machine.
  defp scope_workspace(query, :global), do: query

  defp scope_workspace(query, workspace_id),
    do: where(query, [n], n.workspace_id == ^workspace_id)

  defp maybe_filter_type(query, nil), do: query
  defp maybe_filter_type(query, type), do: where(query, [n], n.node_type == ^type)

  defp maybe_filter_status(query, nil), do: query
  defp maybe_filter_status(query, status), do: where(query, [n], n.status == ^status)

  defp maybe_filter_branch(query, nil), do: query

  defp maybe_filter_branch(query, branch) do
    where(query, [n], fragment("? ->> 'branch' = ?", n.metadata, ^branch))
  end

  defp maybe_search(query, nil), do: query
  defp maybe_search(query, ""), do: query

  defp maybe_search(query, search_term) do
    pattern = contains_pattern(search_term)

    where(
      query,
      [n],
      ilike(n.title, ^pattern) or ilike(n.description, ^pattern)
    )
  end

  @doc """
  An ILIKE pattern matching `term` anywhere, with the term taken literally.

  `%` and `_` are LIKE wildcards and `\\` is its default escape character, so
  a term interpolated as-is turned "%" and "_" into match-everything and made
  a search for a backslash match nothing. Backslash is escaped first, so the
  escapes added for `%` and `_` are not themselves escaped.
  """
  def contains_pattern(term) do
    escaped =
      term
      |> String.replace("\\", "\\\\")
      |> String.replace("%", "\\%")
      |> String.replace("_", "\\_")

    "%" <> escaped <> "%"
  end

  defp audit_change(workspace_id, node, action, changes \\ %{}) do
    %AuditLog{}
    |> AuditLog.changeset(%{
      workspace_id: workspace_id,
      entity_type: "node",
      entity_id: node.id,
      entity_change_id: node.change_id,
      action: action,
      changes: changes,
      source: "mcp"
    })
    |> Repo.insert()
  end

  defp broadcast(workspace_id, message) do
    Phoenix.PubSub.broadcast(
      DeciduousMcp.PubSub,
      "graph:#{workspace_id}",
      message
    )
  end
end

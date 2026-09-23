defmodule DeciduousMcp.MCP.Scope do
  @moduledoc """
  Decides which workspace a tool call acts on, and whether it acts on all of
  them.

  Precedence, highest first:

    1. `X-Deciduous-Workspace` header, resolved by
       `DeciduousMcp.Web.WorkspacePlug` into `frame.assigns.pinned_workspace_id`.
       A pinned repo cannot have its nodes redirected by a tool argument.
    2. The call's own `workspace` argument, which is how the single user-scope
       registration serves every directory on the machine: the client passes
       the git repo root's basename.
    3. `@fallback` — everything that is not a git repo pools into one tree
       rather than minting a workspace per temp directory.

  Read tools additionally accept `workspace: "*"`, which is the global view:
  no workspace filter at all, every project at once. Write tools reject it,
  because a node has to land somewhere specific.

  Every write also claims a short advisory lock, keyed by workspace and
  branch (`DeciduousMcp.Locks`), before it is allowed to proceed — two agents
  on different branches never contend by default, but two on the same one do,
  and the second gets told who is holding it rather than writing a node that
  interleaves with a burst the first agent is mid-way through.
  """

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Locks

  @fallback "scratch"
  @global "*"

  @doc """
  The `workspace` property to merge into a tool's `input_schema`.
  `global?: true` documents the `"*"` form for read tools.
  """
  def schema_property(opts \\ []) do
    base =
      "Project this call belongs to: the basename of the git repo root " <>
        "(e.g. \"deciduous\", \"blog\"). Anything outside a git repo uses " <>
        "\"#{@fallback}\". Ignored when the client pinned a workspace via the " <>
        "X-Deciduous-Workspace header."

    description =
      if opts[:global?] do
        base <> " Pass \"#{@global}\" to query across every project at once."
      else
        base
      end

    %{workspace: %{type: "string", description: description}}
  end

  @doc """
  Merges the `workspace` argument into a tool definition's input schema.

  Applied to the definition map rather than written into each tool's literal
  schema so that the wording of the argument — which is the only thing telling
  a client how to name a project — lives in exactly one place.
  """
  def with_workspace_arg(definition, opts \\ []) do
    update_in(definition, [:input_schema], fn schema ->
      schema
      |> Map.put_new(:type, "object")
      |> Map.update(:properties, schema_property(opts), &Map.merge(&1, schema_property(opts)))
    end)
  end

  @doc """
  Resolves a write scope, and claims the branch lock for it.

  On conflict, the error names who holds it and for how much longer, so the
  caller (an LLM, almost always) has what it needs to just say so rather than
  silently retrying into the same collision.
  """
  def write_workspace_id(frame, args) do
    case Map.get(args, "workspace") do
      @global ->
        {:error,
         "workspace \"#{@global}\" is read-only: a node must be written to one " <>
           "project. Pass the repo name."}

      _ ->
        with {:ok, workspace_id} <- resolve_single(frame, args) do
          claim_lock(workspace_id, frame, args)
        end
    end
  end

  @doc """
  Resolves a write scope from the node being written to, and claims the
  branch lock for it.

  `update_node`, `delete_node` and `delete_edge` name a row by id rather than
  a workspace by name, so the workspace argument the other write tools take
  would be the wrong source of truth here: a caller that omitted it would
  lock `scratch` while editing a node in `blog`. The node already knows its
  workspace. Look it up, refuse if it is gone, then claim the lock exactly
  as `write_workspace_id/2` does.

  For an edge, the source node stands in for the edge — an edge row carries
  no branch of its own, and both of its endpoints are in one workspace by
  construction.
  """
  def write_scope_for_node(frame, node_id, args) do
    with {:ok, node} <- lookup_node(node_id),
         :ok <- check_live(node),
         :ok <- check_pin(frame, node) do
      claim_lock(node.workspace_id, frame, args)
    else
      {:error, :not_found} -> {:error, "Node not found: #{node_id}"}
      {:error, message} when is_binary(message) -> {:error, message}
    end
  end

  # The moduledoc's promise is that a pinned repo cannot have its writes
  # redirected, nor read its neighbours. Resolving the workspace from the node would quietly break it
  # the other way round: a client pinned to `blog` naming a node in
  # `deciduous` would take `deciduous`'s lock and edit `deciduous`'s row.
  defp check_pin(frame, node) do
    case pinned_id(frame) do
      nil ->
        :ok

      pinned when pinned == node.workspace_id ->
        :ok

      _ ->
        {:error,
         "node #{node.id} belongs to another workspace than the one this client is pinned to"}
    end
  end

  # A soft-deleted row is kept for audit and for /export's tombstones, not
  # to be read or edited by id. Reading one answered with no sign it was
  # deleted; editing one said "Node updated"; deleting one again reset
  # deleted_at. Say what happened to it instead of "not found", so a caller
  # holding a stale id learns why.
  defp check_live(%{deleted_at: nil}), do: :ok

  defp check_live(node),
    do: {:error, "node #{node.id} was deleted at #{DateTime.to_iso8601(node.deleted_at)}"}

  defp lookup_node(node_id) do
    case Ecto.UUID.cast(node_id) do
      {:ok, _} -> Nodes.get_node(node_id)
      :error -> {:error, :not_found}
    end
  end

  defp claim_lock(workspace_id, frame, args) do
    with {:ok, workspace} <- Workspaces.get_workspace(workspace_id) do
      lock_key = Locks.lock_key_for(workspace, Map.get(args, "branch"))
      session_id = session_id(frame)
      client = client_info(frame)

      case Locks.acquire(workspace_id, lock_key, session_id, client.name, client.version) do
        {:ok, _lock} ->
          {:ok, workspace_id}

        {:error, holder} ->
          {:error, lock_conflict_message(workspace.name, lock_key, holder)}
      end
    else
      {:error, :not_found} -> {:error, "workspace vanished between resolve and lock"}
    end
  end

  defp lock_conflict_message(workspace_name, lock_key, holder) do
    remaining = max(DateTime.diff(holder.expires_at, DateTime.utc_now(), :second), 0)
    who = holder.client_name || "another client"

    where =
      case lock_key do
        "*" -> "workspace-wide"
        "" -> "no branch recorded"
        branch -> "branch \"#{branch}\""
      end

    "workspace \"#{workspace_name}\" (#{where}) is locked by #{who}" <>
      if(holder.client_version, do: " (#{holder.client_version})", else: "") <>
      ", session #{short_session(holder.session_id)}. " <>
      "Releases in #{remaining}s if that session goes idle, or finishes sooner. " <>
      "Retry shortly, or write to a different branch."
  end

  # Every session id Hermes hands out starts with the literal "session_", so
  # slicing the first N characters shows that fixed prefix, not anything that
  # tells two sessions apart. Strip it first.
  defp short_session(id) do
    id
    |> String.replace_prefix("session_", "")
    |> String.slice(0, 8)
  end

  defp session_id(frame) do
    frame.private
    |> Map.new()
    |> Map.get(:session_id, "unknown-session")
  end

  defp client_info(frame) do
    info =
      frame.private
      |> Map.new()
      |> Map.get(:client_info, %{})

    %{name: info["name"] || info[:name], version: info["version"] || info[:version]}
  end

  @doc """
  Resolves a read scope.

  Returns `{:ok, :global}` for the cross-project view or `{:ok, id}` for one
  workspace. A header pin beats `"*"` — a pinned repo stays pinned on reads too,
  so a project that opted into isolation cannot be made to read its neighbours.
  """
  def read_scope(frame, args) do
    cond do
      pinned = pinned_id(frame) ->
        {:ok, pinned}

      Map.get(args, "workspace") == @global ->
        {:ok, :global}

      true ->
        resolve_single(frame, args)
    end
  end

  @doc """
  Resolves a node named by id for a read, holding it to the pin.

  `read_scope/2` covers the tools that take a workspace. The ones that take
  a node id (show_node, get_ancestors, get_descendants) had no scope at all:
  a client pinned to `blog` could read any node on the server by UUID,
  description and prompt included, and walk another workspace's tree. Edges
  never cross workspaces, so checking the node a read starts from is enough
  to keep a traversal inside it too.

  Returns `{:ok, node}` or `{:error, message}`.
  """
  def read_node(frame, node_id, preloads \\ []) do
    with {:ok, node} <- Nodes.get_node(node_id, preloads),
         :ok <- check_live(node),
         :ok <- check_pin(frame, node) do
      {:ok, node}
    else
      {:error, :not_found} -> {:error, "Node not found: #{node_id}"}
      {:error, message} when is_binary(message) -> {:error, message}
    end
  end

  @doc "The workspace id a client pinned by header, or nil."
  def pinned_workspace_id(frame), do: pinned_id(frame)

  defp resolve_single(frame, args) do
    case pinned_id(frame) do
      nil -> resolve_by_name(Map.get(args, "workspace"))
      id -> {:ok, id}
    end
  end

  defp resolve_by_name(name) do
    raw = if is_binary(name) and String.trim(name) != "", do: name, else: @fallback

    with {:ok, normalized} <- Workspaces.normalize_name(raw),
         {:ok, workspace} <- Workspaces.find_or_create(normalized) do
      {:ok, workspace.id}
    else
      {:error, reason} -> {:error, "could not resolve workspace: #{inspect(reason)}"}
    end
  end

  defp pinned_id(frame) do
    frame.assigns
    |> Map.new()
    |> Map.get(:pinned_workspace_id)
  end

  def fallback_name, do: @fallback
  def global_token, do: @global
end

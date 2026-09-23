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
  @global Workspaces.global_token()

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
    with {:ok, workspace_id} <- resolve_for_write(frame, args) do
      claim_lock(workspace_id, frame, args)
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
         :ok <- check_pin(frame, node),
         :ok <- check_live(node) do
      claim_lock(node.workspace_id, frame, args)
    else
      {:error, :not_found} -> {:error, "Node not found: #{node_id}"}
      {:error, message} when is_binary(message) -> {:error, message}
    end
  end

  @doc """
  Checks that a node other than the one a write is scoped by may be
  touched by it: it exists, it is in the pinned workspace if there is a
  pin, and it is not deleted. Claims no lock.

  delete_edge scopes itself by its source node, so an edge *into* a
  deleted node was deleted with "Edge deleted" while one out of it was
  refused. Its target goes through this.
  """
  def check_node(frame, node_id) do
    with {:ok, node} <- lookup_node(node_id),
         :ok <- check_pin(frame, node),
         :ok <- check_live(node) do
      :ok
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
    case pinned(frame) do
      nil ->
        :ok

      pin ->
        case pinned_existing_id(pin) do
          {:ok, id} when id == node.workspace_id ->
            :ok

          _ ->
            {:error,
             "node #{node.id} belongs to another workspace than the one this client is pinned to"}
        end
    end
  end

  # Always asked after check_pin, never before. The other order told a
  # client pinned to one workspace which of another workspace's node ids
  # were deleted, and when: "was deleted at T" for those, "belongs to
  # another workspace" for live ones, "not found" for the rest.
  #
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

  A read never creates a workspace. An unknown name is an error that says so:
  before, every read went through find_or_create, and a typo, a probe for
  `proto-%`, or a name with a right-to-left override each left a permanent
  empty project behind in list_workspaces.
  """
  def read_scope(frame, args) do
    case read_target(frame, args) do
      {:absent, name} ->
        {:error,
         "no workspace named #{inspect(name)}: nothing has been written to it yet. " <>
           "The first write creates it; list_workspaces shows the ones that exist."}

      other ->
        other
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
         :ok <- check_pin(frame, node),
         :ok <- check_live(node) do
      {:ok, node}
    else
      {:error, :not_found} -> {:error, "Node not found: #{node_id}"}
      {:error, message} when is_binary(message) -> {:error, message}
    end
  end

  @doc """
  How the client is pinned: `nil` when it is not, `{:ok, id}` when its
  workspace exists, `:absent` when the header names one nothing has been
  written to yet.

  Three answers, not an id or nil: the plug no longer creates the pinned
  workspace, so "no id" no longer means "no pin". A caller that read nil as
  unpinned would show a client pinned to a new name every other workspace.
  """
  def pin_status(frame) do
    case pinned(frame) do
      nil ->
        nil

      pin ->
        case pinned_existing_id(pin) do
          {:ok, id} -> {:ok, id}
          {:error, :not_found} -> :absent
        end
    end
  end

  @doc """
  `read_scope/2`, but an unknown workspace is `{:absent, name}` rather than
  an error, for callers with a truthful empty answer to give (GET /export of
  a workspace nothing has been pushed to yet is an empty graph).
  """
  def read_target(frame, args) do
    case pinned(frame) do
      nil ->
        case requested_name(args) do
          {:ok, @global} -> {:ok, :global}
          {:ok, name} -> lookup(name)
          {:error, message} -> {:error, message}
        end

      pin ->
        case pinned_existing_id(pin) do
          {:ok, id} -> {:ok, id}
          {:error, :not_found} -> {:absent, pin_name(pin)}
        end
    end
  end

  defp resolve_for_write(frame, args) do
    case pinned(frame) do
      nil ->
        case requested_name(args) do
          {:ok, @global} ->
            {:error,
             "workspace \"#{@global}\" is read-only: a node must be written to one " <>
               "project. Pass the repo name."}

          {:ok, name} ->
            create(name)

          {:error, message} ->
            {:error, message}
        end

      {:id, id} ->
        {:ok, id}

      {:name, name} ->
        create(name)
    end
  end

  @doc """
  The workspace a write with these arguments would have to create, or nil
  when it names one that exists (or none, or "*", or an invalid name).

  `DeciduousMcp.MCP.Component` runs a call that would create one inside a
  transaction, and keeps the workspace only if the call succeeded and left
  a node in it. Creation has to happen before the tool's own checks (the
  tool needs the id to look anything up), so without that, a refused
  add_edge or a capture_conversation_turn that wrote nothing still left
  a permanent empty project in list_workspaces.
  """
  def workspace_to_create(frame, args) do
    target =
      case pinned(frame) do
        {:id, _} ->
          nil

        {:name, name} ->
          name

        nil ->
          case requested_name(args) do
            {:ok, @global} -> nil
            {:ok, name} -> name
            {:error, _} -> nil
          end
      end

    if target && match?({:error, :not_found}, Workspaces.get_by_name(target)), do: target
  end

  # The `workspace` argument, normalized. Compared with "*" only after
  # normalizing: `" *"` used to pass the write tools' read-only check as a
  # different string and then trim to a literal workspace named "*".
  defp requested_name(args) do
    raw =
      case Map.get(args, "workspace") do
        nil -> @fallback
        name when is_binary(name) -> if String.trim(name) == "", do: @fallback, else: name
        other -> other
      end

    case Workspaces.normalize_name(raw) do
      {:ok, name} -> {:ok, name}
      {:error, reason} -> {:error, Workspaces.describe_name_error(raw, reason)}
    end
  end

  defp lookup(name) do
    case Workspaces.get_by_name(name) do
      {:ok, workspace} -> {:ok, workspace.id}
      {:error, :not_found} -> {:absent, name}
    end
  end

  defp create(name) do
    case Workspaces.find_or_create(name) do
      {:ok, workspace} ->
        {:ok, workspace.id}

      {:error, reason} when is_atom(reason) ->
        {:error, Workspaces.describe_name_error(name, reason)}

      {:error, _} ->
        {:error, "could not create workspace #{inspect(name)}"}
    end
  end

  # What `DeciduousMcp.Web.WorkspacePlug` put in assigns. The plug no longer
  # creates the pinned workspace on connect (that made every initialize a
  # write), so a pin may name a workspace that does not exist yet:
  # `{:name, name}`. One that did exist when the request arrived is
  # `{:id, id}`.
  defp pinned(frame) do
    assigns = Map.new(frame.assigns)

    case {Map.get(assigns, :pinned_workspace_id), Map.get(assigns, :pinned_workspace_name)} do
      {id, _} when is_binary(id) -> {:id, id}
      {nil, name} when is_binary(name) -> {:name, name}
      _ -> nil
    end
  end

  defp pinned_existing_id({:id, id}), do: {:ok, id}

  defp pinned_existing_id({:name, name}) do
    with {:ok, workspace} <- Workspaces.get_by_name(name), do: {:ok, workspace.id}
  end

  defp pin_name({:name, name}), do: name

  def fallback_name, do: @fallback
  def global_token, do: @global
end

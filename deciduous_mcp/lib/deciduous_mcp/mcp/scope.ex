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

  alias DeciduousMcp.Graph.Workspaces
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

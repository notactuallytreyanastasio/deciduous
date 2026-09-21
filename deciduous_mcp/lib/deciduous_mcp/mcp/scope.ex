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
  """

  alias DeciduousMcp.Graph.Workspaces

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
  Resolves a write scope. Always a single workspace id.
  """
  def write_workspace_id(frame, args) do
    case Map.get(args, "workspace") do
      @global ->
        {:error,
         "workspace \"#{@global}\" is read-only: a node must be written to one " <>
           "project. Pass the repo name."}

      _ ->
        resolve_single(frame, args)
    end
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

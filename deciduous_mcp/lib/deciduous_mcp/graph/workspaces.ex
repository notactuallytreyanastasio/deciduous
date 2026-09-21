defmodule DeciduousMcp.Graph.Workspaces do
  @moduledoc """
  Context module for workspace management.
  A workspace isolates a team's decision graph data.
  """
  import Ecto.Query
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Workspace

  @doc """
  Finds a workspace by name, or creates it if it doesn't exist.
  Used during MCP initialization to resolve the active workspace.
  """
  def find_or_create(name) do
    case Repo.one(from w in Workspace, where: w.name == ^name) do
      nil ->
        %Workspace{}
        |> Workspace.changeset(%{name: name})
        |> Repo.insert()

      workspace ->
        {:ok, workspace}
    end
  end

  @doc """
  Gets a workspace by ID.
  """
  def get_workspace(id) do
    case Repo.get(Workspace, id) do
      nil -> {:error, :not_found}
      workspace -> {:ok, workspace}
    end
  end

  @doc """
  Lists all workspaces.
  """
  def list_workspaces do
    Repo.all(Workspace)
  end
end

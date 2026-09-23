defmodule DeciduousMcp.MCP.AskGraphScopeTest do
  @moduledoc """
  ask_graph's text search stays inside the workspace it was asked about and
  skips deleted nodes. It used `or_where` once per search term, which Ecto
  renders as `(workspace AND not deleted AND scope) OR term1 OR term2`, so a
  term matched every workspace on the server, deleted nodes included.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.Nodes
  alias DeciduousMcp.MCP.Tools.AskGraph

  setup do
    mine = create_test_workspace("ask-mine")
    theirs = create_test_workspace("ask-theirs")

    {:ok, _} = Nodes.create_node(mine.id, %{node_type: "goal", title: "zebra migration here"})

    {:ok, _} =
      Nodes.create_node(theirs.id, %{node_type: "goal", title: "zebra migration elsewhere"})

    {:ok, gone} =
      Nodes.create_node(mine.id, %{node_type: "goal", title: "zebra migration deleted"})

    {:ok, _} = Nodes.delete_node(gone.id)

    frame = %Hermes.Server.Frame{private: %{session_id: "session_ask"}, assigns: %{}}
    %{frame: frame}
  end

  test "text matches come only from the asked workspace, never deleted", %{frame: frame} do
    {:ok, json} =
      AskGraph.call(%{
        arguments: %{"workspace" => "ask-mine", "question" => "zebra migration"},
        server: frame
      })

    titles = json |> Jason.decode!() |> Map.fetch!("results") |> Enum.map(& &1["title"])

    assert titles == ["zebra migration here"]
  end
end

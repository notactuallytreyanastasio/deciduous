defmodule DeciduousMcp.MCP.CandidateParentsTest do
  @moduledoc """
  Write-time candidate discovery (Jev-Mem 3.2): add_node without parent_id
  answers with ranked parent guesses, and never links one.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Candidates, Edges, Nodes}
  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.MCP.Tools.{AddNode, FindOrphans}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Edge

  setup do
    ws = create_test_workspace("cands")
    frame = %Hermes.Server.Frame{private: %{session_id: "session_C"}, assigns: %{}}

    node = fn type, title, extra ->
      {:ok, n} =
        Nodes.create_node(ws.id, Map.merge(%{node_type: type, title: title}, extra))

      n
    end

    %{ws: ws, frame: frame, node: node}
  end

  defp call(tool, args, frame) do
    {:reply, resp, _} = Component.dispatch_tool(tool, args, frame)
    [%{"text" => text}] = resp.content
    Jason.decode!(text)
  end

  test "an outcome's best parent is the matching action, and nothing is linked",
       %{frame: frame, node: node} do
    goal = node.("goal", "Rate limit the public API", %{})
    option = node.("option", "Rate limit with a token bucket", %{})
    _decision = node.("decision", "Use token bucket for rate limiting", %{})

    action =
      node.("action", "Implement token bucket rate limiter", %{
        metadata: %{"branch" => "rl", "files" => ["lib/limiter.ex"]}
      })

    _other = node.("action", "Rewrite the landing page copy", %{})

    out =
      call(
        AddNode,
        %{
          "workspace" => "cands",
          "node_type" => "outcome",
          "title" => "Token bucket rate limiter passes load test",
          "branch" => "rl",
          "files" => ["lib/limiter.ex"]
        },
        frame
      )

    # Existing keys unchanged.
    for k <- ~w(id change_id node_type title status created message),
        do: assert(Map.has_key?(out, k))

    refute Map.has_key?(out, "parent_id")

    [first | _] = sp = out["suggested_parents"]
    assert first["id"] == action.id
    assert first["signals"]["type_fit"] == 1.0
    assert first["signals"]["same_branch"] == true
    assert first["signals"]["shared_files"] == ["lib/limiter.ex"]
    assert first["signals"]["text_similarity"] > 0.3
    assert length(sp) <= 5
    # An option never sits directly above an outcome; a goal may, weakly.
    refute Enum.any?(sp, &(&1["id"] == option.id))

    case Enum.find(sp, &(&1["id"] == goal.id)) do
      nil -> :ok
      g -> assert g["signals"]["type_fit"] == 0.2 and g["score"] < first["score"]
    end

    assert out["suggested_parents_hint"] =~ "to_node_id=#{out["id"]}"

    assert Edges.edges_to(out["id"]) == []
  end

  test "an option's candidates are goals; open goals outrank completed ones",
       %{frame: frame, node: node} do
    done = node.("goal", "Add dark mode", %{status: "completed"})
    open = node.("goal", "Add dark mode", %{status: "active"})

    out =
      call(
        AddNode,
        %{
          "workspace" => "cands",
          "node_type" => "option",
          "title" => "Dark mode via CSS variables"
        },
        frame
      )

    ids = Enum.map(out["suggested_parents"], & &1["id"])
    assert ids == [open.id, done.id]
    assert hd(out["suggested_parents"])["signals"]["open"] == true
  end

  test "goals and parented nodes get no suggestions", %{frame: frame, node: node} do
    goal = node.("goal", "g", %{})

    g =
      call(AddNode, %{"workspace" => "cands", "node_type" => "goal", "title" => "another"}, frame)

    refute Map.has_key?(g, "suggested_parents")

    o =
      call(
        AddNode,
        %{
          "workspace" => "cands",
          "node_type" => "option",
          "title" => "o",
          "parent_id" => goal.id
        },
        frame
      )

    refute Map.has_key?(o, "suggested_parents")
  end

  test "no fitting node: empty list and a hint saying so", %{frame: frame} do
    out =
      call(AddNode, %{"workspace" => "cands", "node_type" => "outcome", "title" => "x"}, frame)

    assert out["suggested_parents"] == []
    assert out["suggested_parents_hint"] =~ "orphan until linked"
  end

  test "other workspaces and deleted nodes are never candidates", %{frame: frame, node: node} do
    other = create_test_workspace("elsewhere")
    {:ok, _} = Nodes.create_node(other.id, %{node_type: "goal", title: "Ship search"})
    dead = node.("goal", "Ship search", %{})
    {:ok, _} = Nodes.delete_node(dead.id)

    out =
      call(
        AddNode,
        %{"workspace" => "cands", "node_type" => "option", "title" => "Ship search"},
        frame
      )

    assert out["suggested_parents"] == []
  end

  test "find_orphans suggest_parents excludes the orphan's own descendants",
       %{ws: ws, frame: frame, node: node} do
    _goal = node.("goal", "Speed up sync", %{})
    orphan = node.("decision", "Speed up sync with batching", %{})
    child = node.("option", "Speed up sync batch child", %{})
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: orphan.id, to_node_id: child.id})

    plain = call(FindOrphans, %{"workspace" => "cands"}, frame)
    refute Map.has_key?(hd(plain["orphans"]), "suggested_parents")

    out = call(FindOrphans, %{"workspace" => "cands", "suggest_parents" => true}, frame)
    assert out["suggested_for"] == 1
    [o] = out["orphans"]
    assert o["id"] == orphan.id
    ids = Enum.map(o["suggested_parents"], & &1["id"])
    refute child.id in ids
    assert length(ids) <= 3
    assert Repo.aggregate(Edge, :count) == 1
  end

  test "parent_fit follows the flow and refuses unknown types" do
    assert Candidates.parent_fit("outcome")["action"] == 1.0
    assert Candidates.parent_fit("goal") == %{}
    assert_raise ArgumentError, fn -> Candidates.parent_fit("nonsense") end
    assert Candidates.pool_bound() == 300
  end
end

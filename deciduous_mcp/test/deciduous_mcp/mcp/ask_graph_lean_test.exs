defmodule DeciduousMcp.MCP.AskGraphLeanTest do
  @moduledoc """
  ask_graph can return less: `detail: "lean"` shortens each result and
  `include_retired: false` leaves out retired notes (observations whose status
  is superseded or abandoned), never a decision or any other type, except on
  a history question. Without either argument, and without the server
  settings that change the defaults, the answer is what it was before, plus
  the two keys `detail` and `retired_omitted`.

  Through the whole router, so the argument schema is exercised.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  @ws "ask-lean"
  @hist_ws "ask-lean-history"
  @kinds_ws "ask-lean-kinds"

  @old_top_level ~w(depth_cap distinctive_unmatched_terms question result_count results
                    rounds route_terms routes scope search_terms stop_reason term_hits
                    truncated_rounds type_breakdown unmatched_terms)
  @old_hit_keys ~w(change_id connected_from connects_to created_at description id metadata
                   node_type path reached_by score status title)
  @old_edge_keys ~w(edge_type node_id node_type rationale title)

  setup do
    {:ok, ws} = Workspaces.find_or_create(@ws)

    long = String.duplicate("redis cache note ", 50)

    goal =
      node!(ws, "goal", "Redis cache for lookups", %{
        description: long,
        status: "active",
        metadata: %{"branch" => "main", "prompt" => "cache the lookups"}
      })

    # A pair of nodes on a subject of their own, so a question about it finds
    # exactly these two: a description of exactly 400 characters, and one edge.
    plan = node!(ws, "goal", "Zymurgy plan")

    boundary =
      node!(ws, "observation", "Zymurgy brewing notes", %{description: String.duplicate("b", 400)})

    edge!(ws, plan, boundary)

    # Seven edges out of the goal and six into it, so a cap of five shows.
    for n <- 1..7 do
      edge!(ws, goal, node!(ws, "observation", "Redis cache outgoing note #{n}"))
    end

    for n <- 1..6 do
      edge!(ws, node!(ws, "action", "Redis cache incoming step #{n}"), goal)
    end

    # The retired notes a lean read hides: observations, superseded or abandoned.
    superseded = node!(ws, "observation", "Redis old cache handoff note", %{status: "superseded"})
    abandoned = node!(ws, "observation", "Redis abandoned cache scratch", %{status: "abandoned"})

    # Retired, but not notes: a lean read keeps them, as it keeps a rejected option.
    old_decision = node!(ws, "decision", "Redis old cache design", %{status: "superseded"})
    dropped_action = node!(ws, "action", "Redis abandoned cache spike", %{status: "abandoned"})
    rejected = node!(ws, "option", "Redis rejected cache layout", %{status: "rejected"})

    edge!(ws, goal, superseded)
    edge!(ws, goal, abandoned)
    edge!(ws, goal, old_decision)
    edge!(ws, goal, dropped_action)
    edge!(ws, goal, rejected)

    %{
      client: McpClient.connect(),
      goal: goal,
      boundary: boundary,
      plan: plan,
      superseded: superseded,
      abandoned: abandoned,
      old_decision: old_decision,
      dropped_action: dropped_action,
      rejected: rejected
    }
  end

  defp node!(ws, type, title, extra \\ %{}) do
    {:ok, node} = Nodes.create_node(ws.id, Map.merge(%{node_type: type, title: title}, extra))
    node
  end

  defp edge!(ws, from, to) do
    {:ok, _} =
      Edges.create_edge(ws.id, %{
        from_node_id: from.id,
        to_node_id: to.id,
        edge_type: "leads_to",
        rationale: "because"
      })
  end

  defp ask(client, args, workspace \\ @ws) do
    McpClient.call!(
      client,
      "ask_graph",
      Map.merge(%{"question" => "redis cache", "workspace" => workspace}, args)
    )
  end

  defp hit(answer, node), do: Enum.find(answer["results"], &(&1["id"] == node.id))
  defp ids(answer), do: Enum.map(answer["results"], & &1["id"])

  # What every deployment default test restores afterwards.
  defp put_defaults(settings) do
    previous = Application.fetch_env(:deciduous_mcp, :read_defaults)
    Application.put_env(:deciduous_mcp, :read_defaults, settings)

    on_exit(fn ->
      case previous do
        {:ok, value} -> Application.put_env(:deciduous_mcp, :read_defaults, value)
        :error -> Application.delete_env(:deciduous_mcp, :read_defaults)
      end
    end)
  end

  test "defaults_unchanged", %{client: client, goal: goal} do
    answer = ask(client, %{})

    assert Enum.sort(Map.keys(answer)) ==
             Enum.sort(@old_top_level ++ ["detail", "retired_omitted"])

    assert answer["detail"] == "full"
    assert answer["retired_omitted"] == 0

    h = hit(answer, goal)
    assert Enum.sort(Map.keys(h) -- ["ranks"]) == @old_hit_keys
    assert String.length(h["description"]) == 50 * String.length("redis cache note ")
    refute Map.has_key?(h, "description_truncated")
    refute Map.has_key?(h, "connects_to_total")
    assert h["metadata"]["prompt"] == "cache the lookups"
    assert length(h["connects_to"]) == 12
    assert length(h["connected_from"]) == 6

    assert Enum.all?(
             h["connects_to"] ++ h["connected_from"],
             &(Enum.sort(Map.keys(&1)) == @old_edge_keys)
           )

    # Retired nodes are in it, as before.
    assert Enum.any?(answer["results"], &(&1["status"] == "superseded"))
    assert Enum.any?(answer["results"], &(&1["status"] == "abandoned"))

    # Saying the defaults out loud changes nothing.
    assert ask(client, %{"detail" => "full", "include_retired" => true}) == answer
  end

  test "lean_truncates_at_400", %{client: client, goal: goal, boundary: boundary, plan: plan} do
    answer = ask(client, %{"detail" => "lean"})
    assert answer["detail"] == "lean"

    h = hit(answer, goal)
    assert String.length(h["description"]) == 400
    assert h["description_truncated"] == true

    # Exactly 400 characters is not cut, and says nothing about it.
    about = ask(client, %{"detail" => "lean", "question" => "zymurgy"})
    b = hit(about, boundary)
    assert String.length(b["description"]) == 400
    refute Map.has_key?(b, "description_truncated")

    # Nodes that carry no description are untouched.
    p = hit(about, plan)
    assert p["description"] == nil
    refute Map.has_key?(p, "description_truncated")
  end

  test "lean_caps_edges_at_five_and_reports_totals", %{
    client: client,
    goal: goal,
    boundary: boundary
  } do
    answer = ask(client, %{"detail" => "lean"})
    h = hit(answer, goal)

    assert length(h["connects_to"]) == 5
    assert h["connects_to_total"] == 12
    assert length(h["connected_from"]) == 5
    assert h["connected_from_total"] == 6

    # Under the cap the totals are just the counts.
    about = ask(client, %{"detail" => "lean", "question" => "zymurgy"})
    b = hit(about, boundary)
    assert b["connects_to"] == [] and b["connects_to_total"] == 0
    assert length(b["connected_from"]) == 1 and b["connected_from_total"] == 1

    # No context asked, none given, in either mode.
    bare = ask(client, %{"detail" => "lean", "include_context" => false})
    h = hit(bare, goal)
    refute Map.has_key?(h, "connects_to")
    refute Map.has_key?(h, "connects_to_total")
  end

  test "lean_omits_metadata_and_ranks", %{client: client, goal: goal} do
    full = ask(client, %{})
    assert Enum.any?(full["results"], &Map.has_key?(&1, "ranks"))

    lean = ask(client, %{"detail" => "lean"})
    refute Enum.any?(lean["results"], &Map.has_key?(&1, "metadata"))
    refute Enum.any?(lean["results"], &Map.has_key?(&1, "ranks"))

    # What stays, stays: identity, status, how it was reached, the score.
    h = hit(lean, goal)
    f = hit(full, goal)

    for key <- ~w(id change_id node_type title status created_at reached_by path score) do
      assert h[key] == f[key], "#{key} changed in lean mode"
    end

    # And lean changes which nodes come back, and in what order, not at all.
    assert ids(lean) == ids(full)
  end

  test "retired_notes_dropped_when_include_retired_false", ctx do
    %{client: client, goal: goal, superseded: superseded, abandoned: abandoned} = ctx
    kept = ask(client, %{})
    answer = ask(client, %{"include_retired" => false})

    refute superseded.id in ids(answer)
    refute abandoned.id in ids(answer)
    assert answer["retired_omitted"] == 2
    assert answer["result_count"] == length(answer["results"])
    assert answer["result_count"] == kept["result_count"] - 2
    assert answer["type_breakdown"] |> Map.values() |> Enum.sum() == answer["result_count"]

    # A retired note is not listed under a live node either, and the lean
    # totals count what is left.
    titles = Enum.map(hit(answer, goal)["connects_to"], & &1["title"])
    refute "Redis old cache handoff note" in titles
    refute "Redis abandoned cache scratch" in titles
    assert length(hit(answer, goal)["connects_to"]) == 10

    lean = ask(client, %{"detail" => "lean", "include_retired" => false})
    assert hit(lean, goal)["connects_to_total"] == 10
  end

  test "retired_decisions_and_actions_are_kept", ctx do
    %{client: client, goal: goal, old_decision: old_decision, dropped_action: dropped} = ctx

    for args <- [%{"include_retired" => false}, %{"detail" => "lean", "include_retired" => false}] do
      answer = ask(client, args)

      assert old_decision.id in ids(answer)
      assert dropped.id in ids(answer)
      assert hit(answer, old_decision)["status"] == "superseded"
      assert hit(answer, dropped)["status"] == "abandoned"
    end

    # And they stay in the goal's edges (lean cuts those at five, so full here).
    titles =
      Enum.map(
        hit(ask(client, %{"include_retired" => false}), goal)["connects_to"],
        & &1["title"]
      )

    assert "Redis old cache design" in titles
    assert "Redis abandoned cache spike" in titles
  end

  test "only_retired_observations_are_hidden_whatever_the_other_type" do
    {:ok, ws} = Workspaces.find_or_create(@kinds_ws)

    # One retired node of every type, on a subject of its own.
    kept =
      for {type, status} <- [
            {"goal", "abandoned"},
            {"option", "superseded"},
            {"decision", "superseded"},
            {"action", "abandoned"},
            {"outcome", "abandoned"},
            {"revisit", "superseded"}
          ] do
        node!(ws, type, "Quokka plan #{type}", %{status: status})
      end

    note = node!(ws, "observation", "Quokka plan observation", %{status: "superseded"})
    live_note = node!(ws, "observation", "Quokka plan live observation", %{status: "completed"})

    client = McpClient.connect()
    question = %{"question" => "quokka plan"}

    lean = ask(client, Map.put(question, "include_retired", false), @kinds_ws)
    assert Enum.all?(kept, &(&1.id in ids(lean)))
    refute note.id in ids(lean)
    assert live_note.id in ids(lean)
    assert lean["retired_omitted"] == 1

    default = ask(client, question, @kinds_ws)
    assert note.id in ids(default)
    assert default["retired_omitted"] == 0
    assert default["result_count"] == lean["result_count"] + 1
  end

  test "history_question_keeps_retired" do
    {:ok, ws} = Workspaces.find_or_create(@hist_ws)
    goal = node!(ws, "goal", "Caching work for lookups", %{status: "active"})
    old = node!(ws, "observation", "Write-back caching handoff note", %{status: "superseded"})
    new = node!(ws, "decision", "Write-through caching", %{status: "completed"})
    edge!(ws, goal, old)
    edge!(ws, goal, new)

    client = McpClient.connect()

    answer =
      ask(
        client,
        %{"question" => "what was superseded in the caching work", "include_retired" => false},
        @hist_ws
      )

    assert "history" in Enum.map(answer["routes"], & &1["name"])
    assert old.id in ids(answer)
    assert answer["retired_omitted"] == 0

    # The same nodes asked about without a history cue lose the retired note.
    plain = ask(client, %{"question" => "caching", "include_retired" => false}, @hist_ws)
    refute old.id in ids(plain)
    assert new.id in ids(plain)
    assert plain["retired_omitted"] == 1
  end

  test "rejected_options_are_kept", %{client: client, rejected: rejected, goal: goal} do
    answer = ask(client, %{"include_retired" => false})

    assert rejected.id in ids(answer)

    assert "Redis rejected cache layout" in Enum.map(
             hit(answer, goal)["connects_to"],
             & &1["title"]
           )

    lean = ask(client, %{"detail" => "lean", "include_retired" => false})
    assert rejected.id in ids(lean)
  end

  test "deployment_default_lean_applies_without_argument", ctx do
    %{client: client, goal: goal, superseded: superseded} = ctx
    put_defaults(detail: "lean", hide_retired: true)

    answer = ask(client, %{})

    assert answer["detail"] == "lean"
    assert hit(answer, goal)["description_truncated"] == true
    refute Map.has_key?(hit(answer, goal), "metadata")
    refute superseded.id in ids(answer)
    assert answer["retired_omitted"] == 2
  end

  test "argument_overrides_deployment_default", ctx do
    %{client: client, goal: goal, superseded: superseded} = ctx
    put_defaults(detail: "lean", hide_retired: true)

    answer = ask(client, %{"detail" => "full", "include_retired" => true})

    assert answer["detail"] == "full"
    refute Map.has_key?(hit(answer, goal), "description_truncated")
    assert hit(answer, goal)["metadata"]["prompt"] == "cache the lookups"
    assert superseded.id in ids(answer)
    assert answer["retired_omitted"] == 0

    # A false that arrives is a value, not "nothing passed": the setting that
    # hides retired nodes still yields to include_retired: true above, and a
    # deployment that shows them yields to include_retired: false.
    put_defaults(detail: "full", hide_retired: false)
    shown = ask(client, %{"include_retired" => false})
    refute superseded.id in ids(shown)
    assert shown["retired_omitted"] == 2
  end
end

defmodule DeciduousMcp.MCP.ConsolidationReportTest do
  @moduledoc """
  consolidation_report, over the MCP path: what each section reports and
  what it leaves out, the caps and what they say they cut, the refusals,
  and that the call writes nothing to any table.
  """
  use DeciduousMcp.DataCase, async: false

  import Ecto.Query

  alias DeciduousMcp.Graph.{Edges, Nodes, Workspaces}
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.Node
  alias DeciduousMcp.Test.McpClient

  @ws "consolidation-ws"

  setup do
    {:ok, ws} = Workspaces.find_or_create(@ws)
    %{ws: ws, client: McpClient.connect()}
  end

  defp node(ws, type, title, attrs \\ %{}) do
    {:ok, n} = Nodes.create_node(ws.id, Map.merge(%{node_type: type, title: title}, attrs))
    n
  end

  defp link(ws, from, to) do
    {:ok, _} = Edges.create_edge(ws.id, %{from_node_id: from.id, to_node_id: to.id})
    :ok
  end

  defp age(n, days) do
    at = DateTime.add(DateTime.utc_now(), -days * 86_400, :second)
    Repo.update_all(from(x in Node, where: x.id == ^n.id), set: [inserted_at: at])
    n
  end

  defp report(client, args \\ %{}) do
    McpClient.call!(client, "consolidation_report", Map.put(args, "workspace", @ws))
  end

  defp pair_ids(finding), do: finding["nodes"] |> Enum.map(& &1["id"]) |> MapSet.new()

  defp finding_for(section, a, b) do
    Enum.find(section["findings"], &(pair_ids(&1) == MapSet.new([a.id, b.id])))
  end

  describe "duplicate_goals" do
    test "labels merge, keep_separate and uncertain with the evidence", %{ws: ws, client: c} do
      root = node(ws, "goal", "Unrelated root about billing exports")

      same1 = node(ws, "goal", "Build testing framework library from scratch")
      same2 = node(ws, "goal", "Build testing framework library from scratch")

      # pg_trgm scores this pair 1.0: it ignores "+" and "#".
      cpp = node(ws, "goal", "C++ Backend Deep Dive - Temper")
      csharp = node(ws, "goal", "C# Backend Deep Dive - Temper")

      parent = node(ws, "goal", "Add asciinema demo to GitHub Pages")
      sub = node(ws, "goal", "Add asciinema demo player to GitHub Pages")
      link(ws, parent, sub)

      d1 =
        node(ws, "goal", "Rate limit the public API", %{
          description: "token bucket per API key, 100 requests per minute, redis backed"
        })

      d2 =
        node(ws, "goal", "Rate limit the public API endpoints", %{
          description: "Legal asked us to cap scraping of profile pages; block by IP range"
        })

      r = report(c)["duplicate_goals"]

      merge = finding_for(r, same1, same2)
      assert merge["label"] == "merge"
      assert merge["evidence"]["title_similarity"] == 1.0
      assert [first | _] = merge["nodes"]
      assert first["id"] == same1.id, "the older goal is the one kept"
      assert merge["suggestion"] =~ "update_node #{same2.id} status=superseded"

      sub_finding = finding_for(r, cpp, csharp)
      assert sub_finding["label"] == "uncertain"
      assert sub_finding["evidence"]["title_similarity"] == 1.0
      assert sub_finding["evidence"]["words_only_in"][cpp.id] == ["c++"]
      assert sub_finding["evidence"]["words_only_in"][csharp.id] == ["c#"]

      nested = finding_for(r, parent, sub)
      assert nested["label"] == "keep_separate"
      assert nested["evidence"]["one_is_ancestor_of_other"]

      distinct = finding_for(r, d1, d2)
      assert distinct["label"] == "keep_separate"
      assert distinct["evidence"]["description_similarity"] < 0.2

      refute Enum.any?(r["findings"], &MapSet.member?(pair_ids(&1), root.id))
      assert r["pairs_above_threshold"] == 4
      assert r["truncated"] == false
    end

    test "a pair with the same verbatim prompt merges", %{ws: ws, client: c} do
      a =
        node(ws, "goal", "Add dark mode to the viewer", %{
          metadata: %{"prompt" => "add dark mode"}
        })

      b = node(ws, "goal", "Add dark mode to viewer", %{metadata: %{"prompt" => "add dark mode"}})

      f = finding_for(report(c)["duplicate_goals"], a, b)
      assert f["label"] == "merge"
      assert f["evidence"]["same_prompt"] == true
    end

    test "max_pairs caps the pairs examined and says how many it left", %{ws: ws, client: c} do
      for _ <- 1..3, do: node(ws, "goal", "Ship the release pipeline")

      r = report(c, %{"max_pairs" => 1})["duplicate_goals"]
      assert r["pairs_above_threshold"] == 3
      assert r["pairs_examined"] == 1
      assert length(r["findings"]) == 1
      assert r["pair_cap"] == 1
      assert r["truncated"] == true
      assert r["nodes_compared"] == 3
      assert r["pairs_compared"] == 3
    end

    test "max_nodes compares only the newest and says how many there were", %{ws: ws, client: c} do
      old = node(ws, "goal", "Ship the release pipeline") |> age(10)
      a = node(ws, "goal", "Ship the release pipeline")
      b = node(ws, "goal", "Ship the release pipeline")

      r = report(c, %{"max_nodes" => 2})["duplicate_goals"]
      assert r["nodes_total"] == 3
      assert r["nodes_compared"] == 2
      assert r["pairs_compared"] == 1
      assert [f] = r["findings"]
      assert pair_ids(f) == MapSet.new([a.id, b.id])
      refute finding_for(r, old, a)
    end

    test "deleted goals are not compared", %{ws: ws, client: c} do
      a = node(ws, "goal", "Ship the release pipeline")
      b = node(ws, "goal", "Ship the release pipeline")
      {:ok, _} = Nodes.delete_node(b.id)

      r = report(c)["duplicate_goals"]
      assert r["findings"] == []
      assert r["nodes_compared"] == 1
      refute finding_for(r, a, b)
    end
  end

  describe "competing_decisions" do
    setup %{ws: ws} do
      goal = node(ws, "goal", "Pick a job queue")
      option = node(ws, "option", "Use a Postgres-backed queue")
      link(ws, goal, option)
      %{goal: goal, option: option}
    end

    test "two similar live decisions under one option, no revisit: reported", ctx do
      %{ws: ws, client: c, option: option} = ctx
      d1 = node(ws, "decision", "Choose Oban for background jobs")
      d2 = node(ws, "decision", "Choose Oban for the background jobs")
      link(ws, option, d1)
      link(ws, option, d2)

      f = finding_for(report(c)["competing_decisions"], d1, d2)
      assert f, "expected the pair to be reported"
      assert f["evidence"]["shared_parents"] == [option.id]
      assert f["evidence"]["revisit_between"] == false
      assert f["suggestion"] =~ "add_node revisit with parent_id #{d1.id}"
    end

    test "sharing only a nearest goal ancestor is enough", ctx do
      %{ws: ws, client: c, goal: goal} = ctx
      o1 = node(ws, "option", "Redis")
      o2 = node(ws, "option", "RabbitMQ")
      link(ws, goal, o1)
      link(ws, goal, o2)
      d1 = node(ws, "decision", "Choose Oban for background jobs")
      d2 = node(ws, "decision", "Choose Oban for the background jobs")
      link(ws, o1, d1)
      link(ws, o2, d2)

      f = finding_for(report(c)["competing_decisions"], d1, d2)
      assert f["evidence"]["shared_parents"] == []
      assert f["evidence"]["shared_goal_ancestors"] == [goal.id]
    end

    test "a revisit between them, a superseded one, or no shared ancestry: not reported", ctx do
      %{ws: ws, client: c, option: option} = ctx

      # A -> revisit -> B
      a = node(ws, "decision", "Choose Oban for background jobs")
      b = node(ws, "decision", "Choose Oban for the background jobs")
      rv = node(ws, "revisit", "Reconsidering the job library")
      link(ws, option, a)
      link(ws, option, b)
      link(ws, a, rv)
      link(ws, rv, b)

      # superseded: already resolved
      s1 = node(ws, "decision", "Use GenStage pipelines for ingest")
      s2 = node(ws, "decision", "Use GenStage pipelines for the ingest", %{status: "superseded"})
      link(ws, option, s1)
      link(ws, option, s2)

      # similar but in unrelated trees
      g2 = node(ws, "goal", "Something else entirely")
      u1 = node(ws, "decision", "Adopt Broadway for event consumers")
      u2 = node(ws, "decision", "Adopt Broadway for the event consumers")
      link(ws, option, u1)
      link(ws, g2, u2)

      r = report(c)["competing_decisions"]
      refute finding_for(r, a, b)
      refute finding_for(r, s1, s2)
      refute finding_for(r, u1, u2)
      assert r["findings"] == []
      # a/b and u1/u2 were examined; s2 was never a candidate
      assert r["pairs_examined"] == 2
    end
  end

  describe "stale_actions" do
    test "old open actions with no outcome reachable before the next action", %{ws: ws, client: c} do
      g = node(ws, "goal", "Speed up the importer")
      done = node(ws, "action", "Batch inserts") |> age(30)
      out = node(ws, "outcome", "3x faster")
      link(ws, g, done)
      link(ws, done, out)

      via_obs = node(ws, "action", "Profile the parser") |> age(30)
      obs = node(ws, "observation", "Parser dominates")
      out2 = node(ws, "outcome", "Found the hot loop")
      link(ws, g, via_obs)
      link(ws, via_obs, obs)
      link(ws, obs, out2)

      # Its only outcome belongs to the next action, not to it.
      handed_off = node(ws, "action", "Rewrite the tokenizer") |> age(40)
      next = node(ws, "action", "Tune the tokenizer")
      out3 = node(ws, "outcome", "Tokenizer 2x")
      link(ws, g, handed_off)
      link(ws, handed_off, next)
      link(ws, next, out3)

      fresh = node(ws, "action", "Try a streaming parser") |> age(2)
      closed = node(ws, "action", "Old spike", %{status: "completed"}) |> age(90)
      for n <- [fresh, closed], do: link(ws, g, n)

      r = report(c, %{"stale_days" => 14})["stale_actions"]
      ids = Enum.map(r["findings"], & &1["id"])

      assert handed_off.id in ids
      refute done.id in ids
      refute via_obs.id in ids
      refute fresh.id in ids
      refute closed.id in ids
      # `next` is new, so not stale either
      assert ids == [handed_off.id]
      assert [%{"age_days" => 40}] = r["findings"]
    end

    test "max_items lists the oldest and reports the total", %{ws: ws, client: c} do
      for d <- [20, 30, 40], do: node(ws, "action", "Loose end #{d}") |> age(d)

      r = report(c, %{"max_items" => 2})["stale_actions"]
      assert r["total"] == 3
      assert r["listed"] == 2
      assert r["truncated"] == true
      assert Enum.map(r["findings"], & &1["title"]) == ["Loose end 40", "Loose end 30"]
    end
  end

  describe "parentless" do
    test "actions and outcomes with no live parent; goals and linked nodes are not", %{
      ws: ws,
      client: c
    } do
      g = node(ws, "goal", "A goal")
      linked = node(ws, "action", "Linked action")
      link(ws, g, linked)
      loose_out = node(ws, "outcome", "Loose outcome")
      gone = node(ws, "action", "Deleted parent")
      stranded = node(ws, "outcome", "Stranded outcome")
      link(ws, gone, stranded)
      {:ok, _} = Nodes.delete_node(gone.id)
      _obs = node(ws, "observation", "Loose observation")

      r = report(c)["parentless"]
      ids = r["findings"] |> Enum.map(& &1["id"]) |> MapSet.new()
      assert ids == MapSet.new([loose_out.id, stranded.id])
      assert Enum.all?(r["findings"], &(&1["suggestion"] =~ "the action that produced it"))
      assert r["total"] == 2
    end
  end

  test "writes nothing to any table", %{ws: ws, client: c} do
    g = node(ws, "goal", "Ship the release pipeline")
    node(ws, "goal", "Ship the release pipeline")
    a = node(ws, "action", "Loose end") |> age(30)
    link(ws, g, a)

    %{rows: tables} =
      Repo.query!(
        "SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'schema_migrations' ORDER BY 1"
      )

    counts = fn ->
      for [t] <- tables, into: %{} do
        %{rows: [[n]]} = Repo.query!(~s{SELECT count(*) FROM "#{t}"})
        {t, n}
      end
    end

    before = counts.()
    r = report(c)
    assert r["read_only"] == true
    assert length(r["duplicate_goals"]["findings"]) == 1
    assert counts.() == before
  end

  describe "refusals" do
    test "every workspace at once is refused", %{client: c} do
      assert {:error, message} =
               McpClient.call(c, "consolidation_report", %{"workspace" => "*"})

      assert message =~ "one workspace"
    end

    test "an unknown workspace is refused and not created", %{client: c} do
      assert {:error, message} =
               McpClient.call(c, "consolidation_report", %{"workspace" => "never-written"})

      assert message =~ ~s(no workspace named "never-written")
      assert {:error, _} = Workspaces.get_by_name("never-written")
    end

    test "out-of-range settings are refused, not clamped", %{client: c} do
      for {k, v} <- [{"similarity", 0.1}, {"max_pairs", 0}, {"max_nodes", 100_000}] do
        assert {:error, message} =
                 McpClient.call(c, "consolidation_report", %{"workspace" => @ws, k => v})

        assert message =~ k, "#{k}=#{v}: #{message}"
        refute message =~ "nothing was written"
      end
    end

    test "the module refuses what the schema would have", %{ws: ws} do
      assert {:error, msg} = DeciduousMcp.Graph.Consolidation.report(ws.id, %{stale_days: 1.5})
      assert msg =~ "stale_days must be an integer"
    end
  end

  test "is listed by tools/list" do
    names = DeciduousMcp.MCP.Tools.all_definitions() |> Enum.map(& &1.name)
    assert "consolidation_report" in names
  end
end

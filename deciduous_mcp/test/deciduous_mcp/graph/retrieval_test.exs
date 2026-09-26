defmodule DeciduousMcp.Graph.RetrievalTest do
  @moduledoc """
  Closed-loop retrieval behind ask_graph: fused anchors, routed expansion,
  the stop reason, unmatched terms, and loading that does not grow with the
  number of edges.

  Fixture, one workspace:

      goal "Pick a cache for the session store"
        -> decision "Choose the session cache backend" (superseded)
             -chosen->   option "Redis for session caching"
             -rejected-> option "Memcached for session caching"
             -> revisit "Reconsidering after eviction storms"
                  -> decision "Move sessions to Postgres"
      action "Plan the migration of billing"   (no edges)
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Edges, Nodes, Related, Retrieval}
  alias DeciduousMcp.MCP.Tools.AskGraph

  setup do
    ws = create_test_workspace("retrieval-ws")

    node = fn type, title, extra ->
      {:ok, n} = Nodes.create_node(ws.id, Map.merge(%{node_type: type, title: title}, extra))
      n
    end

    link = fn from, to, type ->
      {:ok, _} =
        Edges.create_edge(ws.id, %{from_node_id: from.id, to_node_id: to.id, edge_type: type})
    end

    goal = node.("goal", "Pick a cache for the session store", %{})
    decision = node.("decision", "Choose the session cache backend", %{status: "superseded"})
    redis = node.("option", "Redis for session caching", %{})
    memcached = node.("option", "Memcached for session caching", %{})
    revisit = node.("revisit", "Reconsidering after eviction storms", %{})
    postgres = node.("decision", "Move sessions to Postgres", %{})
    billing = node.("action", "Plan the migration of billing", %{})

    link.(goal, decision, "leads_to")
    link.(decision, redis, "chosen")
    link.(decision, memcached, "rejected")
    link.(decision, revisit, "leads_to")
    link.(revisit, postgres, "leads_to")

    %{
      ws: ws,
      node: node,
      link: link,
      goal: goal,
      decision: decision,
      redis: redis,
      memcached: memcached,
      revisit: revisit,
      postgres: postgres,
      billing: billing
    }
  end

  defp all_hits(r), do: r.anchors ++ r.expanded
  defp hit(r, node), do: Enum.find(all_hits(r), &(&1.node.id == node.id))

  test "a why question follows the chosen edge from the anchor, and says so", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "why did we choose redis?")

    assert r.terms == ["redis"]
    assert "why" not in r.terms and "choose" in r.route_terms
    assert Enum.map(r.anchors, & &1.node.id) == [ctx.redis.id]

    d = hit(r, ctx.decision)
    assert d.reached_by == "rationale"

    assert d.path == [
             %{from: ctx.redis.id, edge_type: "chosen", direction: "in", route: "rationale"}
           ]

    assert Enum.map(r.routes, & &1.name) == ["rationale", "context"]
    assert r.stop_reason == "evidence_sufficient"
  end

  test "full text catches a stemmed form that ILIKE misses", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "migrations")

    [a] = r.anchors
    assert a.node.id == ctx.billing.id
    # "%migrations%" is not in "Plan the migration of billing"; only the
    # stemmed tsvector matches.
    assert a.ranks == %{fts: 1}
    assert r.unmatched_terms == []
  end

  test "a term nothing contains is reported, and no type list stands in for it", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "what did we decide about kubernetes")

    assert r.terms == ["kubernetes"]
    assert r.unmatched_terms == ["kubernetes"]
    assert r.term_hits == %{"kubernetes" => 0}
    assert all_hits(r) == []
    assert r.stop_reason == "no_anchors"

    {:ok, r} = Retrieval.run(ctx.ws.id, "redis kubernetes")
    assert r.unmatched_terms == ["kubernetes"]
    assert hit(r, ctx.redis)
  end

  test "a structural question with no content words is answered by type and status", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "what decisions are still pending?")

    assert r.terms == []
    assert Enum.map(r.anchors, & &1.node.id) == [ctx.postgres.id]
    assert hd(r.anchors).ranks == %{type_hint: 1}
  end

  test "a history question gets the deeper cap and walks to the revisit", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "trace the history of the session cache backend")

    assert r.depth_cap == 4
    assert "history" in Enum.map(r.routes, & &1.name)

    rv = hit(r, ctx.revisit)
    assert rv.reached_by == "history"
    assert List.last(rv.path).edge_type == "leads_to"
  end

  test "a revisit found on a history question is followed to its replacement", ctx do
    {:ok, r} = Retrieval.run(ctx.ws.id, "what replaced the cache backend")

    pg = hit(r, ctx.postgres)
    assert pg, inspect(Enum.map(all_hits(r), & &1.node.title))
    assert pg.reached_by == "history"

    assert [
             %{from: from1, edge_type: "leads_to", direction: "out"},
             %{from: from2, edge_type: "leads_to", direction: "out"}
           ] = pg.path

    assert {from1, from2} == {ctx.decision.id, ctx.revisit.id}
    assert r.rounds == 2
  end

  test "ordered/1 puts expanded nodes after the five best anchors" do
    a = for i <- 1..7, do: %{node: %{id: "a#{i}"}}
    e = [%{node: %{id: "e1"}}]

    assert Enum.map(Retrieval.ordered(%{anchors: a, expanded: e}), & &1.node.id) ==
             ~w(a1 a2 a3 a4 a5 e1 a6 a7)
  end

  test "blocked questions follow requires/blocks, and nothing else through that route", ctx do
    blocker = ctx.node.("action", "Provision the cluster", %{})
    ctx.link.(blocker, ctx.redis, "blocks")

    {:ok, r} = Retrieval.run(ctx.ws.id, "what is blocking redis caching")

    b = hit(r, blocker)
    assert b, inspect(Enum.map(all_hits(r), & &1.node.title))
    assert b.reached_by == "dependency"
    assert [%{edge_type: "blocks", direction: "in"}] = b.path
  end

  test "the budget is split one each, then by weight, largest remainder" do
    routes = [%{name: "a", weight: 1.0}, %{name: "b", weight: 1.0}, %{name: "c", weight: 0.25}]
    # 21 left after one each: 9.33, 9.33, 2.33; the one leftover goes to the
    # first of the tied remainders.
    assert Retrieval.allocate(routes, 24) == %{"a" => 11, "b" => 10, "c" => 3}
    assert Retrieval.allocate(routes, 3) == %{"a" => 1, "b" => 1, "c" => 1}

    assert_raise ArgumentError, ~r/smaller than the 3 active routes/, fn ->
      Retrieval.allocate(routes, 2)
    end
  end

  test "unknown scope, bad budget and malformed routes are errors, not fallbacks", ctx do
    assert {:error, msg} = Retrieval.run(ctx.ws.id, "redis", scope: "everything")
    assert msg =~ "unknown scope"
    assert {:error, _} = Retrieval.run(ctx.ws.id, "redis", budget: 0)
    assert {:error, msg} = Retrieval.run(ctx.ws.id, "redis", extra_routes: [%{name: "x"}])
    assert msg =~ "invalid route"
  end

  describe "scope applies to every route's neighbours" do
    # An extra route supplies neighbours through its own expand/3, not
    # through the edge query that carries the scope's WHERE. Before the
    # scope was applied in one place, ask_graph scope=decisions returned an
    # action and scope=active a completed node whenever the question had a
    # cue word ("files") that switched shared_identifier on.
    setup ctx do
      meta = %{metadata: %{"files" => ["src/store.rs", "src/evict.rs"]}}
      d = ctx.node.("decision", "Choose the lookup cache", meta)
      a = ctx.node.("action", "Tune eviction thresholds", Map.put(meta, :status, "completed"))
      %{d: d, a: a}
    end

    test "Related's shared_identifier route under scope=decisions and scope=active", ctx do
      q = "which files did the lookup cache touch"

      {:ok, all} = Retrieval.run(ctx.ws.id, q, extra_routes: [Related.route()])
      assert hit(all, ctx.a).reached_by == "shared_identifier"

      for scope <- ["decisions", "active"] do
        {:ok, r} = Retrieval.run(ctx.ws.id, q, scope: scope, extra_routes: [Related.route()])
        assert hit(r, ctx.d), "#{scope}: the decision anchor is in scope"
        refute hit(r, ctx.a), "#{scope}: completed action reached by #{inspect(hit(r, ctx.a))}"
      end
    end

    test "any extra route, not only Related's", ctx do
      stub = %{
        name: "stub",
        # A cue route, so expansion runs until it has admitted something.
        cues: ~w(stubcue),
        weight: 1.0,
        expand: fn _ws, frontier, _visited ->
          for from <- frontier, do: {from, ctx.a, 1.0, %{}}
        end
      }

      {:ok, r} =
        Retrieval.run(ctx.ws.id, "stubcue lookup cache", scope: "decisions", extra_routes: [stub])

      assert hit(r, ctx.d)
      refute hit(r, ctx.a)

      {:ok, r} = Retrieval.run(ctx.ws.id, "stubcue lookup cache", extra_routes: [stub])
      assert hit(r, ctx.a)
    end
  end

  test "an extra route's expand/3 is budgeted and scored like an edge route", ctx do
    loner = ctx.node.("observation", "Session cache sizing notes", %{})

    route = %{
      name: "shared_identifier",
      cues: ~w(file files),
      weight: 1.0,
      expand: fn _ws, frontier, _visited ->
        for from <- frontier,
            from == ctx.redis.id,
            do: {from, loner, 1.0, %{shared_files: ["cache.ex"]}}
      end
    }

    {:ok, r} = Retrieval.run(ctx.ws.id, "which files touch redis", extra_routes: [route])

    l = hit(r, loner)
    assert l.reached_by == "shared_identifier"
    assert [%{from: from, route: "shared_identifier", shared_files: ["cache.ex"]}] = l.path
    assert from == ctx.redis.id
  end

  test "extract_terms keeps paths whole" do
    assert Retrieval.extract_terms("what changed in src/db.rs?") == ["changed", "src/db.rs"]
    assert Retrieval.extract_terms("see .deciduous/sync.") == ["see", "deciduous/sync"]
  end

  test "ask_graph issues the same number of queries for 3 neighbours as for 40", ctx do
    frame = %Hermes.Server.Frame{private: %{session_id: "retr"}, assigns: %{}}

    count = fn question ->
      ref = make_ref()
      me = self()

      :telemetry.attach(
        "count-#{inspect(ref)}",
        [:deciduous_mcp, :repo, :query],
        fn _, _, _, _ -> send(me, {ref, :q}) end,
        nil
      )

      {:ok, json} =
        AskGraph.call(%{
          arguments: %{"workspace" => "retrieval-ws", "question" => question},
          server: frame
        })

      :telemetry.detach("count-#{inspect(ref)}")

      n =
        Stream.repeatedly(fn ->
          receive do
            {^ref, :q} -> 1
          after
            0 -> nil
          end
        end)

      {Jason.decode!(json), n |> Enum.take_while(& &1) |> length()}
    end

    hub = ctx.node.("goal", "zanzibar hub", %{})
    for i <- 1..3, do: ctx.link.(hub, ctx.node.("action", "spoke #{i}", %{}), "leads_to")
    {small, q_small} = count.("why zanzibar")

    for i <- 4..40, do: ctx.link.(hub, ctx.node.("action", "spoke #{i}", %{}), "leads_to")
    {big, q_big} = count.("why zanzibar")

    assert length(hd(small["results"])["connects_to"]) == 3
    assert length(hd(big["results"])["connects_to"]) == 40
    assert q_small == q_big, "queries: #{q_small} for 3 edges, #{q_big} for 40"
  end
end

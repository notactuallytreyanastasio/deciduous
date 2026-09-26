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
    assert r.stop_reason == "distinctive_term_unmatched"

    {:ok, r} = Retrieval.run(ctx.ws.id, "redis kubernetes")
    assert r.unmatched_terms == ["kubernetes"]
    assert hit(r, ctx.redis)
  end

  describe "a distinctive word the graph never recorded stops retrieval" do
    test "an uncommon word: no expansion, at most three anchors", ctx do
      # "session" matches four nodes and the chain behind them; the
      # question is about websockets, which nothing mentions.
      {:ok, r} = Retrieval.run(ctx.ws.id, "why did we pick a session cache for websocket clients")

      assert "websocket" in r.unmatched_terms
      assert r.distinctive_unmatched_terms == ["websocket"]
      assert r.stop_reason == "distinctive_term_unmatched"
      assert r.expanded == []
      assert r.rounds == 0
      assert length(r.anchors) in 1..3
      assert Enum.all?(r.routes, &(&1.spent == 0))

      # The same question without the absent word expands as before.
      {:ok, r} = Retrieval.run(ctx.ws.id, "why did we pick a session cache")
      assert r.distinctive_unmatched_terms == []
      assert r.expanded != []
    end

    test "a common English word nothing contains does not stop it", ctx do
      # "algorithm" and "trying" are in no node; they are ordinary words, so
      # their absence says nothing about the subject.
      {:ok, r} =
        Retrieval.run(ctx.ws.id, "why the session cache algorithm we were trying")

      assert "algorithm" in r.unmatched_terms
      assert "trying" in r.unmatched_terms
      assert r.distinctive_unmatched_terms == []
      refute r.stop_reason == "distinctive_term_unmatched"
      assert r.expanded != []
    end

    test "a common word written as a name is distinctive", ctx do
      # "stripe" is common English; "Stripe" mid-sentence is a name.
      {:ok, r} = Retrieval.run(ctx.ws.id, "Which session cache does Stripe use?")
      assert r.distinctive_unmatched_terms == ["stripe"]
      assert r.stop_reason == "distinctive_term_unmatched"

      # Capitalised as the first word of a sentence is not a name.
      {:ok, r} = Retrieval.run(ctx.ws.id, "Stripe the session cache? Why")
      assert r.distinctive_unmatched_terms == []

      # An inner capital or a digit is, wherever it stands.
      {:ok, r} = Retrieval.run(ctx.ws.id, "IPv6 session cache")
      assert r.distinctive_unmatched_terms == ["ipv6"]
    end

    test "a matched rare word is not a stop, however rare", ctx do
      # "memcached" is in one node and in no English word list.
      {:ok, r} = Retrieval.run(ctx.ws.id, "why was memcached for the session cache rejected")
      assert r.unmatched_terms == []
      assert r.distinctive_unmatched_terms == []
      assert hit(r, ctx.memcached)
    end

    test "ask_graph reports it", _ctx do
      {:ok, json} =
        AskGraph.call(%{
          arguments: %{
            "workspace" => "retrieval-ws",
            "question" => "session cache for websocket clients"
          },
          server: %Hermes.Server.Frame{private: %{session_id: "retr"}, assigns: %{}}
        })

      answer = Jason.decode!(json)
      assert answer["stop_reason"] == "distinctive_term_unmatched"
      assert answer["distinctive_unmatched_terms"] == ["websocket"]
      assert answer["result_count"] <= 3
    end
  end

  test "past eight terms, uncommon words are kept before common ones" do
    terms =
      Retrieval.extract_terms(
        "What message queue sits between the API and the background workers, Kafka or RabbitMQ?"
      )

    assert length(terms) == 8
    assert "kafka" in terms
    assert "rabbitmq" in terms
    # Common words go first ("workers"), and the rest stay in question order.
    refute "workers" in terms
    assert Enum.take(terms, 2) == ["message", "queue"]
    assert List.last(terms) == "rabbitmq"
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

  test "a path in the question switches shared_identifier on, with no cue word", ctx do
    # The route's cues listed ".rs" and ".ex", but question words are split
    # on anything that is not a word character or "-", so "db.rs" arrived
    # as "db" and "rs" and an extension cue could never match.
    active = fn q ->
      {:ok, r} = Retrieval.run(ctx.ws.id, q, extra_routes: [Related.route()])
      Enum.map(r.routes, & &1.name)
    end

    assert "shared_identifier" in active.("what did we decide about src/db.rs")
    assert "shared_identifier" in active.("anything on CLAUDE.md?")
    assert "shared_identifier" in active.("notes on lib/api/")
    refute "shared_identifier" in active.("why did we pick postgres")
    refute "shared_identifier" in active.("see e.g. the end")
  end

  describe "one shared file is enough, unless the file is a hub" do
    # Related gave one shared path usefulness 0.5, and with no question
    # word in the neighbour Retrieval scored that (0.5 + 0.5) / 4.5 = 0.222,
    # under its 0.25 threshold: one shared file never admitted anything.
    setup ctx do
      a =
        ctx.node.("action", "Put a redis cache in front of the lookup", %{
          metadata: %{"files" => ["src/lookup.rs"]}
        })

      # Same file, spelled so that no ILIKE on "src/lookup.rs" finds it: it
      # can only be reached as the same path after normalisation.
      b =
        ctx.node.("action", "Tune eviction thresholds", %{
          metadata: %{"files" => ["src//lookup.rs"]}
        })

      %{a: a, b: b}
    end

    defp via_files(ctx, q) do
      {:ok, r} = Retrieval.run(ctx.ws.id, q, extra_routes: [Related.route()])
      hit(r, ctx.b)
    end

    test "a node sharing the exact file the question names is admitted", ctx do
      assert %{reached_by: "shared_identifier"} = via_files(ctx, "what happened to src/lookup.rs")
    end

    test "a node sharing one file with an anchor, when files are asked about", ctx do
      assert %{reached_by: "shared_identifier"} =
               via_files(ctx, "which files did the redis cache touch")
    end

    test "a file more than 20 nodes name is a hub: sharing it alone admits nothing", ctx do
      for i <- 1..20,
          do:
            ctx.node.("action", "hub toucher #{i}", %{metadata: %{"files" => ["src/lookup.rs"]}})

      refute via_files(ctx, "which files did the redis cache touch")
    end
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

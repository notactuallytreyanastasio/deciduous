defmodule DeciduousMcp.Graph.RelatedTest do
  @moduledoc """
  Relations derived from exact shared identifiers (metadata.files and
  metadata.commit), Jev-Mem section 3.2. The shapes tested here are the ones
  found in the dev database: 7/9/40-character hashes of one commit, literal
  `HEAD~n`, directory entries ending in `/`.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Related}
  alias DeciduousMcp.Schema.{Edge, Node}
  alias DeciduousMcp.Test.McpClient

  setup do
    ws = create_test_workspace("rel-ws")
    other = create_test_workspace("rel-other")
    %{wid: ws.id, other: other.id}
  end

  defp node(wid, title, meta, type \\ "action") do
    {:ok, n} = Nodes.create_node(wid, %{node_type: type, title: title, metadata: meta})
    n
  end

  describe "identity" do
    test "normalize_path drops ./, collapses // and /./, keeps case and a leading /" do
      assert Related.normalize_path("./src/a.rs") == "src/a.rs"
      assert Related.normalize_path("././src//a/./b.rs ") == "src/a/b.rs"
      assert Related.normalize_path("Src/A.rs") == "Src/A.rs"
      assert Related.normalize_path("/temper/docs/why.md") == "/temper/docs/why.md"
      assert Related.normalize_path("lib/") == "lib/"
      assert_raise ArgumentError, fn -> Related.normalize_path("  ") end
      assert_raise ArgumentError, fn -> Related.normalize_path(nil) end
    end

    test "commit_key accepts 7-40 hex, refuses HEAD refs and short or non-hex strings" do
      assert Related.commit_key("ABC1234") == {:ok, "abc1234"}
      assert {:ok, _} = Related.commit_key(String.duplicate("a", 40))
      assert {:error, "commit \"HEAD~2\" is a ref" <> _} = Related.commit_key("HEAD~2")
      assert {:error, _} = Related.commit_key("abc12")
      assert {:error, _} = Related.commit_key("zzzzzzz")
      assert {:error, _} = Related.commit_key(1_234_567)
    end
  end

  describe "related/2" do
    test "ranks by shared paths, counts the same commit across hash lengths", %{wid: wid} do
      me =
        node(wid, "me", %{
          "files" => ["./src/a.rs", "src/b.rs", "src/c.rs"],
          "commit" => "ebee67a"
        })

      two = node(wid, "two paths", %{"files" => ["src/a.rs", "src/b.rs"]})
      one = node(wid, "one path", %{"files" => ["src//c.rs"]})
      sha = node(wid, "same commit", %{"commit" => "ebee67a174abb159147cf680bd7002dc48eea388"})
      _none = node(wid, "unrelated", %{"files" => ["src/z.rs"], "commit" => "1234567"})

      {:ok, %{related: rels, commit_ignored: nil}} = Related.related(me)

      assert Enum.map(rels, & &1.title) == ["two paths", "same commit", "one path"]
      assert hd(rels).id == two.id and hd(rels).score == 2
      assert hd(rels).shared_files == ["src/a.rs", "src/b.rs"]

      s = Enum.find(rels, &(&1.id == sha.id))
      assert s.shared_commit == "ebee67a174abb159147cf680bd7002dc48eea388"
      assert s.shared_files == [] and s.score == 2

      assert Enum.find(rels, &(&1.id == one.id)).shared_files == ["src/c.rs"]
    end

    test "rarer shared path wins a tie", %{wid: wid} do
      me = node(wid, "me", %{"files" => ["hub.rs", "rare.rs"]})
      hub = node(wid, "hub sharer", %{"files" => ["hub.rs"]})
      rare = node(wid, "rare sharer", %{"files" => ["rare.rs"]})
      for i <- 1..3, do: node(wid, "hub #{i}", %{"files" => ["hub.rs"]})

      {:ok, %{related: [first | _] = rels}} = Related.related(me)
      assert first.id == rare.id
      assert Enum.any?(rels, &(&1.id == hub.id))
    end

    test "HEAD refs relate to nothing and the reason is returned", %{wid: wid} do
      me = node(wid, "me", %{"commit" => "HEAD~1"})
      _other = node(wid, "other", %{"commit" => "HEAD~1"})

      assert {:ok, %{related: [], commit_ignored: "commit \"HEAD~1\" is a ref" <> _}} =
               Related.related(me)
    end

    test "deleted nodes and other workspaces never appear", %{wid: wid, other: other} do
      me = node(wid, "me", %{"files" => ["x.rs"]})
      gone = node(wid, "gone", %{"files" => ["x.rs"]})
      {:ok, _} = Nodes.delete_node(gone.id)
      _elsewhere = node(other, "elsewhere", %{"files" => ["x.rs"]})

      assert {:ok, %{related: []}} = Related.related(me)
      assert {:error, "Node " <> _} = Related.related(gone.id)
    end

    test "a node whose own files are not a list of paths is an error, not an empty list", %{
      wid: wid
    } do
      me = node(wid, "me", %{})
      {:ok, _} = Repo.update(Ecto.Changeset.change(me, metadata: %{"files" => "a.rs,b.rs"}))

      assert {:error, "metadata.files is not a list of paths" <> _} = Related.related(me.id)
    end

    test "nothing is written: no edge exists after a read", %{wid: wid} do
      me = node(wid, "me", %{"files" => ["x.rs"]})
      _ = node(wid, "peer", %{"files" => ["x.rs"]})
      {:ok, %{related: [_]}} = Related.related(me)
      assert Repo.aggregate(Edge, :count) == 0
    end
  end

  describe "expand/3 and route/1" do
    test "one entry per live frontier node, visited excluded", %{wid: wid} do
      a = node(wid, "a", %{"files" => ["x.rs"]})
      b = node(wid, "b", %{"files" => ["x.rs", "y.rs"]})
      c = node(wid, "c", %{"files" => ["y.rs"]})
      bare = node(wid, "bare", %{})

      m = Related.expand(wid, [a.id, bare.id, Ecto.UUID.generate()], exclude: [c.id])
      assert Map.keys(m) |> Enum.sort() == Enum.sort([a.id, bare.id])
      assert Enum.map(m[a.id], & &1.id) == [b.id]
      assert m[bare.id] == []

      m = Related.expand(wid, [b.id], exclude: [a.id])
      assert Enum.map(m[b.id], & &1.id) == [c.id]

      assert_raise ArgumentError, fn -> Related.expand(:global, [a.id]) end
    end

    test "route expands in the Retrieval shape, also under :global", %{wid: wid, other: other} do
      a = node(wid, "a", %{"files" => ["x.rs"]})
      b = node(wid, "b", %{"files" => ["x.rs"]})
      oa = node(other, "oa", %{"files" => ["x.rs"]})
      ob = node(other, "ob", %{"files" => ["x.rs"]})

      %{name: "shared_identifier", cues: cues, expand: expand} = Related.route()
      assert "file" in cues

      assert [{from, %Node{id: to}, 0.5, %{shared_files: ["x.rs"], shared_commit: nil}}] =
               expand.(wid, [a.id], MapSet.new())

      assert {from, to} == {a.id, b.id}

      pairs = expand.(:global, [a.id, oa.id], []) |> Enum.map(fn {f, n, _, _} -> {f, n.id} end)
      assert Enum.sort(pairs) == Enum.sort([{a.id, b.id}, {oa.id, ob.id}])

      assert expand.(wid, [a.id], [b.id]) == []
    end
  end

  describe "nodes_for_file/2" do
    test "exact first, then directory containment either way", %{wid: wid} do
      exact = node(wid, "exact", %{"files" => ["lib/a/b.ex"]})
      dir = node(wid, "dir", %{"files" => ["lib/"]})
      _other = node(wid, "other", %{"files" => ["lib/c.ex"]})

      {:ok, hits} = Related.nodes_for_file(wid, "./lib/a/b.ex")

      assert Enum.map(hits, &{&1.node.id, &1.match}) == [
               {exact.id, :exact},
               {dir.id, :dir_contains}
             ]

      {:ok, hits} = Related.nodes_for_file(wid, "lib/a/")
      assert Enum.map(hits, &{&1.node.id, &1.match}) == [{exact.id, :under_dir}]

      assert {:error, _} = Related.nodes_for_file(:global, "lib/a/b.ex")
      assert {:error, "file must be a non-empty path" <> _} = Related.nodes_for_file(wid, " ")
    end
  end

  describe "decisions_above/2" do
    test "stops at the first decision on each path, nearest first, through real edges only",
         %{wid: wid} do
      alias DeciduousMcp.Graph.Edges

      link = fn a, b ->
        {:ok, _} =
          Edges.create_edge(wid, %{from_node_id: a.id, to_node_id: b.id, edge_type: "leads_to"})
      end

      goal = node(wid, "goal", %{}, "goal")
      far = node(wid, "far decision", %{}, "decision")
      opt = node(wid, "option", %{}, "option")
      near = node(wid, "near decision", %{}, "decision")
      act = node(wid, "act", %{"files" => ["x.rs"]})
      act2 = node(wid, "act2", %{"files" => ["x.rs"]})
      mid = node(wid, "mid", %{})
      dead = node(wid, "dead decision", %{}, "decision")

      link.(goal, far)
      link.(far, opt)
      link.(opt, near)
      link.(near, act)
      link.(opt, mid)
      link.(mid, act2)
      link.(dead, act2)
      {:ok, _} = Nodes.delete_node(dead.id)

      assert [%{decision: %{id: n}, via: [a1]}, %{decision: %{id: f}, via: [a2]}] =
               Related.decisions_above([act.id, act2.id])

      assert {n, a1} == {near.id, act.id}
      # act2 -> mid -> option -> far decision: three levels up.
      assert {f, a2} == {far.id, act2.id}
      assert Related.decisions_above([act2.id], 2) == []
    end
  end

  describe "through MCP" do
    setup %{wid: _} do
      client = McpClient.connect()

      add = fn title, type, extra ->
        McpClient.call!(
          client,
          "add_node",
          Map.merge(%{"node_type" => type, "title" => title, "workspace" => "rel-ws"}, extra)
        )["id"]
      end

      %{client: client, add: add}
    end

    test "show_node carries a related section read off files and commit", %{client: c, add: add} do
      me = add.("me", "action", %{"files" => ["src/a.rs", "src/b.rs"], "commit" => "abc1234"})
      d = add.("decision about a", "decision", %{"files" => ["./src/a.rs"]})
      o = add.("outcome at commit", "outcome", %{"commit" => "abc1234def"})

      shown = McpClient.call!(c, "show_node", %{"node_id" => me})

      assert [
               %{"id" => ^o, "score" => 2, "shared_commit" => "abc1234def", "shared_files" => []},
               %{
                 "id" => ^d,
                 "score" => 1,
                 "shared_files" => ["src/a.rs"],
                 "shared_commit" => nil,
                 "node_type" => "decision",
                 "title" => "decision about a",
                 "status" => "pending"
               }
             ] = shown["related"]

      assert shown["edges_from"] == [] and shown["edges_to"] == []
      refute Map.has_key?(shown, "related_commit_ignored")

      head = add.("head", "action", %{"commit" => "HEAD"})
      shown = McpClient.call!(c, "show_node", %{"node_id" => head})
      assert shown["related"] == []
      assert shown["related_commit_ignored"] =~ "not a hash"
    end

    test "query_nodes file filter composes with type and says how each matched", %{
      client: c,
      add: add
    } do
      d = add.("decided about b", "decision", %{"files" => ["src/b.rs"]})
      a = add.("acted on b", "action", %{"files" => ["./src/b.rs", "src/c.rs"]})
      dir = add.("touched src", "decision", %{"files" => ["src/"]})
      _ = add.("elsewhere", "decision", %{"files" => ["lib/x.ex"]})

      %{"count" => 3, "nodes" => nodes} =
        McpClient.call!(c, "query_nodes", %{"workspace" => "rel-ws", "file" => "src/b.rs"})

      assert Enum.map(nodes, &{&1["id"], &1["file_match"]}) |> Enum.take(2) |> Enum.sort() ==
               Enum.sort([{d, "exact"}, {a, "exact"}])

      assert List.last(nodes)["id"] == dir and List.last(nodes)["file_match"] == "dir_contains"

      %{"nodes" => decisions} =
        McpClient.call!(c, "query_nodes", %{
          "workspace" => "rel-ws",
          "file" => "src/b.rs",
          "type" => "decision"
        })

      assert Enum.map(decisions, & &1["id"]) == [d, dir]

      limited =
        McpClient.call!(c, "query_nodes", %{
          "workspace" => "rel-ws",
          "file" => "src/b.rs",
          "limit" => 1
        })

      assert %{"count" => 1, "file_node_count" => 3, "nodes" => [%{"file_match" => "exact"}]} =
               limited

      %{"count" => 0, "decisions_above" => []} =
        McpClient.call!(c, "query_nodes", %{"workspace" => "rel-ws", "file" => "nope.rs"})

      # The decision the action was done under, one real edge up.
      chose =
        add.("chose to rewrite b", "decision", %{})

      McpClient.call!(c, "add_edge", %{
        "from_node_id" => chose,
        "to_node_id" => a,
        "workspace" => "rel-ws"
      })

      %{"decisions_above" => above} =
        McpClient.call!(c, "query_nodes", %{"workspace" => "rel-ws", "file" => "src/b.rs"})

      assert [%{"id" => ^chose, "title" => "chose to rewrite b", "via" => [^a]}] = above

      refute Map.has_key?(
               McpClient.call!(c, "query_nodes", %{"workspace" => "rel-ws"}),
               "decisions_above"
             )

      assert {:error, msg} =
               McpClient.call(c, "query_nodes", %{"workspace" => "*", "file" => "src/b.rs"})

      assert msg =~ "one workspace"

      assert {:error, _} =
               McpClient.call(c, "query_nodes", %{"workspace" => "rel-ws", "file" => ""})
    end
  end
end

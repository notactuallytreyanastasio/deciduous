defmodule DeciduousMcp.MCP.MultiAgentTest do
  @moduledoc """
  The multi-agent surface: branch locks on every write tool, the per-branch
  view in check_activity, and borrows as took_from edges.

  Tool `call/1` is exercised directly with a hand-built `Hermes.Server.Frame`,
  the same shape Hermes hands a tool at runtime: `private.session_id` is what
  the lock is keyed on, and `assigns` is where a header pin would land.
  """
  use DeciduousMcp.DataCase

  alias DeciduousMcp.Graph.{Edges, Nodes}
  alias DeciduousMcp.Locks
  alias DeciduousMcp.MCP.Tools.{CheckActivity, DeleteEdge, DeleteNode, LogObservation, UpdateNode}

  setup do
    workspace = create_test_workspace("multi-agent")

    {:ok, a} =
      Nodes.create_node(workspace.id, %{
        node_type: "decision",
        title: "SRS wall kicks",
        status: "active",
        metadata: %{"branch" => "agent-6"}
      })

    {:ok, b} =
      Nodes.create_node(workspace.id, %{
        node_type: "action",
        title: "Rotation table",
        status: "active",
        metadata: %{"branch" => "agent-3"}
      })

    {:ok, _edge} =
      Edges.create_edge(workspace.id, %{
        from_node_id: a.id,
        to_node_id: b.id,
        edge_type: "leads_to"
      })

    %{workspace: workspace, a: a, b: b}
  end

  defp frame(session_id) do
    %Hermes.Server.Frame{
      private: %{session_id: session_id, client_info: %{"name" => "test-#{session_id}"}},
      assigns: %{}
    }
  end

  defp call(tool, args, session_id), do: tool.call(%{arguments: args, server: frame(session_id)})

  defp pinned_frame(session_id, workspace_id) do
    %{frame(session_id) | assigns: %{pinned_workspace_id: workspace_id}}
  end

  defp call_pinned(tool, args, session_id, workspace_id),
    do: tool.call(%{arguments: args, server: pinned_frame(session_id, workspace_id)})

  describe "the branch lock covers the tools that name a row by id" do
    setup %{workspace: ws} do
      {:ok, _} = Locks.acquire(ws.id, "x", "session_A", "holder", "1")
      :ok
    end

    test "update_node refuses while another session holds the branch", %{b: b} do
      assert {:error, %{message: message}} =
               call(
                 UpdateNode,
                 %{"node_id" => b.id, "title" => "Stolen", "branch" => "x"},
                 "session_B"
               )

      assert message =~ ~s(branch "x") and message =~ "locked by holder"
      assert {:ok, %{title: "Rotation table"}} = Nodes.get_node(b.id)
    end

    test "delete_node refuses while another session holds the branch", %{b: b} do
      assert {:error, %{message: message}} =
               call(DeleteNode, %{"node_id" => b.id, "branch" => "x"}, "session_B")

      assert message =~ "locked by holder"
      assert {:ok, %{deleted_at: nil}} = Nodes.get_node(b.id)
    end

    test "delete_edge refuses while another session holds the branch", %{a: a, b: b} do
      assert {:error, %{message: message}} =
               call(
                 DeleteEdge,
                 %{"from_node_id" => a.id, "to_node_id" => b.id, "branch" => "x"},
                 "session_B"
               )

      assert message =~ "locked by holder"
      assert [_] = Edges.edges_from(a.id)
    end

    test "the three tools go through on a branch nobody holds", %{a: a, b: b} do
      assert {:ok, _} =
               call(
                 UpdateNode,
                 %{"node_id" => b.id, "title" => "Rotation table v2", "branch" => "y"},
                 "session_B"
               )

      assert {:ok, _} =
               call(
                 DeleteEdge,
                 %{"from_node_id" => a.id, "to_node_id" => b.id, "branch" => "y"},
                 "session_B"
               )

      assert {:ok, _} = call(DeleteNode, %{"node_id" => b.id, "branch" => "y"}, "session_B")
      assert {:ok, %{title: "Rotation table v2", deleted_at: %DateTime{}}} = Nodes.get_node(b.id)
      assert [] = Edges.edges_from(a.id)
    end

    test "the holder itself is not blocked by its own lock", %{b: b} do
      assert {:ok, _} =
               call(
                 UpdateNode,
                 %{"node_id" => b.id, "status" => "completed", "branch" => "x"},
                 "session_A"
               )
    end

    test "a node that does not exist is refused before any lock is taken" do
      missing = Ecto.UUID.generate()

      assert {:error, %{message: "Node not found: " <> _}} =
               call(
                 UpdateNode,
                 %{"node_id" => missing, "title" => "x", "branch" => "z"},
                 "session_B"
               )

      assert {:error, %{message: "Node not found: " <> _}} =
               call(DeleteNode, %{"node_id" => "not-a-uuid"}, "session_B")
    end
  end

  describe "check_activity" do
    test "lists the last node on every branch, locked or not", %{workspace: ws, a: a, b: b} do
      {:ok, _} = Locks.acquire(ws.id, "agent-6", "session_A", "claude-code", "2.1")

      {:ok, newer} =
        Nodes.create_node(ws.id, %{
          node_type: "outcome",
          title: "Kicks work",
          status: "completed",
          metadata: %{"branch" => "agent-6"}
        })

      {:ok, loose} = Nodes.create_node(ws.id, %{node_type: "observation", title: "No branch"})

      assert {:ok, json} = call(CheckActivity, %{"workspace" => "multi-agent"}, "session_A")
      %{"branches" => branches, "active_sessions" => 1} = Jason.decode!(json)

      by_branch = Map.new(branches, &{&1["branch"], &1})
      assert Map.keys(by_branch) |> Enum.sort() == [nil, "agent-3", "agent-6"]

      assert by_branch["agent-6"]["last_node"]["id"] == newer.id
      refute by_branch["agent-6"]["last_node"]["id"] == a.id
      assert by_branch["agent-6"]["last_node"]["title"] == "Kicks work"
      assert by_branch["agent-6"]["locked_by"]["client"] == "claude-code"
      assert by_branch["agent-6"]["locked_by"]["is_you"] == true

      assert by_branch["agent-3"]["last_node"]["id"] == b.id
      assert by_branch["agent-3"]["locked_by"] == nil

      assert by_branch[nil]["last_node"]["id"] == loose.id
    end

    test "a soft-deleted node is not anyone's last node", %{b: b} do
      {:ok, _} = Nodes.delete_node(b.id)
      assert {:ok, json} = call(CheckActivity, %{"workspace" => "multi-agent"}, "session_A")
      branches = Jason.decode!(json)["branches"]
      refute Enum.any?(branches, &(&1["branch"] == "agent-3"))
    end

    test "branches is capped at the twenty most recent, and says how many there are", %{
      workspace: ws
    } do
      for i <- 1..25 do
        {:ok, _} =
          Nodes.create_node(ws.id, %{
            node_type: "action",
            title: "Round #{i}",
            metadata: %{"branch" => "run-#{String.pad_leading(to_string(i), 2, "0")}"}
          })
      end

      # 25 here plus the two fixture branches (agent-3, agent-6).
      assert {:ok, json} = call(CheckActivity, %{"workspace" => "multi-agent"}, "session_A")
      %{"branches" => branches, "branches_total" => 27} = Jason.decode!(json)
      assert length(branches) == 20
      # Most recently written first: the last run created is at the top.
      assert hd(branches)["branch"] == "run-25"
      refute Enum.any?(branches, &(&1["branch"] == "agent-3"))

      assert {:ok, json} =
               call(CheckActivity, %{"workspace" => "multi-agent", "branches" => 3}, "session_A")

      %{"branches" => three, "branches_total" => 27} = Jason.decode!(json)
      assert Enum.map(three, & &1["branch"]) == ["run-25", "run-24", "run-23"]

      assert {:ok, json} =
               call(CheckActivity, %{"workspace" => "multi-agent", "branches" => 0}, "session_A")

      assert %{"branches" => [], "branches_total" => 27} = Jason.decode!(json)
    end
  end

  describe "a client pinned to one workspace cannot write another's rows" do
    setup do
      %{other: create_test_workspace("multi-agent-other")}
    end

    test "update_node is refused and the row is unchanged", %{b: b, other: other} do
      assert {:error, %{message: message}} =
               call_pinned(UpdateNode, %{"node_id" => b.id, "title" => "Stolen"}, "s", other.id)

      assert message =~ "another workspace"
      assert {:ok, %{title: "Rotation table"}} = Nodes.get_node(b.id)
    end

    test "delete_node is refused and the row is unchanged", %{b: b, other: other} do
      assert {:error, %{message: message}} =
               call_pinned(DeleteNode, %{"node_id" => b.id}, "s", other.id)

      assert message =~ "another workspace"
      assert {:ok, %{deleted_at: nil}} = Nodes.get_node(b.id)
    end

    test "delete_edge is refused and the edge survives", %{a: a, b: b, other: other} do
      assert {:error, %{message: message}} =
               call_pinned(
                 DeleteEdge,
                 %{"from_node_id" => a.id, "to_node_id" => b.id, "edge_type" => "leads_to"},
                 "s",
                 other.id
               )

      assert message =~ "another workspace"
      assert [_] = Edges.edges_from(a.id)
    end

    test "the pinned workspace itself is still writable", %{workspace: ws, b: b} do
      assert {:ok, _} =
               call_pinned(UpdateNode, %{"node_id" => b.id, "title" => "Renamed"}, "s", ws.id)

      assert {:ok, %{title: "Renamed"}} = Nodes.get_node(b.id)
    end
  end

  describe "check_activity under a workspace-wide lock" do
    test "every branch row shows the holder, not nil", %{workspace: ws} do
      {:ok, ws} =
        DeciduousMcp.Repo.update(
          Ecto.Changeset.change(ws, settings: %{"lock_scope" => "workspace"})
        )

      {:ok, _} = Locks.acquire(ws.id, Locks.lock_key_for(ws, "agent-6"), "session_A", "cc", "1")

      assert {:ok, json} = call(CheckActivity, %{"workspace" => "multi-agent"}, "session_B")

      %{"branches" => branches, "sessions" => [%{"workspace_wide_lock" => true}]} =
        Jason.decode!(json)

      assert branches != []
      assert Enum.all?(branches, &(&1["locked_by"]["client"] == "cc"))
      assert Enum.all?(branches, &(&1["locked_by"]["is_you"] == false))
    end

    test "a negative branches argument is clamped to zero, not defaulted" do
      assert {:ok, json} =
               call(CheckActivity, %{"workspace" => "multi-agent", "branches" => -1}, "session_A")

      assert %{"branches" => [], "branches_total" => 2} = Jason.decode!(json)
    end
  end

  describe "log_observation writes the node and its edges as one transaction" do
    test "a source deleted after it was resolved leaves no observation behind", %{
      workspace: ws,
      b: b
    } do
      {:ok, _} = Nodes.delete_node(b.id)
      before = length(Nodes.list_nodes(ws.id, type: "observation"))

      assert {:error, message} = LogObservation.write(ws.id, %{"title" => "Borrowed"}, nil, b)
      assert message =~ "deleted before the edge could be written"
      assert length(Nodes.list_nodes(ws.id, type: "observation")) == before
    end

    test "the same failure through call/1 is a clean error, not a raise", %{b: b} do
      {:ok, _} = Nodes.delete_node(b.id)

      # Resolution already refuses a deleted source, so the caller sees the
      # resolve error; what matters is that it is an {:error, _} tuple.
      assert {:error, %{message: message}} =
               call(
                 LogObservation,
                 %{"workspace" => "multi-agent", "title" => "x", "took_from" => b.id},
                 "session_A"
               )

      assert message =~ "took_from"
    end
  end

  describe "took_from" do
    test "is an edge type", do: assert("took_from" in DeciduousMcp.Schema.Edge.edge_types())

    test "log_observation draws the borrow edge from the source by UUID", %{workspace: ws, a: a} do
      assert {:ok, json} =
               call(
                 LogObservation,
                 %{
                   "workspace" => "multi-agent",
                   "branch" => "agent-3",
                   "title" => "Took SRS kicks from agent-6",
                   "took_from" => a.id,
                   "why" => "Their table handled the I piece at the wall; mine did not"
                 },
                 "session_B"
               )

      %{"id" => obs_id, "took_from" => %{"from_node_id" => from, "from_change_id" => change_id}} =
        Jason.decode!(json)

      assert from == a.id and change_id == a.change_id

      assert [edge] = Edges.edges_to(obs_id)
      assert edge.edge_type == "took_from"
      assert edge.from_node_id == a.id
      assert edge.rationale == "Their table handled the I piece at the wall; mine did not"
      assert edge.workspace_id == ws.id
    end

    test "log_observation resolves took_from and related_to by change_id", %{a: a, b: b} do
      assert {:ok, json} =
               call(
                 LogObservation,
                 %{
                   "workspace" => "multi-agent",
                   "title" => "Borrowed",
                   "took_from" => a.change_id,
                   "related_to" => b.change_id
                 },
                 "session_B"
               )

      obs_id = Jason.decode!(json)["id"]

      types =
        obs_id |> Edges.edges_to() |> Enum.map(&{&1.from_node_id, &1.edge_type}) |> Enum.sort()

      assert types == Enum.sort([{a.id, "took_from"}, {b.id, "leads_to"}])
    end

    test "an unknown source is refused and nothing is written", %{workspace: ws} do
      before = length(Nodes.list_nodes(ws.id))

      assert {:error, %{message: "took_from: no node nope in this workspace"}} =
               call(
                 LogObservation,
                 %{"workspace" => "multi-agent", "title" => "x", "took_from" => "nope"},
                 "session_B"
               )

      assert length(Nodes.list_nodes(ws.id)) == before
    end

    test "a source from another workspace is refused", %{workspace: _ws} do
      other = create_test_workspace("elsewhere")
      {:ok, foreign} = Nodes.create_node(other.id, %{node_type: "goal", title: "Not yours"})

      assert {:error, %{message: "took_from: no node " <> _}} =
               call(
                 LogObservation,
                 %{"workspace" => "multi-agent", "title" => "x", "took_from" => foreign.id},
                 "session_B"
               )
    end
  end
end

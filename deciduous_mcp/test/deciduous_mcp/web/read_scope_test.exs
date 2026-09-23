defmodule DeciduousMcp.Web.ReadScopeTest do
  @moduledoc """
  A read never creates a workspace. Before, every read tool resolved its
  workspace through find_or_create, so a typo, a probe for "proto-%", or a
  name carrying a right-to-left override each left a permanent empty project
  in list_workspaces. Writes still create on first use; that is how a new
  repo starts logging without anyone provisioning it.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpHttp

  defp exists?(name), do: match?({:ok, _}, Workspaces.get_by_name(name))

  test "read tools on an unknown workspace say so and create nothing" do
    sid = McpHttp.session()

    for {tool, args} <- [
          {"query_nodes", %{}},
          {"get_graph", %{}},
          {"find_orphans", %{}},
          {"check_activity", %{}},
          {"ask_graph", %{"question" => "anything"}}
        ] do
      name = "ghost-#{tool}"
      result = McpHttp.call(sid, tool, Map.put(args, "workspace", name))

      assert {:tool_error, message} = result, "#{tool}: #{inspect(result)}"
      assert message =~ ~s(no workspace named "#{name}"), "#{tool}: #{message}"
      refute exists?(name), "#{tool} created workspace #{name}"
    end

    for name <- ["proto-%", "proto-\u202eevil"] do
      McpHttp.call(sid, "query_nodes", %{"workspace" => name})
      refute exists?(String.downcase(name)), "query_nodes created #{inspect(name)}"
    end
  end

  test "a write creates the workspace, and a read then finds it" do
    sid = McpHttp.session()

    assert {:ok, %{"id" => _}} =
             McpHttp.call(sid, "add_node", %{
               "node_type" => "goal",
               "title" => "first",
               "workspace" => "fresh-repo"
             })

    assert {:ok, %{"count" => 1}} =
             McpHttp.call(sid, "query_nodes", %{"workspace" => "fresh-repo"})
  end

  test "a header pin to an unknown workspace creates it on the first write, not on connect" do
    pin = [{"x-deciduous-workspace", "pinned-fresh"}]
    sid = McpHttp.session(pin)
    refute exists?("pinned-fresh")

    assert {:tool_error, message} = McpHttp.call(sid, "query_nodes", %{}, pin)
    assert message =~ ~s(no workspace named "pinned-fresh")
    refute exists?("pinned-fresh")

    # the pin still beats the argument on reads, even before the workspace exists
    {:ok, _} = Workspaces.find_or_create("someone-else")

    assert {:tool_error, _} =
             McpHttp.call(sid, "query_nodes", %{"workspace" => "someone-else"}, pin)

    assert {:ok, %{"id" => _}} =
             McpHttp.call(sid, "add_node", %{"node_type" => "goal", "title" => "g"}, pin)

    assert exists?("pinned-fresh")
    assert {:ok, %{"count" => 1}} = McpHttp.call(sid, "query_nodes", %{}, pin)
  end

  test "GET /export of an unknown workspace is an empty graph and creates nothing" do
    {status, _, body} = McpHttp.request("GET", "/export?workspace=never-pushed")
    assert status == 200
    assert %{"nodes" => [], "edges" => [], "documents" => []} = Jason.decode!(body)
    refute exists?("never-pushed")
  end

  test "\" *\" is the global token after trimming, so it cannot be written to" do
    sid = McpHttp.session()

    assert {:tool_error, message} =
             McpHttp.call(sid, "add_node", %{
               "node_type" => "goal",
               "title" => "x",
               "workspace" => " *"
             })

    assert message =~ "read-only"
    refute exists?("*")
  end

  test "workspace names with control or formatting characters are refused" do
    sid = McpHttp.session()

    for name <- ["w\u0000x", "proto-\u202eevil", "tab\there"] do
      result =
        McpHttp.call(sid, "add_node", %{
          "node_type" => "goal",
          "title" => "x",
          "workspace" => name
        })

      assert {:tool_error, message} = result, inspect({name, result})
      assert message =~ "control or formatting character", message
      refute message =~ "Postgrex"
    end
  end
end

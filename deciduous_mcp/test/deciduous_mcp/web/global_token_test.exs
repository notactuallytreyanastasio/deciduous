defmodule DeciduousMcp.Web.GlobalTokenTest do
  @moduledoc """
  `*` is the global view, on every way a workspace can be named.

  The first fix compared the `workspace` argument with "*" after
  normalizing it, so `" *"` stopped writing a literal workspace named "*".
  The header and POST /import went through the same normalizer and never
  compared: `X-Deciduous-Workspace: *` then add_node wrote a node into a
  workspace named "*", which an argument read of "*" could not reach
  (that means global), and `/import {"workspace": "*"}` created one too.

  Over real HTTP to the running listener.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.Test.McpHttp

  defp no_star_workspace do
    assert {:error, :not_found} = Workspaces.get_by_name("*")
  end

  test "a header naming * is refused, and nothing is written to a workspace named *" do
    for raw <- ["*", " *", "* "] do
      headers = [{"x-deciduous-workspace", raw}]
      {status, _, body} = McpHttp.post(McpHttp.initialize_body(), headers)

      if status == 200 do
        sid = McpHttp.session(headers)

        McpHttp.call(sid, "add_node", %{"node_type" => "goal", "title" => "hdrstar"}, headers)
      end

      no_star_workspace()
      assert status == 400, "header #{inspect(raw)} answered #{status} #{body}"
      assert body =~ "global"
    end
  end

  test "an import into * is refused and creates nothing" do
    body = Jason.encode!(%{workspace: "*", graph: %{nodes: [], edges: []}})

    {status, _, resp} =
      McpHttp.request("POST", "/import", body, [{"content-type", "application/json"}])

    no_star_workspace()
    assert status == 422, "import answered #{status} #{resp}"
    assert resp =~ "global"
  end
end

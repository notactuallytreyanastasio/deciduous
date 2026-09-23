defmodule DeciduousMcp.MCP.UpdateNodeMetadataTest do
  @moduledoc """
  S4: update_node's metadata is a patch, not a replacement. Sending
  `{"confidence": 50}` used to leave metadata == %{"confidence" => 50}:
  branch, prompt, commit and files gone, and the node dropped out of every
  branch-filtered read.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Graph.{Nodes, Workspaces}
  alias DeciduousMcp.Test.McpClient

  @meta %{
    "branch" => "feat-x",
    "prompt" => "the verbatim prompt",
    "commit" => "abc1234",
    "files" => ["a.rs", "b.rs"],
    "confidence" => 80
  }

  setup do
    {:ok, _ws} = Workspaces.find_or_create("um-ws")
    client = McpClient.connect()

    %{"id" => id} =
      McpClient.call!(client, "add_node", %{
        "node_type" => "action",
        "title" => "um node",
        "workspace" => "um-ws",
        "branch" => "feat-x"
      })

    {:ok, node} = Nodes.get_node(id)
    {:ok, _} = Repo.update(Ecto.Changeset.change(node, metadata: @meta))
    %{client: client, id: id}
  end

  defp metadata(id), do: elem(Nodes.get_node(id), 1).metadata

  test "a metadata key is merged in and every other key survives", %{client: client, id: id} do
    McpClient.call!(client, "update_node", %{
      "node_id" => id,
      "metadata" => %{"confidence" => 50},
      "branch" => "feat-x"
    })

    assert metadata(id) == Map.put(@meta, "confidence", 50)

    %{"nodes" => nodes} =
      McpClient.call!(client, "query_nodes", %{"workspace" => "um-ws", "branch" => "feat-x"})

    assert Enum.any?(nodes, &(&1["id"] == id))
  end

  test "a key sent as null is removed", %{client: client, id: id} do
    McpClient.call!(client, "update_node", %{
      "node_id" => id,
      "metadata" => %{"commit" => nil},
      "branch" => "feat-x"
    })

    assert metadata(id) == Map.delete(@meta, "commit")
  end

  test "a confidence that is not a number from 0 to 100 is refused", %{client: client, id: id} do
    for bad <- ["999", "50", 999, -1, true] do
      assert {:error, message} =
               McpClient.call(client, "update_node", %{
                 "node_id" => id,
                 "metadata" => %{"confidence" => bad},
                 "branch" => "feat-x"
               })

      assert message =~ "confidence", "#{inspect(bad)} was accepted"
    end

    assert metadata(id) == @meta
  end

  test "metadata that is not an object is refused", %{client: client, id: id} do
    assert {:error, message} =
             McpClient.call(client, "update_node", %{
               "node_id" => id,
               "metadata" => "branch=main",
               "branch" => "feat-x"
             })

    assert message =~ "metadata"
    assert metadata(id) == @meta
  end
end

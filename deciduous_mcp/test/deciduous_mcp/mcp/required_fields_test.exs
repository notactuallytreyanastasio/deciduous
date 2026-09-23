defmodule DeciduousMcp.MCP.RequiredFieldsTest do
  @moduledoc """
  T12: a field a tool cannot do without is marked required in the schema
  the client is sent, so an agent learns it from tools/list and not from a
  refusal. capture_conversation_turn's goal, decision, action and outcome
  objects each become a node, and a node needs a title, but none of the
  four said so; an agent that sent `goal: {prompt: "..."}` found out from
  "Turn not captured: title can't be blank".
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.Test.McpHttp

  defp tools do
    sid = McpHttp.session()

    {200, _, body} =
      McpHttp.post(%{jsonrpc: "2.0", id: 9, method: "tools/list"}, [{"mcp-session-id", sid}])

    McpHttp.decode(body)["result"]["tools"]
  end

  # Every object schema, at any depth, that becomes a node: it has a title.
  defp titled_objects(%{"properties" => props} = schema, path) do
    here = if Map.has_key?(props, "title"), do: [{path, schema}], else: []

    here ++
      Enum.flat_map(props, fn {k, v} -> titled_objects(v, path <> "." <> k) end)
  end

  defp titled_objects(%{"items" => items}, path), do: titled_objects(items, path <> "[]")
  defp titled_objects(_, _), do: []

  test "T12: every object that becomes a node marks title required" do
    # update_node's title is the one a caller may leave as it is.
    missing =
      for tool <- tools(),
          tool["name"] != "update_node",
          {path, schema} <- titled_objects(tool["inputSchema"], tool["name"]),
          "title" not in (schema["required"] || []),
          do: path

    assert missing == []
  end

  test "T12: a goal object without a title is refused by the schema, naming the path" do
    sid = McpHttp.session()

    assert {:tool_error, message} =
             McpHttp.call(sid, "capture_conversation_turn", %{
               "workspace" => "required-fields",
               "summary" => "s",
               "goal" => %{"prompt" => "p"}
             })

    assert message =~ "goal.title is required"
  end
end

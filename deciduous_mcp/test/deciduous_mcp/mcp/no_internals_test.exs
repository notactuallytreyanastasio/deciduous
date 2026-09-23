defmodule DeciduousMcp.MCP.NoInternalsTest do
  @moduledoc """
  A refused or failed tool call answers with a sentence. It never carries an
  inspected changeset (which prints the row being written), a Postgrex
  struct, or a stack trace.
  """
  use DeciduousMcp.DataCase, async: false

  alias DeciduousMcp.MCP.Component
  alias DeciduousMcp.Test.McpHttp

  @leak ~r/#Ecto|Changeset|Postgrex|%\{|stacktrace|\.ex:\d+/

  defmodule Raises do
    @moduledoc false
    def definition, do: %{name: "raises", input_schema: %{type: "object", properties: %{}}}
    def call(_), do: raise(ArgumentError, "secret row contents")
  end

  defmodule Exits do
    @moduledoc false
    def definition, do: %{name: "exits", input_schema: %{type: "object", properties: %{}}}
    def call(_), do: exit({:shutdown, %{row: "secret row contents"}})
  end

  test "a changeset error is described field by field, over HTTP" do
    sid = McpHttp.session()

    {:ok, %{"id" => id}} =
      McpHttp.call(sid, "add_node", %{
        "node_type" => "goal",
        "title" => "keep",
        "description" => "private description",
        "workspace" => "no-internals"
      })

    # A self edge is refused by Edge.changeset, not by the argument check
    # (update_node's metadata.confidence, used here before, now is).
    result =
      McpHttp.call(sid, "add_edge", %{
        "from_node_id" => id,
        "to_node_id" => id,
        "workspace" => "no-internals"
      })

    assert {:tool_error, message} = result, inspect(result)
    assert message =~ "to_node_id: cannot create edge from a node to itself"
    refute message =~ @leak
    refute message =~ "private description"
  end

  test "an exception inside a tool is a one-line error naming the tool, not a dump" do
    frame = %Hermes.Server.Frame{private: %{session_id: "t"}, assigns: %{}}

    for module <- [Raises, Exits] do
      assert {:error, %Hermes.MCP.Error{message: message, data: data}, _} =
               Component.dispatch_tool(module, %{}, frame)

      assert message =~ "#{module.definition().name} failed"
      refute message =~ "secret row contents"
      refute inspect(data) =~ "secret row contents"
    end
  end
end

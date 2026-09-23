defmodule DeciduousMcp.Test.Sleep do
  @moduledoc false
  # A tool whose only job is to take as long as it is told to, then report
  # that it got to the end. The report is what shows a killed handler really
  # stopped rather than finishing unobserved.
  use DeciduousMcp.MCP.Component, type: :tool

  def definition do
    %{
      name: "sleep",
      description: "Sleeps for ms milliseconds, then tells the test it finished.",
      input_schema: %{
        type: "object",
        properties: %{ms: %{type: "integer", description: "how long"}},
        required: ["ms"]
      }
    }
  end

  def call(%{arguments: %{"ms" => ms}}) do
    Process.sleep(ms)
    if pid = Process.whereis(:deadline_test), do: send(pid, {:sleep_finished, ms})
    {:ok, %{slept: ms}}
  end
end

defmodule DeciduousMcp.Test.DeadlineServer do
  @moduledoc false
  # A second Hermes server, so the deadline can be tested in milliseconds
  # without a slow tool in the real server or a 60s test.
  use Hermes.Server, name: "deadline-test", version: "0", capabilities: [:tools]

  component(DeciduousMcp.Test.Sleep)

  @impl true
  def init(_client_info, frame), do: {:ok, frame}
end

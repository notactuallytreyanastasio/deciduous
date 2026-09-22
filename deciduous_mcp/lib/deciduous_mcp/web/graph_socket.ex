defmodule DeciduousMcp.Web.GraphSocket do
  @moduledoc """
  WebSocket half of the event stream: subscribes to the workspace's PubSub
  topic on connect, pushes each `{:graph_event, event}` as a JSON text frame,
  and does nothing else — this holds no state a client can mutate, so there is
  nothing here for `handle_in/2` to act on.
  """
  @behaviour WebSock

  @impl true
  def init(%{topic: topic}) do
    Phoenix.PubSub.subscribe(DeciduousMcp.PubSub, topic)
    {:ok, %{}}
  end

  @impl true
  def handle_in(_data, state), do: {:ok, state}

  @impl true
  def handle_info({:graph_event, event}, state) do
    {:push, {:text, Jason.encode!(event)}, state}
  end

  def handle_info(_other, state), do: {:ok, state}

  @impl true
  def terminate(_reason, state), do: {:ok, state}
end

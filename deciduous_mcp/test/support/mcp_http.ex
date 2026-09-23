defmodule DeciduousMcp.Test.McpHttp do
  @moduledoc """
  A real MCP client for tests: TCP to the Bandit listener the application
  starts on `PORT`, through auth, the workspace plug, the session guard and
  Hermes, exactly as Claude Code reaches it.

  `Plug.Test` calls the router in the test process, so it cannot show what a
  client sees when Bandit answers a crashed plug (an empty 500), and it
  cannot run requests in parallel from separate connections. This can.

  Database: requests are served by Bandit's processes, not the test's. Use it
  from a test that either shares the sandbox (`DeciduousMcp.DataCase`, which
  is shared when not async) or runs outside it (`DeciduousMcp.RealDbCase`).
  """

  @doc """
  The port the application's listener is bound to. `PORT=0` (what CI's
  verify-release.sh passes, so parallel runs cannot collide) asks the OS for
  a free port, so the number has to come from the listener, not from `PORT`.
  """
  def port do
    case System.get_env("PORT") do
      p when p in [nil, ""] -> 4000
      "0" -> bound_port()
      p -> String.to_integer(p)
    end
  end

  defp bound_port do
    DeciduousMcp.Supervisor
    |> Supervisor.which_children()
    |> Enum.find_value(fn {_id, pid, _type, _mods} ->
      with true <- is_pid(pid),
           {:ok, {_ip, port}} <- safe_listener_info(pid) do
        port
      else
        _ -> nil
      end
    end) || raise "PORT=0 but no Bandit listener is running under DeciduousMcp.Supervisor"
  end

  defp safe_listener_info(pid) do
    ThousandIsland.listener_info(pid)
  catch
    _, _ -> :error
  end

  def token, do: Application.fetch_env!(:deciduous_mcp, :api_token)

  @doc "POST a raw body to /mcp. Returns `{status, headers, body}`."
  def post(body, headers \\ []) do
    body = if is_binary(body), do: body, else: Jason.encode!(body)
    request("POST", "/mcp", body, [{"content-type", "application/json"} | headers])
  end

  @doc "Any request to the listener, authenticated. Returns `{status, headers, body}`."
  def request(method, path, body \\ nil, headers \\ []) do
    headers =
      [
        {"authorization", "Bearer " <> token()},
        {"accept", "application/json, text/event-stream"}
      ] ++ headers

    {:ok, conn} = Mint.HTTP.connect(:http, "127.0.0.1", port(), mode: :passive)
    {:ok, conn, ref} = Mint.HTTP.request(conn, method, path, headers, body)
    {status, resp_headers, resp_body} = receive_all(conn, ref, {nil, [], []})
    Mint.HTTP.close(conn)
    {status, Map.new(resp_headers), IO.iodata_to_binary(resp_body)}
  end

  defp receive_all(conn, ref, acc) do
    {:ok, conn, responses} = Mint.HTTP.recv(conn, 0, 60_000)

    Enum.reduce_while(responses, {:cont, acc}, fn
      {:status, ^ref, s}, {:cont, {_, h, b}} -> {:cont, {:cont, {s, h, b}}}
      {:headers, ^ref, hs}, {:cont, {s, h, b}} -> {:cont, {:cont, {s, h ++ hs, b}}}
      {:data, ^ref, d}, {:cont, {s, h, b}} -> {:cont, {:cont, {s, h, [b, d]}}}
      {:done, ^ref}, {:cont, acc} -> {:halt, {:done, acc}}
      _, a -> {:cont, a}
    end)
    |> case do
      {:done, acc} -> acc
      {:cont, acc} -> receive_all(conn, ref, acc)
    end
  end

  @doc "Initializes a session and returns its id. `headers` ride on every request."
  def session(headers \\ []) do
    {200, h, _} = post(initialize_body(), headers)
    sid = Map.fetch!(h, "mcp-session-id")

    post(%{jsonrpc: "2.0", method: "notifications/initialized"}, [
      {"mcp-session-id", sid} | headers
    ])

    # a request queues behind the initialized cast
    {200, _, _} =
      post(%{jsonrpc: "2.0", id: 0, method: "tools/list"}, [{"mcp-session-id", sid} | headers])

    sid
  end

  def initialize_body(id \\ 1) do
    %{
      jsonrpc: "2.0",
      id: id,
      method: "initialize",
      params: %{
        protocolVersion: "2025-06-18",
        capabilities: %{},
        clientInfo: %{name: "mcp-http-test", version: "0"}
      }
    }
  end

  @doc """
  Calls a tool. Returns `{:ok, decoded_result}`; `{:tool_error, message}`
  when the tool refused (this server answers a tool's own error as JSON-RPC
  -32000 "execution error", and an isError result is treated the same); or
  `{:rpc_error, error_map}` for any other JSON-RPC error.
  """
  def call(sid, tool, args, headers \\ [], id \\ 1) do
    {_status, _h, body} =
      post(
        %{jsonrpc: "2.0", id: id, method: "tools/call", params: %{name: tool, arguments: args}},
        [{"mcp-session-id", sid} | headers]
      )

    case decode(body) do
      %{"error" => %{"code" => -32000, "message" => message} = error}
      when not is_map_key(error, "data") ->
        {:tool_error, message}

      %{"error" => error} ->
        {:rpc_error, error}

      %{"result" => %{"isError" => true, "content" => content}} ->
        {:tool_error, Enum.map_join(content, "", & &1["text"])}

      %{"result" => %{"content" => content}} ->
        text = Enum.map_join(content, "", & &1["text"])

        case Jason.decode(text) do
          {:ok, v} -> {:ok, v}
          _ -> {:ok, text}
        end
    end
  end

  @doc "Decodes a JSON or single-event SSE body."
  def decode(body) do
    if String.starts_with?(body, "event:") or String.starts_with?(body, "data:") do
      body
      |> String.split("\n")
      |> Enum.find_value(fn
        "data:" <> json -> Jason.decode!(String.trim(json))
        _ -> nil
      end)
    else
      Jason.decode!(body)
    end
  end
end

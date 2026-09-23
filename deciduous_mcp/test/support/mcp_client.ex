defmodule DeciduousMcp.Test.McpClient do
  @moduledoc """
  An MCP client that talks to the whole router, the way a real client does:
  bearer token, `initialize`, the `initialized` notification, then
  `tools/call` with the session id and, optionally, the
  `X-Deciduous-Workspace` pin header on every request.

  The findings these tests reproduce were found over HTTP, and the pin is
  resolved by `WorkspacePlug` from the header, so calling a tool module
  directly with a hand-built frame would skip the part under test.
  """
  import Plug.Test
  import Plug.Conn

  alias DeciduousMcp.Web.Router

  defstruct [:token, :sid, headers: []]

  @opts Router.init([])

  def connect(opts \\ []) do
    token = Application.fetch_env!(:deciduous_mcp, :api_token)

    headers =
      case opts[:pin] do
        nil -> []
        ws -> [{"x-deciduous-workspace", ws}]
      end

    client = %__MODULE__{token: token, headers: headers}

    conn =
      post(client, %{
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: %{
          protocolVersion: "2025-03-26",
          capabilities: %{},
          clientInfo: %{name: opts[:name] || "guard-test", version: "0"}
        }
      })

    [sid] = get_resp_header(conn, "mcp-session-id")
    client = %{client | sid: sid}
    post(client, %{jsonrpc: "2.0", method: "notifications/initialized"})
    # Queued behind the cast above, so the session is initialized after this.
    post(client, %{jsonrpc: "2.0", id: 0, method: "tools/list"})
    client
  end

  def post(%__MODULE__{} = client, body) do
    headers = if client.sid, do: [{"mcp-session-id", client.sid} | client.headers], else: client.headers

    conn =
      conn(:post, "/mcp", Jason.encode!(body))
      |> put_req_header("authorization", "Bearer " <> client.token)
      |> put_req_header("content-type", "application/json")
      |> put_req_header("accept", "application/json, text/event-stream")

    headers
    |> Enum.reduce(conn, fn {k, v}, c -> put_req_header(c, k, v) end)
    |> Router.call(@opts)
  end

  @doc """
  Calls a tool. Returns `{:ok, decoded}` for a result, `{:error, message}`
  for a tool error, `{:crash, error}` when the handler raised, and
  `{:rpc_error, error}` for any other JSON-RPC error.
  """
  def call(%__MODULE__{} = client, tool, args) do
    id = System.unique_integer([:positive])

    conn =
      post(client, %{
        jsonrpc: "2.0",
        id: id,
        method: "tools/call",
        params: %{name: tool, arguments: args}
      })

    case Jason.decode!(conn.resp_body) do
      # What a handler task that raised comes back as (vendored Hermes
      # patch): kept apart from a tool's own error, because a crash is the
      # thing most of these tests are guarding against.
      %{"error" => %{"message" => "request handler crashed"} = error} ->
        {:crash, error}

      # A tool's `{:error, %{message: _}}` is sent as a JSON-RPC execution
      # error, not as an isError result.
      %{"error" => %{"code" => -32000, "message" => message}} ->
        {:error, message}

      %{"error" => error} ->
        {:rpc_error, error}

      %{"result" => %{"isError" => true, "content" => content}} ->
        {:error, text(content)}

      %{"result" => %{"content" => content}} ->
        raw = text(content)

        case Jason.decode(raw) do
          {:ok, decoded} -> {:ok, decoded}
          {:error, _} -> {:ok, raw}
        end
    end
  end

  @doc "Like `call/3`, but a non-`{:ok, _}` answer fails the test with what came back."
  def call!(client, tool, args) do
    case call(client, tool, args) do
      {:ok, decoded} -> decoded
      other -> raise "#{tool} #{inspect(args)} failed: #{inspect(other)}"
    end
  end

  @doc "GET a non-MCP route (e.g. /export) with the client's token and pin."
  def get(%__MODULE__{} = client, path) do
    conn =
      conn(:get, path)
      |> put_req_header("authorization", "Bearer " <> client.token)

    client.headers
    |> Enum.reduce(conn, fn {k, v}, c -> put_req_header(c, k, v) end)
    |> Router.call(@opts)
  end

  defp text(content), do: Enum.map_join(content, "", &(&1["text"] || ""))
end

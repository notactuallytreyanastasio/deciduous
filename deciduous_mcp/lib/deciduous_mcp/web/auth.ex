defmodule DeciduousMcp.Web.Auth do
  @moduledoc """
  Bearer token authentication for the public MCP endpoint.

  The server is reachable from the open internet, so every request that is not
  `/health` must carry `Authorization: Bearer <token>` matching
  `DECIDUOUS_MCP_TOKEN`.

  The comparison is constant time. A plain `==` on a binary leaks the length of
  the shared prefix through timing, which is enough to recover a token one byte
  at a time given enough requests.

  There is no token fallback and no default. If `DECIDUOUS_MCP_TOKEN` is unset
  the application refuses to boot (see `DeciduousMcp.Application`) rather than
  quietly serving the whole decision graph to anyone who finds the subdomain.
  """
  @behaviour Plug

  import Plug.Conn

  @impl Plug
  def init(opts), do: opts

  @impl Plug
  def call(conn, _opts) do
    with [header] <- get_req_header(conn, "authorization"),
         "Bearer " <> presented <- header,
         true <- valid?(presented) do
      conn
    else
      _ -> deny(conn)
    end
  end

  defp valid?(presented) do
    expected = Application.get_env(:deciduous_mcp, :api_token)
    is_binary(expected) and Plug.Crypto.secure_compare(presented, expected)
  end

  defp deny(conn) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(401, Jason.encode!(%{error: "unauthorized"}))
    |> halt()
  end
end

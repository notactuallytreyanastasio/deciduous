defmodule DeciduousMcp.Web.Auth do
  @moduledoc """
  Bearer token authentication for the public MCP endpoint.

  The server is reachable from the open internet, so every request that is not
  `/health` must carry `Authorization: Bearer <token>` matching
  `DECIDUOUS_MCP_TOKEN`.

  The comparison is constant time. A plain `==` on a binary leaks the length of
  the shared prefix through timing, which is enough to recover a token one byte
  at a time given enough requests.

  Two kinds of token are accepted. The one in `DECIDUOUS_MCP_TOKEN` is the
  server's own: it is the only one that can issue or revoke others
  (`require_master/1`). A named token (`DeciduousMcp.Tokens`) does
  everything else, and its name is assigned as `:actor`, which is what
  `DeciduousMcp.Activity` records beside a write.

  `:actor` is assigned on every request, as `nil` for the server's own
  token. Hermes keeps one frame per server and merges `conn.assigns` into
  it on every request, so an assign that is only sometimes set is inherited
  by the next session that does not set it (see
  `DeciduousMcp.Web.WorkspacePlug`, where this was first seen).

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
         {:ok, actor} <- identify(presented) do
      conn
      |> assign(:actor, actor)
      |> assign(:master_token, is_nil(actor))
    else
      _ -> deny(conn)
    end
  end

  @doc """
  Halts with 403 unless the request carried the server's own token. For the
  routes that issue and revoke named tokens: a named token that could mint
  another would make revoking it pointless.
  """
  def require_master(conn) do
    if conn.assigns[:master_token] == true do
      conn
    else
      conn
      |> put_resp_content_type("application/json")
      |> send_resp(
        403,
        Jason.encode!(%{
          error: "only the server's own token (DECIDUOUS_MCP_TOKEN) manages tokens"
        })
      )
      |> halt()
    end
  end

  defp identify(presented) do
    if master?(presented) do
      {:ok, nil}
    else
      DeciduousMcp.Tokens.verify(presented)
    end
  end

  defp master?(presented) do
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

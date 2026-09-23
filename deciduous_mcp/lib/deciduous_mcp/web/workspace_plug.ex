defmodule DeciduousMcp.Web.WorkspacePlug do
  @moduledoc """
  Resolves the *pinned* workspace for a request, if the client pinned one.

  One server process serves every project on the machine, so the workspace
  cannot be decided once at boot the way a per-project stdio server could. It
  is decided per request, and there are two ways in:

    * `X-Deciduous-Workspace: <name>` — a hard pin. A repo that drops a
      `.mcp.json` with this header gets that workspace for every call, and no
      tool argument can move a node out of it.

    * nothing — the workspace comes from each tool's `workspace` argument
      instead, resolved in `DeciduousMcp.MCP.Scope`.

  This plug only handles the first case. It assigns `:pinned_workspace_id`,
  which `Hermes.Server.Frame` inherits from `Plug.Conn.assigns` on HTTP
  transports, so every tool sees it without the tools knowing about HTTP.

  An unknown workspace name is created rather than rejected: a new repo should
  start logging on its first call, not fail until someone provisions it.
  """
  @behaviour Plug

  import Plug.Conn

  alias DeciduousMcp.Graph.Workspaces

  @header "x-deciduous-workspace"

  @impl Plug
  def init(opts), do: opts

  @impl Plug
  def call(conn, _opts) do
    case get_req_header(conn, @header) do
      [raw | _] ->
        case Workspaces.normalize_name(raw) do
          {:ok, name} ->
            case Workspaces.find_or_create(name) do
              {:ok, workspace} -> assign(conn, :pinned_workspace_id, workspace.id)
              {:error, _} -> unavailable(conn, name)
            end

          {:error, reason} ->
            reject(conn, raw, reason)
        end

      # Assigned explicitly, as nil, rather than left absent. Hermes 0.14.1
      # keeps one Frame per server and `populate_frame/4` does
      # `Map.merge(frame.assigns, conn.assigns)` on every request, so an
      # assign is never removed once any session has set it. Seen on
      # production 2026-09-22: session P pinned `epstein` by header, then
      # session Q with no header asked `query_nodes` for `tetris-arena` and
      # got epstein's nodes. Writing nil here overwrites the leaked pin on
      # every request that does not carry the header.
      [] ->
        assign(conn, :pinned_workspace_id, nil)
    end
  end

  # A match here used to be `{:ok, workspace} = ...`, and any failure became
  # Bandit's empty 500. Say what failed instead.
  defp unavailable(conn, name) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(503, Jason.encode!(%{error: "could not resolve workspace", value: name}))
    |> halt()
  end

  defp reject(conn, raw, reason) do
    body =
      Jason.encode!(%{
        error: "invalid #{@header} header",
        value: raw,
        reason: to_string(reason)
      })

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(400, body)
    |> halt()
  end
end

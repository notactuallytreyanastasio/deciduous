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

  This plug only handles the first case. It assigns `:pinned_workspace_name`
  and, when that workspace already exists, `:pinned_workspace_id`, which
  `Hermes.Server.Frame` inherits from `Plug.Conn.assigns` on HTTP transports,
  so every tool sees them without the tools knowing about HTTP.

  An unknown workspace name is not rejected, and not created here either: a
  new repo should start logging on its first call, not fail until someone
  provisions it, so `DeciduousMcp.MCP.Scope` creates it on the first write.
  Creating it here made every connect a write, and 30 connects at once to a
  new name raced on the unique index (23 of them got an empty HTTP 500).
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
          # "*" is the global view. A pin is where writes land, and the
          # normalized header was pinned as it came: `*` became a literal
          # workspace named "*" on the first add_node, reachable only through
          # the pin, while the same token as an argument read everything.
          {:ok, "*"} ->
            reject(conn, raw, :global)

          {:ok, name} ->
            id =
              case Workspaces.get_by_name(name) do
                {:ok, workspace} -> workspace.id
                {:error, :not_found} -> nil
              end

            conn
            |> assign(:pinned_workspace_id, id)
            |> assign(:pinned_workspace_name, name)

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
        conn
        |> assign(:pinned_workspace_id, nil)
        |> assign(:pinned_workspace_name, nil)
    end
  end

  defp reject(conn, raw, reason) do
    body =
      Jason.encode!(%{
        error: "invalid #{@header} header",
        value: raw,
        reason: Workspaces.describe_name_error(raw, reason)
      })

    conn
    |> put_resp_content_type("application/json")
    |> send_resp(400, body)
    |> halt()
  end
end

defmodule DeciduousMcp.MCP.ReadDefaults do
  @moduledoc """
  What `ask_graph` and `query_nodes` do when a caller says nothing about
  detail or retired notes (observations whose status is superseded or
  abandoned; decisions and every other type are never hidden): the deployment's choice, from the
  `DECIDUOUS_ASK_DETAIL_DEFAULT` and `DECIDUOUS_HIDE_RETIRED_DEFAULT`
  settings (see `config/runtime.exs`, which validates them at boot).

  With neither set, the answers are `"full"` and `false`, which is what
  both tools returned before the settings existed. An argument a caller
  passes always wins over these.
  """

  @doc ~S|The detail `ask_graph` renders without a `detail` argument: "full" or "lean".|
  def detail do
    case config()[:detail] do
      "lean" -> "lean"
      _ -> "full"
    end
  end

  @doc "True when retired notes (superseded or abandoned observations) are left out unless a caller asks for them."
  def hide_retired?, do: config()[:hide_retired] == true

  defp config, do: Application.get_env(:deciduous_mcp, :read_defaults, [])
end

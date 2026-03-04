defmodule Deciduex.Application do
  @moduledoc false
  use Application

  alias Burrito.Util.Args
  alias Deciduex.CLI

  @impl true
  def start(_type, _args) do
    children = []
    opts = [strategy: :one_for_one, name: Deciduex.Supervisor]

    # Only invoke CLI when running as a Burrito-wrapped binary.
    # Burrito sets the BURRITO environment variable at runtime.
    if System.get_env("BURRITO") do
      args = burrito_args()
      CLI.main(args)
      System.halt(0)
    end

    Supervisor.start_link(children, opts)
  end

  defp burrito_args do
    if Code.ensure_loaded?(Args) do
      Args.get_arguments()
    else
      System.argv()
    end
  end
end

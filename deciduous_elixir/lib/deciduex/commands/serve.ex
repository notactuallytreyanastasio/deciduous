defmodule Deciduex.Commands.Serve do
  @moduledoc """
  Implements the `serve` command to start the decision graph viewer.

  Usage: deciduex serve [--port PORT]

  Default port: 3000

  This command explicitly starts required OTP applications (Ranch, Cowboy)
  to work properly in the `eval` context used by the CLI wrapper.
  """

  @required_apps [:crypto, :ranch, :cowlib, :cowboy, :plug, :plug_cowboy]

  def run(args) do
    port = parse_port(args)

    # Start required applications for Plug.Cowboy
    ensure_applications_started()

    IO.puts("")
    IO.puts("\e[1;32mDeciduous\e[0m")
    IO.puts("   Graph viewer: http://localhost:#{port}")
    IO.puts("   Press Ctrl+C to stop")
    IO.puts("")

    start_server(port)
  end

  defp parse_port(args) do
    case args do
      ["--port", port_str | _] -> parse_port_value(port_str)
      ["-p", port_str | _] -> parse_port_value(port_str)
      _ -> 3000
    end
  end

  defp parse_port_value(str) do
    case Integer.parse(str) do
      {port, ""} when port > 0 and port < 65_536 -> port
      _ -> 3000
    end
  end

  defp ensure_applications_started do
    Enum.each(@required_apps, fn app ->
      case Application.ensure_all_started(app) do
        {:ok, _} -> :ok
        {:error, {app, reason}} ->
          IO.puts(:stderr, "Warning: Failed to start #{app}: #{inspect(reason)}")
      end
    end)
  end

  defp start_server(port) do
    children = [
      {Plug.Cowboy, scheme: :http, plug: Deciduex.Serve.Router, options: [port: port]}
    ]

    opts = [strategy: :one_for_one, name: Deciduex.ServeSupervisor]

    case Supervisor.start_link(children, opts) do
      {:ok, _pid} ->
        # Keep running until interrupted
        Process.sleep(:infinity)

      {:error, {:already_started, _}} ->
        IO.puts(:stderr, "Server already running on port #{port}")
        System.halt(1)

      {:error, reason} ->
        IO.puts(:stderr, "Failed to start server: #{inspect(reason)}")
        System.halt(1)
    end
  end
end

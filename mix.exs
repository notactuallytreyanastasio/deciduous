defmodule Deciduex.MixProject do
  use Mix.Project

  @version "0.14.0-beta.3"

  def project do
    [
      app: :deciduex,
      version: @version,
      elixir: "~> 1.17",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      releases: releases(),
      aliases: aliases(),
      package: package(),
      description: "Decision graph tooling for AI-assisted development",
      source_url: "https://github.com/notactuallytreyanastasio/deciduous",
      homepage_url: "https://notactuallytreyanastasio.github.io/deciduous/",
      docs: docs(),
      dialyzer: [
        plt_local_path: "_build/plts",
        plt_core_path: "_build/plts",
        plt_add_apps: [:credo]
      ]
    ]
  end

  def application do
    [
      extra_applications: [:logger],
      mod: {Deciduex.Application, []}
    ]
  end

  # Credo checks in dev/ only compiled in dev/test (Credo not available in prod)
  defp elixirc_paths(:prod), do: ["lib"]
  defp elixirc_paths(:test), do: ["lib", "dev", "test/support"]
  defp elixirc_paths(:dev), do: ["lib", "dev"]

  defp deps do
    [
      {:ecto_sqlite3, "~> 0.17"},
      {:jason, "~> 1.4"},
      {:elixir_uuid, "~> 1.2"},
      {:burrito, "~> 1.0"},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false},
      {:ex_doc, "~> 0.31", only: :dev, runtime: false}
    ]
  end

  def cli do
    [
      preferred_envs: [
        precommit: :test
      ]
    ]
  end

  # Note: escript doesn't work because exqlite uses NIFs.
  # Use Mix tasks instead: mix deciduous init, mix deciduous add, etc.
  # Or install standalone via Homebrew/Cargo for PATH binary.

  defp package do
    [
      name: "deciduex",
      licenses: ["Apache-2.0"],
      links: %{
        "GitHub" => "https://github.com/notactuallytreyanastasio/deciduous",
        "Docs" => "https://notactuallytreyanastasio.github.io/deciduous/"
      },
      files: ~w(lib priv .formatter.exs mix.exs README.md LICENSE CHANGELOG.md)
    ]
  end

  defp docs do
    [
      main: "readme",
      extras: ["README.md", "CHANGELOG.md"]
    ]
  end

  defp aliases do
    [
      precommit: [
        "format --check-formatted",
        "credo --strict",
        "dialyzer",
        "test --warnings-as-errors"
      ]
    ]
  end

  defp releases do
    base = [
      deciduex: [
        steps: release_steps(),
        burrito: [
          targets: [
            darwin_arm64: [os: :darwin, cpu: :aarch64],
            darwin_amd64: [os: :darwin, cpu: :x86_64],
            linux_amd64: [os: :linux, cpu: :x86_64]
          ]
        ]
      ]
    ]

    base
  end

  # Only wrap with Burrito when BURRITO_TARGET is set (requires Zig).
  # Otherwise build a plain OTP release.
  defp release_steps do
    if System.get_env("BURRITO_TARGET") do
      [:assemble, &Burrito.wrap/1]
    else
      [:assemble, &create_cli_wrapper/1]
    end
  end

  # Create the cli wrapper script that handles symlinks properly
  defp create_cli_wrapper(release) do
    cli_path = Path.join([release.path, "bin", "cli"])

    cli_content = ~S"""
    #!/bin/sh
    # Deciduous CLI entry point.

    # Resolve symlinks to get the real script location
    SCRIPT="$0"
    while [ -L "$SCRIPT" ]; do
      SCRIPT_DIR="$(cd "$(dirname "$SCRIPT")" && pwd)"
      SCRIPT="$(readlink "$SCRIPT")"
      [ "${SCRIPT#/}" = "$SCRIPT" ] && SCRIPT="$SCRIPT_DIR/$SCRIPT"
    done
    SCRIPT_DIR="$(cd "$(dirname "$SCRIPT")" && pwd)"
    RELEASE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
    RELEASE_BIN="$RELEASE_ROOT/bin/deciduex"

    if [ ! -f "$RELEASE_BIN" ]; then
      echo "Error: deciduex release not found at $RELEASE_BIN" >&2
      exit 1
    fi

    # Build Elixir list string from shell arguments
    args=""
    for arg in "$@"; do
      if [ -n "$args" ]; then
        args="$args, "
      fi
      args="$args\"$arg\""
    done

    exec "$RELEASE_BIN" eval "Deciduex.CLI.main([$args])"
    """

    File.write!(cli_path, String.trim_leading(cli_content))
    File.chmod!(cli_path, 0o755)
    release
  end
end

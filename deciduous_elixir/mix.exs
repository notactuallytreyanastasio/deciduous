defmodule Deciduex.MixProject do
  use Mix.Project

  def project do
    [
      app: :deciduex,
      version: "0.1.0",
      elixir: "~> 1.19",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      releases: releases(),
      aliases: aliases(),
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
      {:plug_cowboy, "~> 2.7"},
      {:burrito, "~> 1.0"},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false}
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
      [:assemble]
    end
  end
end

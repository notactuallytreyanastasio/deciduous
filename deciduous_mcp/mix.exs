defmodule DeciduousMcp.MixProject do
  use Mix.Project

  def project do
    [
      app: :deciduous_mcp,
      version: "1.0.1",
      elixir: "~> 1.16",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      aliases: aliases(),
      deps: deps(),
      releases: [
        deciduous_mcp: [
          include_executables_for: [:unix],
          # The container runs migrations before boot via eval, so the release
          # has to carry priv/repo/migrations.
          applications: [deciduous_mcp: :permanent]
        ]
      ],
      description: "MCP server for Deciduous decision graphs with shared PostgreSQL backend",
      package: package()
    ]
  end

  def application do
    [
      extra_applications: [:logger],
      mod: {DeciduousMcp.Application, []}
    ]
  end

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_), do: ["lib"]

  defp deps do
    [
      # Database
      {:ecto_sql, "~> 3.11"},
      {:postgrex, "~> 0.19"},

      # JSON
      {:jason, "~> 1.4"},

      # HTTP surface: Hermes ships its Streamable HTTP plug behind
      # `Code.ensure_loaded?(Plug)`, so plug is a hard dependency here even
      # though it is optional upstream.
      {:plug, "~> 1.18"},
      {:bandit, "~> 1.6"},

      # PubSub for real-time collaboration
      {:phoenix_pubsub, "~> 2.1"},

      # MCP Protocol - Hermes MCP (Elixir MCP SDK by Cloudwalk)
      {:hermes_mcp, path: "vendor/hermes_mcp"},

      # UUID generation
      {:elixir_uuid, "~> 1.2"},

      # Testing
      {:ex_machina, "~> 2.8", only: :test},
      {:mox, "~> 1.1", only: :test}
    ]
  end

  defp aliases do
    [
      setup: ["deps.get", "ecto.setup"],
      "ecto.setup": ["ecto.create", "ecto.migrate", "run priv/repo/seeds.exs"],
      "ecto.reset": ["ecto.drop", "ecto.setup"],
      test: ["ecto.create --quiet", "ecto.migrate --quiet", "test"]
    ]
  end

  defp package do
    [
      licenses: ["MIT"],
      links: %{"GitHub" => "https://github.com/notactuallytreyanastasio/deciduous"}
    ]
  end
end

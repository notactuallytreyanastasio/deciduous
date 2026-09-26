defmodule Mix.Tasks.Deciduous.Eval do
  @shortdoc "Runs the ask_graph retrieval eval against a rolled-back fixture graph"
  @moduledoc """
  Runs `DeciduousMcp.Eval.AskGraph`: loads the fixture decision graph
  through the MCP tools, asks every question through `ask_graph`, prints
  recall@5, recall@10, MRR, context recall and the adversarial pass rate,
  then rolls the database back.

      PGDATABASE=deciduous_mcp_test_x mix deciduous.eval
      mix deciduous.eval --only multi_hop,temporal
      mix deciduous.eval --json out.json

  Runs in the test environment (set in mix.exs `cli/0`), because it needs
  the SQL sandbox to leave nothing behind and the test MCP client. The
  listener binds PORT (default here: 0, any free port) and the bearer token
  is DECIDUOUS_MCP_TOKEN, or a random one when unset.
  """
  use Mix.Task

  @categories ~w(single_hop multi_hop temporal file adversarial)

  @impl true
  def run(argv) do
    {opts, _, invalid} = OptionParser.parse(argv, strict: [only: :string, json: :string])
    if invalid != [], do: Mix.raise("unknown options: #{inspect(invalid)}")

    if Mix.env() != :test do
      Mix.raise("deciduous.eval needs MIX_ENV=test (it runs inside the SQL sandbox)")
    end

    only =
      case opts[:only] do
        nil ->
          nil

        s ->
          for c <- String.split(s, ","), c = String.trim(c) do
            if c in @categories,
              do: String.to_atom(c),
              else:
                Mix.raise(
                  "unknown category #{inspect(c)}; one of #{Enum.join(@categories, ", ")}"
                )
          end
      end

    if System.get_env("PORT") in [nil, ""], do: System.put_env("PORT", "0")

    # Read by config/runtime.exs, which app.start loads; the application
    # refuses to boot without one.
    if System.get_env("DECIDUOUS_MCP_TOKEN") in [nil, ""] do
      System.put_env(
        "DECIDUOUS_MCP_TOKEN",
        Base.encode16(:crypto.strong_rand_bytes(32), case: :lower)
      )
    end

    Mix.Task.run("ecto.create", ["--quiet"])
    Mix.Task.run("ecto.migrate", ["--quiet"])
    Mix.Task.run("app.start")

    repo = DeciduousMcp.Repo
    Ecto.Adapters.SQL.Sandbox.mode(repo, :manual)
    :ok = Ecto.Adapters.SQL.Sandbox.checkout(repo)
    Ecto.Adapters.SQL.Sandbox.mode(repo, {:shared, self()})

    try do
      result = DeciduousMcp.Eval.AskGraph.run(only: only)
      Mix.shell().info(DeciduousMcp.Eval.AskGraph.format(result))

      if path = opts[:json] do
        File.write!(path, Jason.encode_to_iodata!(result, pretty: true))
        Mix.shell().info("wrote #{path}")
      end
    after
      Ecto.Adapters.SQL.Sandbox.checkin(repo)
    end
  end
end

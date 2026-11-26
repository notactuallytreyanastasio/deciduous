defmodule Mix.Tasks.Deciduous.Eval do
  @shortdoc "Runs the ask_graph retrieval eval against a rolled-back fixture graph"
  @moduledoc """
  Runs `DeciduousMcp.Eval.AskGraph`: loads the fixture decision graph
  through the MCP tools, asks every question through `ask_graph`, prints
  recall@5, recall@10, MRR, context recall and the adversarial pass rate,
  then rolls the database back. It runs twice and prints both scores: once
  as a default call, and once as the lean read (`detail: "lean",
  include_retired: false`). The task fails when the lean run scores below
  the default run on recall@5, recall@10, MRR or context recall by more
  than one question's worth (equal-score nodes change order between runs),
  or on the adversarial pass count at all.

      PGDATABASE=deciduous_mcp_test_x mix deciduous.eval
      mix deciduous.eval --only multi_hop,temporal
      mix deciduous.eval --mode default
      mix deciduous.eval --json out.json

  `--mode` is `default`, `lean` or `both` (the default). `--json` writes the
  first run printed.

  Runs in the test environment (set in mix.exs `cli/0`), because it needs
  the SQL sandbox to leave nothing behind and the test MCP client. The
  listener binds PORT (default here: 0, any free port) and the bearer token
  is DECIDUOUS_MCP_TOKEN, or a random one when unset.
  """
  use Mix.Task

  @categories ~w(single_hop multi_hop temporal file adversarial)

  @impl true
  def run(argv) do
    {opts, _, invalid} =
      OptionParser.parse(argv, strict: [only: :string, json: :string, mode: :string])

    if invalid != [], do: Mix.raise("unknown options: #{inspect(invalid)}")

    mode = opts[:mode] || "both"

    if mode not in ~w(default lean both),
      do: Mix.raise("unknown mode #{inspect(mode)}; one of default, lean, both")

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

    # The lean run loads the fixture again, into a workspace of its own.
    runs =
      [
        {"default", [], nil},
        {"lean",
         [workspace: "eval-ask-graph-lean", ask_args: DeciduousMcp.Eval.AskGraph.lean_args()],
         "lean mode: detail lean, include_retired false"}
      ]
      |> Enum.filter(fn {name, _, _} -> mode in [name, "both"] end)

    try do
      results =
        for {_name, run_opts, label} <- runs do
          result = DeciduousMcp.Eval.AskGraph.run([only: only] ++ run_opts)
          Mix.shell().info(DeciduousMcp.Eval.AskGraph.format(result, label))
          result
        end

      if path = opts[:json] do
        File.write!(path, Jason.encode_to_iodata!(hd(results), pretty: true))
        Mix.shell().info("wrote #{path}")
      end

      with [default, lean] <- results,
           [_ | _] = worse <- DeciduousMcp.Eval.AskGraph.lean_regressions(default, lean) do
        Mix.raise(
          "lean read scored below the default read: " <>
            Enum.map_join(worse, ", ", fn {m, d, l} -> "#{m} #{d} -> #{l}" end)
        )
      end
    after
      Ecto.Adapters.SQL.Sandbox.checkin(repo)
    end
  end
end

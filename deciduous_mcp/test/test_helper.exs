# :eval tests measure retrieval quality and take seconds; `mix test --only eval`.
ExUnit.start(exclude: [:eval])
Ecto.Adapters.SQL.Sandbox.mode(DeciduousMcp.Repo, :manual)

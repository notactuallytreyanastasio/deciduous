defmodule DeciduousMcp.Repo do
  use Ecto.Repo,
    otp_app: :deciduous_mcp,
    adapter: Ecto.Adapters.Postgres
end

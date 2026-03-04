defmodule Deciduex.Repo do
  use Ecto.Repo, otp_app: :deciduex, adapter: Ecto.Adapters.SQLite3
end

defmodule DeciduousMcp.Repo.Migrations.NamedTokens do
  use Ecto.Migration

  @moduledoc """
  A token per person or agent, beside the one in DECIDUOUS_MCP_TOKEN.

  The server had one bearer token. Everyone on a team shared it, so nothing
  the server recorded could say who made a write, and taking one person's
  access away meant changing the token for all of them.

  `api_tokens` holds the SHA-256 of each named token, never the token: a
  token is 32 random bytes, so a fast hash is enough and a dump of this
  table opens nothing. A name is kept after its token is revoked and cannot
  be issued again, because the name is what `write_activity.actor` records
  and a reused name would make two people one.
  """

  def up do
    create table(:api_tokens, primary_key: false) do
      add :name, :text, primary_key: true
      add :token_hash, :binary, null: false

      add :created_at, :utc_datetime_usec,
        null: false,
        default: fragment("(now() AT TIME ZONE 'UTC')")

      add :revoked_at, :utc_datetime_usec
      add :last_used_at, :utc_datetime_usec
    end

    create unique_index(:api_tokens, [:token_hash])

    create constraint(:api_tokens, :api_tokens_name_bounds,
             check: "char_length(name) >= 1 AND char_length(name) <= 100"
           )

    alter table(:write_activity) do
      add :actor, :text
    end
  end

  def down do
    alter table(:write_activity) do
      remove :actor
    end

    drop table(:api_tokens)
  end
end

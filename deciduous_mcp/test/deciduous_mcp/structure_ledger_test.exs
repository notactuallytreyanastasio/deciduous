defmodule DeciduousMcp.StructureLedgerTest do
  @moduledoc """
  STRUCTURE.sql is what a fresh install loads instead of running every
  migration, so it must describe the database the migrations produce.
  Release acceptance diffs a full pg_dump against it, but only at tag time:
  1.0.8 added five migrations across five PRs, and the release run was the
  first thing to notice. This test catches the usual cause, a migration
  missing from STRUCTURE.sql's schema_migrations ledger, in plain `mix test`
  with no database or pg_dump. The full content diff runs on every PR in the
  "STRUCTURE.sql matches the migrations" CI job.
  """
  use ExUnit.Case, async: true

  @root Path.expand("../..", __DIR__)

  # The release image's test stage copies lib/, test/ and priv/ but not
  # STRUCTURE.sql; release acceptance runs test/release/check_structure.sh,
  # the full diff, beside it. A checkout always has the file.
  unless File.exists?(Path.join(@root, "STRUCTURE.sql")) do
    @moduletag skip: "STRUCTURE.sql is not in this build (the release image); check_structure.sh covers it there"
  end

  test "every migration is in STRUCTURE.sql's ledger, and nothing else is" do
    migrations =
      Path.join(@root, "priv/repo/migrations/*.exs")
      |> Path.wildcard()
      |> Enum.map(&(&1 |> Path.basename() |> String.split("_", parts: 2) |> hd()))
      |> MapSet.new()

    ledger =
      Regex.scan(~r/^\s*\((\d{14}), NOW\(\)\)/m, File.read!(Path.join(@root, "STRUCTURE.sql")))
      |> Enum.map(fn [_, v] -> v end)
      |> MapSet.new()

    missing = MapSet.difference(migrations, ledger) |> Enum.sort()
    stale = MapSet.difference(ledger, migrations) |> Enum.sort()

    assert missing == [] and stale == [],
           "STRUCTURE.sql is out of date. Migrations missing from its ledger: " <>
             "#{inspect(missing)}; ledger entries with no migration: #{inspect(stale)}. " <>
             "Regenerate it: migrate a fresh Postgres 17.11 database, then run " <>
             "scripts/dump_structure.sh (see docs/ARCHITECTURE.md)."
  end
end

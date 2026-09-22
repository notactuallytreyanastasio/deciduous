# Script for populating the database.
#
# You can run it with:
#     mix run priv/repo/seeds.exs
#
# Creates a default workspace if none exists.

alias DeciduousMcp.Repo
alias DeciduousMcp.Schema.Workspace

unless Repo.one(Ecto.Query.from(w in Workspace, where: w.name == "default")) do
  Repo.insert!(%Workspace{
    name: "default",
    description: "Default workspace for decision graph tracking"
  })

  IO.puts("Created default workspace")
end

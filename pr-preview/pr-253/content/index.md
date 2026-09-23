# Shared memory for a team of agents

Give each coding agent its own branch and connect them to the same Deciduous workspace. They can read the decisions already made, record what they learn, and credit the findings they reuse.

The shared service stores the graph in Postgres. Agents reach it over authenticated HTTP through MCP. The database stays behind the service; agents do not need a database password.

## Start with a working team

1. [Run Postgres and the shared service on your machine](local-postgres.md). The guide covers persistent storage, credentials, health checks, and a first connection.
2. [Connect your agent clients](clients.md) to the same workspace. Give each agent a distinct branch name.
3. [Run a two-agent handoff](teams.md). One agent records a decision; the other reads it and links the work that uses it.

Already have a server? [The quickstart](quickstart.md) begins with its address and an access token. Already have local graphs? [Upgrade and import them](upgrading.md) with backups and a preview before changing anything.

## Three separate responsibilities

| Component | What it owns | What you configure |
| --- | --- | --- |
| Your agents | Code in separate worktrees, decisions and evidence | HTTP MCP endpoint, bearer token, workspace, branch |
| Deciduous service | Graph tools, project routing, writes and live events | Service token, database connection, HTTP listener |
| Postgres | Shared graph and document storage | Database role, persistent volume, backups |

One service can hold several workspaces. Use the same explicit workspace name across clones and machines. Workspaces organize the graph; the shared bearer token is a service-wide trust boundary, not a per-workspace permission system. Run separate services for teams that should not read each other's data.

## Read before writing

An agent starting work should check recent activity and query the relevant part of the graph. It records the goal, options and chosen decision before implementation, then attaches actions and outcomes as the work proceeds. A later agent can follow those connections without the original chat transcript.

[Working as a team](teams.md) covers branch identities and reused findings. [The decision workflow](workflow.md) covers the graph itself. [Recovering context](recovery.md) shows how a new session picks up the same reasoning.

## Know which store you are using

In Deciduous 1.0, the HTTP MCP server writes to Postgres. Ordinary commands such as `deciduous add`, `deciduous link`, and the local stdio `deciduous mcp` still use SQLite. Running `deciduous remote init` does not redirect those commands to Postgres.

Use the HTTP MCP connection for shared agent work. The Rust CLI's `remote` commands configure the connection, compare stores, transfer graph data, and watch events. [The reference](reference.md) spells out which commands contact the server.

You can keep using [SQLite for solo or offline work](solo.md). Treat moving between the two stores as an explicit operation. A pull changes local data; a push can overwrite corresponding remote records. The [upgrade guide](upgrading.md) explains the checks and limitations.

## Take the same setup to a remote host

[Remote hosting](remote-postgres.md) is the short version: put the service behind HTTPS, keep Postgres on a private network, distribute the service token through a secret store, and test a restore from backup. Your agents then use the new service address with the same workspace conventions.

The [architecture guide](architecture.md) documents the trust boundaries and the current TLS limitations of the database client. [Troubleshooting](troubleshooting.md) covers connection failures, missing records, locks, and split local/remote histories.

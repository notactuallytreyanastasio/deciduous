# Deciduous

Many minds. One memory.

Deciduous gives a team of coding agents a shared record of decisions. Each agent works on its own branch, reads what the others have learned, and links its work to the findings it uses. A new session can recover the reasoning from the graph.

[Watch the agents work](https://deciduous.dev/) · [Read the docs](https://deciduous.dev/tutorial/) · [Apache-2.0](LICENSE)

## Start with shared Postgres memory

For an agent team, run the Deciduous HTTP MCP service with Postgres. Agents use the service endpoint and a bearer token; only the service needs database credentials. One service can hold several workspaces, but its token grants service-wide access. Use separate deployments for separate trust boundaries.

1. [Set up Postgres and the service locally](https://deciduous.dev/tutorial/local-postgres.html). The guide includes persistent storage, private credentials, health checks, and a tested connection.
2. [Connect your agents](https://deciduous.dev/tutorial/clients.html). Claude Code, Codex, and Cursor can use the same HTTP MCP endpoint and workspace.
3. [Run a two-agent handoff](https://deciduous.dev/tutorial/teams.html). One agent records a decision; another reads it, reuses it, and records a `took_from` edge.

Already running a server? Use the [quickstart](https://deciduous.dev/tutorial/quickstart.html). To move the service off your laptop, follow the brief [remote deployment guide](https://deciduous.dev/tutorial/remote-postgres.html).

## Install the CLI

Homebrew:

```sh
brew tap notactuallytreyanastasio/tap
brew install deciduous
```

Cargo:

```sh
cargo install deciduous --locked
```

Prebuilt macOS, Linux, and Windows binaries are on [GitHub Releases](https://github.com/notactuallytreyanastasio/deciduous/releases). Use the matching platform asset and verify it against the release checksums.

Installing the CLI does not start Postgres or migrate a local graph. Continue with the [local setup guide](https://deciduous.dev/tutorial/local-postgres.html). Existing users should read the [upgrade and migration guide](https://deciduous.dev/tutorial/upgrading.html) before transferring data.

## Know which commands share memory

| Interface | Store |
| --- | --- |
| HTTP MCP service in `deciduous_mcp/` | Shared Postgres |
| `deciduous remote status/push/pull/watch` | Explicit server operations |
| Ordinary `deciduous add/link/status` and `deciduous mcp` over stdio | Local SQLite |
| `deciduous sync` | Local SQLite and Git graph records |

`deciduous remote init` saves the server address and workspace. It does not redirect ordinary CLI writes or the stdio MCP server. Use HTTP MCP for shared team writes. A remote pull refreshes local nodes and edges; it is not a complete replica of attachments, themes, or history.

[CLI and API reference](https://deciduous.dev/tutorial/reference.html) covers the supported operations. [Solo and offline use](https://deciduous.dev/tutorial/solo.html) explains the SQLite workflow.

## Work from connected decisions

At session start, an agent checks activity and reads the relevant goal and decision nodes. It records the chosen approach before implementation and attaches an action. After testing, it records an outcome with the command, result, and any remaining gap. A `revisit` keeps the earlier reasoning when an approach changes.

The shared server has 18 MCP tools, including `query_nodes`, `show_node`, `check_activity`, `log_decision`, `log_observation`, and `close_thread`. Branch write leases coordinate short graph writes. They do not lock source files or replace task assignment by your agent runner.

- [Record the reasoning](https://deciduous.dev/tutorial/workflow.html)
- [Recover context](https://deciduous.dev/tutorial/recovery.html)
- [Attach evidence](https://deciduous.dev/tutorial/evidence.html)
- [Architecture and trust boundaries](https://deciduous.dev/tutorial/architecture.html)

The [Tetris arena](https://notactuallytreyanastasio.github.io/tetris-arena/) shows ten agents building on separate branches while reading one graph. The experiment includes the failures as well as the shared findings.

## Upgrade with a backup

The setup helpers default to a preview for state-changing operations. They generate private credentials, preserve the Postgres volume, and back up before replacing the service. The SQLite migration helper copies the database before exporting, uploads referenced attachment bytes, and verifies the imported workspace. It refuses to merge a stale graph into a populated workspace.

The graph export is not a full database backup. Keep SQLite and its attachments, make a Postgres dump, and test a restore before cutover. The [upgrade guide](https://deciduous.dev/tutorial/upgrading.html) lists the data the migration does not preserve.

## Develop Deciduous

```sh
cargo test --locked
cargo clippy --locked
cargo build --release --locked
```

The shared service lives in `deciduous_mcp/`. Read the [development guide](https://deciduous.dev/tutorial/developer.html) for its test database and Elixir commands. Never run a migration or test reset against production.

Docs are Markdown in `docs/content/`, rendered into static HTML:

```sh
npm ci --prefix scripts/docs
npm run build --prefix scripts/docs
npm run check --prefix scripts/docs
python3 -m unittest discover -s scripts/team-memory/tests -v
```

The docs build preserves old tutorial URLs, checks local links, and produces the public helper bundle. See [scripts/docs/README.md](scripts/docs/README.md) for previewing and [scripts/team-memory/README.md](scripts/team-memory/README.md) for script tests.

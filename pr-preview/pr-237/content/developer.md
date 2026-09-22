# Develop the shared graph service

Deciduous has two implementations to account for: the shared Postgres service in `deciduous_mcp/` and the Rust local CLI in `src/`. The team workflow depends on the former. Avoid changing one interface and assuming the other inherited the behavior.

Use a disposable local stack from [local Postgres setup](local-postgres.md). Do not point tests or migration experiments at a production database.

## Repository layout

| Directory | What to change there |
| --- | --- |
| `deciduous_mcp/lib/deciduous_mcp/mcp/tools/` | Shared HTTP MCP tool schemas and handlers |
| `deciduous_mcp/lib/deciduous_mcp/graph/` | Postgres graph operations |
| `deciduous_mcp/lib/deciduous_mcp/web/` | HTTP auth, routes, sessions, workspace pinning, event socket |
| `deciduous_mcp/priv/repo/migrations/` | Postgres schema and triggers |
| `deciduous_mcp/test/` | Shared-service tests |
| `src/remote.rs`, `src/watch.rs` | CLI remote configuration, transfer, event client |
| `src/mcp/` | Local SQLite MCP tools and protocol |
| `src/db.rs`, `src/records.rs` | SQLite and optional Git record synchronization |
| `web/`, `src/viewer.html` | Local graph viewer source and embedded build |
| `docs/content/` | Canonical documentation source |

## Work on the server

The Mix project declares Elixir `~> 1.16`; the supplied container builds with Elixir 1.18.4 and Erlang/OTP 28.0. Use the pinned container recipe when you want the same release environment as deployment.

For a native development environment, install Elixir/Erlang and run a disposable Postgres instance compatible with `deciduous_mcp/config/dev.exs` and `test.exs`. Those files use local development credentials and separate `deciduous_mcp_dev` and `deciduous_mcp_test` databases. They are not production examples. The `DATABASE_URL` override in `runtime.exs` applies in production mode.

From `deciduous_mcp/`, with a development-only token supplied through the environment:

```sh
mix deps.get
mix ecto.create
mix ecto.migrate
mix test
mix format --check-formatted
```

The application requires `DECIDUOUS_MCP_TOKEN` even during tests. Supply a generated development token of at least 32 bytes, not a production credential. `mix test` creates and migrates its configured test database before the tests run. Check its target before invoking it. `mix ecto.reset` drops data and is not part of an upgrade procedure.

Run the development service with `mix run --no-halt` after migrations. The default HTTP port is 4000. Use the [reference](reference.md#http-endpoints) to distinguish `/health`, `/mcp`, and `/events`.

## Add or change a shared tool

Create the tool under `mcp/tools/` using `DeciduousMcp.MCP.Component`, then register it in `mcp/server.ex`. Registration in the convenience `Tools` list alone does not expose it to clients. Check the live `tools/list` result after rebuilding.

For a scoped read, resolve the workspace through `Scope.read_scope`. For a new-node write, use `Scope.write_workspace_id`. For an existing-node mutation, use `Scope.write_scope_for_node` so the lease matches the node's workspace and the header pin is checked.

Keep an actual transaction around any operation documented as all-or-nothing. Several existing convenience helpers perform sequential writes despite their schema descriptions; do not copy that atomicity claim into a new tool without testing rollback behavior.

Add tests for missing or invalid arguments, the correct workspace, header/argument precedence, conflicting sessions on one branch, independent branches, and partial-failure behavior. For borrowed ideas, verify both the source resolution and the `took_from` edge direction.

The custom component module preserves the JSON Schema in `tools/list` and derives a looser Peri validation schema for incoming arguments. Required fields and basic scalar types are checked, while arrays and objects use a permissive type. Do not assume the advertised schema alone enforces every enum or nested constraint. Validate in the handler or graph changeset and test rejection paths.

## Test the HTTP boundary

Check a release through its public routes, not only by calling a tool module in a unit test:

- `/health` answers without a token; protected routes reject missing or wrong credentials.
- MCP initialization and `tools/list` succeed; arguments such as `workspace`, `branch`, and search filters reach the handler.
- A scoped write can be read by a second client in the same workspace.
- Stale MCP sessions fail fast and clients can initialize again.
- `GET /mcp` returns the expected 405; normal tool calls still work.
- `/events` receives a real write and a disconnected client can recover through graph queries.
- Blob upload validates content hashes; document reads do not expose private content through caches.

Do not interpret a workspace-header test as proof of tenant authorization. The current service is a shared trust boundary. Test any future access-control change across ID-based reads, workspace listing, imports, document access, and all write paths.

## Work on the Rust CLI

From the repository root:

```sh
cargo test
cargo build --release
cargo clippy
./target/release/deciduous --version
./target/release/deciduous remote watch --help
```

Use the built binary for command verification. A `deciduous` elsewhere on your PATH may be an older release with different behavior. In particular, 0.19's `remote watch` printed a URL; 1.0 connects and streams by default.

Use a temporary test repository and database for mutation tests. Do not run `deciduous update`, migrations, or import experiments against a user's working checkout as a substitute for a fixture.

## Viewer changes

The viewer reads the local graph. A viewer build does not add a Postgres connection or make `remote pull` automatic. If you change `web/src/`, rebuild and update the embedded HTML locations required by the repository's `AGENTS.md`, then test the local server and static demo. Keep graph data fixtures free of private prompts and documents.

## Release and upgrades

Keep the Rust and Elixir versioned interfaces documented. A server migration and a CLI update are distinct operations. Back up Postgres before a schema upgrade, test restoration into a separate database, and retain the prior application image until the new release is verified. Follow [upgrading](upgrading.md) for the supported operator procedure.

The container runs `DeciduousMcp.Release.migrate()` before it starts the release. Review a new migration for data loss, locking, and compatibility with the old application before relying on that startup behavior. Do not promise that rolling back an image reverses a schema migration.

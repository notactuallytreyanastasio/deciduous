# Develop deciduous

The repository holds three things that are built and tested separately: the Rust CLI at the root (`cargo`), the graph server in `deciduous_mcp/` (Elixir, `mix`), and two web builds, the graph viewer in `web/` and this documentation site in `scripts/docs/`. A change to a tool's behaviour usually needs both the Rust and the Elixir side: the CLI's `deciduous mcp` and the server's `/mcp` are separate implementations with different tool sets ([MCP tools](mcp.md)).

## The CLI

The toolchain is pinned in `rust-toolchain.toml` to 1.92.0, with clippy and rustfmt; rustup picks it up from the file. Stable is pinned on purpose: a newer clippy reported 29 errors on an unchanged tree once.

```text
$ cargo build
   Compiling deciduous v1.1.0 (/Users/you/code/deciduous)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.36s
```

(an incremental build; a clean one takes minutes). Before committing, run what CI runs (`.github/workflows/type-check.yml`); the repository's `CLAUDE.md` lists only the last three, and a branch that passes those can still fail CI on formatting or on a clippy warning:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./scripts/validate-types.sh   # needs jq
```

Use `./target/debug/deciduous` or `./target/release/deciduous` when checking a change. The `deciduous` on your PATH is whatever you installed, and it will happily answer instead.

### `cargo test` on a Mac with Homebrew

`tests/git_sync_integration.rs` drives real git with the freshly built binary as the merge driver. It builds the PATH for each simulated developer from the directory holding `git`, and refuses to run if that directory also holds an installed `deciduous`, which would stand in for the binary under test. With Homebrew's git and Homebrew's deciduous both in `/opt/homebrew/bin`, every test in that file fails at once:

```text
thread 'a_merge_whose_ancestor_will_not_parse_is_refused_not_decided_by_timestamp' panicked at tests/git_sync_integration.rs:43:5:
git shares a directory with an installed deciduous (/opt/homebrew/bin)
```

That is the test protecting itself, not a bug. Put the system git first:

```bash
PATH=/usr/bin:$PATH cargo test
```

```text
$ PATH=/usr/bin:$PATH cargo test --test git_sync_integration a_merge_whose_ancestor
test a_merge_whose_ancestor_will_not_parse_is_refused_not_decided_by_timestamp ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 23 filtered out; finished in 3.09s
```

The full suite run that way passed 687 tests on 2026-11-08. It is slow; filter by test binary (`--test <name>`) or name while iterating.

Tests that run `deciduous init` in a temporary directory need `DECIDUOUS_NO_SERVER=1`, or `init` stops at the server step ([Troubleshooting](troubleshooting.md#init-stops-with-no-terminal-to-ask)).

### Templates

`deciduous init` and `update` write files from templates in `src/init/`. `src/init/known_templates.rs` is generated: the sha256 of every template ever shipped, which is how `update` tells a file deciduous wrote from one a user edited. It is built from every template text in the repository's git history, so regenerate it after a template change is committed: `python3 scripts/gen_known_templates.py > src/init/known_templates.rs`. A template text missing from the list is one `update` will treat as the user's and keep.

## The graph server

`deciduous_mcp/` is an Elixir application (`mix.exs` declares Elixir `~> 1.16`; CI uses Elixir 1.18.4 on OTP 28.3.3). It refuses to boot without a `DECIDUOUS_MCP_TOKEN` of at least 32 bytes, tests included. `mix test` creates and migrates its database first (the `test` alias), using `PGHOST`, `PGUSER`, `PGPASSWORD` (default `postgres`/`postgres` on localhost) and database `deciduous_mcp_test`. CI's job, in `.github/workflows/integration.yml`:

```bash
cd deciduous_mcp
export MIX_ENV=test PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres
export DECIDUOUS_MCP_TOKEN=mix-test-token-not-a-secret-0000000000000000
mix local.hex --force
mix local.rebar --force
mix deps.get
mix test
```

against a `postgres:17-alpine` service. Without the two `local.*` lines, `mix deps.get` on a fresh Elixir install stops to ask before installing Hex. Development (`config/dev.exs`) uses `deciduous_mcp_dev` with the same credentials. Neither was run for this page.

Things that are easy to get wrong here:

- A new tool lives in `lib/deciduous_mcp/mcp/tools/` and has to be registered with the server to appear in `tools/list`. Check the live list after a rebuild; the CLI's tools are a different list.
- Every tool's `workspace` argument is merged in by `DeciduousMcp.MCP.Scope`; resolve reads and writes through it so the `X-Deciduous-Workspace` pin is honoured.
- Unknown arguments are refused by name. A new argument has to be in the schema or clients get `has no argument`.
- A migration needs `STRUCTURE.sql` regenerated with it (from `deciduous_mcp/`, against a fully migrated, disposable PostgreSQL 17: `DATABASE_URL=ecto://postgres:postgres@localhost/deciduous_schema_check ./scripts/dump_structure.sh`, or `PGHOST` plus `PGDATABASE`; it exits without one of them). CI's `structure` job fails when they disagree.
- Server dependencies are audited at release: `mix hex.audit` must be clean or the self-contained server build fails.

Release executables are built with Burrito, which needs Zig 0.16.0 on PATH. `cd deciduous_mcp && MIX_ENV=prod BURRITO_TARGET=darwin_arm64 mix release deciduous_mcp` writes `burrito_out/deciduous_mcp_darwin_arm64`; without `BURRITO_TARGET`, `mix release` makes an ordinary release and no executable. `docker build -f deciduous_mcp/Dockerfile.burrito deciduous_mcp` builds only the Linux executable for the build machine's architecture. Neither was run for this page. To try an unreleased server through the CLI's own installer, point `DECIDUOUS_SERVER_BINARY` at that file (or `DECIDUOUS_SERVER_BUNDLE` at a Docker bundle) before `remote setup --local`. That installs a service on your machine; do it on a machine where that is what you want. `deciduous_mcp/DEPLOY.md` documents every server setting.

## The web viewer

`web/` is React and TypeScript in one `App.tsx`, built by Vite into a single HTML file that the CLI embeds. After changing anything under `web/src/`:

```bash
cd web && npm run build && cd ..
cp web/dist/index.html src/viewer.html
cp web/dist/index.html docs/demo/index.html
cargo test
cargo build --release
```

`deciduous serve` then serves the new viewer from the rebuilt binary. `npm run typecheck` and `npm test` (Vitest) run in `web/`. `scripts/validate-types.sh` checks that the TypeScript types in `web/src/types/` still match the Rust ones. These steps are from the repository's `CLAUDE.md` and `web/package.json`; they were not run for this page.

## This documentation

The site is generated from `docs/content/*.md` by `scripts/docs/`. Navigation and labels are in `scripts/docs/pages.json`; each page needs exactly one `# H1`, and its first paragraph becomes the page description. Links between pages are relative (`[Architecture](architecture.md)`).

```bash
npm ci --prefix scripts/docs
npm run build --prefix scripts/docs   # writes the HTML
npm run check --prefix scripts/docs   # verifies generated files match, then runs test.mjs
```

Generated HTML and the search index are committed with the Markdown; CI fails if they disagree. To look at it: `python3 -m http.server 8766 --directory docs --bind 127.0.0.1`.

## Releasing

The steps (both version numbers, both changelogs, `mix hex.audit`, tagging by hand from the merge commit, and watching `release.yml` until crates.io and the Homebrew tap show the version) are in the repository's `CLAUDE.md` under "Release Process". A release that looks finished may have published nothing; check `gh release view vX.Y.Z`, crates.io and the tap.

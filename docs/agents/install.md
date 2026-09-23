# Install

You need one binary, `deciduous`: the CLI that sets up a project, points it at
the shared server and installs the agent instructions. The shared server is a
separate program, covered in [Shared graph](shared-graph.md), and most machines
never run it.

## Check what is already there

```sh
which -a deciduous
deciduous --version
```

`which -a` lists every copy on `PATH`. Machines often have two, one from
cargo and one from Homebrew, and the first one wins. If they report different
versions, tell the user which one runs, and fix `PATH` or remove the stale copy
rather than installing a third.

Use **1.0.3 or newer**. 1.0.2 installed hooks that deny an agent's tool calls
until it writes to the graph; 1.0.3 removes them, and `deciduous update` on
1.0.3 takes them out of a project that has them.

## Install or upgrade

Pick the channel the machine already uses. Otherwise use Homebrew on macOS and
cargo when a Rust toolchain is present.

```sh
# Homebrew (macOS, Linux)
brew install notactuallytreyanastasio/tap/deciduous
brew upgrade deciduous

# crates.io (needs a Rust toolchain)
cargo install deciduous --locked

# A release binary, no toolchain needed
# https://github.com/notactuallytreyanastasio/deciduous/releases/latest
#   deciduous-darwin-arm64   deciduous-darwin-amd64
#   deciduous-linux-arm64    deciduous-linux-amd64
#   deciduous-windows-amd64.exe
#   checksums.txt            (verify before running)
```

Then confirm:

```sh
deciduous --version
# deciduous 1.0.2
```

## Upgrading a project that was set up by an older version

The binary and each project's integration files are versioned separately.
After upgrading the binary, run this in each project:

```sh
deciduous check-update   # exit 0: files current; exit 1: run update
deciduous update
```

On 1.0.3, `deciduous update` replaces only files deciduous wrote
(`.claude/commands/`, `.claude/skills/`, the section of `CLAUDE.md` between
`<!-- deciduous:start -->` and `<!-- deciduous:end -->`). A file the user
changed is kept; Markdown gets the new template appended in a marked block.
Every file it changes is copied to `.deciduous/update-backups/<time>/` first.
It removes the logging hook scripts it once installed and their entries in
`.claude/settings.json`, and keeps any hook script the user wrote. In a project
with a `[remote]` it leaves `.gitignore` and `.gitattributes` alone. `deciduous update --all ~/code` does every project under a directory.
Show the user `git diff` afterwards. The changes are theirs to commit.

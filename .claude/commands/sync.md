---
description: Check this project's copy of the decision graph against the graph server and refresh it
allowed-tools: Bash(deciduous:*)
---

# Sync with the Graph Server

This project has a `[remote]` in `.deciduous/config.toml`. Its graph lives on a shared server, and `.deciduous/deciduous.db` is this machine's cache of it. There is no `.deciduous/graph.json` to pull, commit or merge, and `deciduous sync` is not the step here: CLI writes are queued in `.deciduous/remote-log.jsonl` and sent before each command exits, MCP tools write to the server directly.

## Step 1: What differs?

```bash
deciduous remote status
```

Lists the writes waiting in the log, the ones the server refused, and every node, edge and document that differs between this copy and the server, each with the command that fixes it. Exits 1 when anything differs.

## Step 2: Refresh

```bash
deciduous remote pull
```

Sends what is waiting, then refreshes the local cache from the server, deletions included.

## Step 3: Send what is stuck

```bash
deciduous remote push                   # writes still waiting (the server was down)
deciduous remote push --seed            # rows only this copy has, such as history from before the remote
deciduous remote push --retry-rejected  # ops the server refused, once the cause is fixed
```

## Linking across users

Local ids differ per machine. Refer to someone else's node by its change_id prefix (the CHANGE column in `deciduous nodes`) or by the server id an agent quotes:

```bash
deciduous link a1b2c3d4 42 -r "our action implements their goal"
```

Agent messages (`deciduous board`) are not part of the graph; with a `[remote]` they live on the server.

## Quick Reference

| Command | What it does |
|---------|--------------|
| `deciduous remote status` | What differs, and the command that fixes each; exit 1 if anything |
| `deciduous remote pull` | Send what is waiting, then refresh the local cache |
| `deciduous remote push` | Send what is waiting in the log |
| `deciduous nodes` | Shows the CHANGE prefix to use for cross-user links |

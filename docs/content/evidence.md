# Attach the evidence, or derive the graph from it

A node is only as good as what backs it. Deciduous has two ways to put evidence next to a claim: attach a file to a node with `deciduous doc attach`, which in a project with a `[remote]` uploads the bytes to the server; and `deciduous transcript import`, which reads a Claude Code session's transcript and writes the goals, actions and outcomes it can prove, each marked with what it rests on.

## Say what ran

Before either, the cheapest evidence is a precise outcome: the command, the count, the commit, and what was not checked.

```javascript
close_thread({
  workspace: "my-app", branch: "agent-tests",
  parent_node_id: "<action id>",
  title: "Request validation passes 8 integration cases",
  description: "npm test -- tests/items.test.ts: 8 passed, 0 failed. Browser behaviour not checked.",
  success: true
})
```

"All tests pass" after running one file is the claim the next agent will wrongly rely on. A `files` list and a `commit` on a node are references, not copies: the reader needs the repository to follow them.

## Documents from the CLI reach the server

The server's MCP tools have no upload: 21 tools, none of them attaches a file. Attaching is a CLI command, and agents run it like anyone else.

```console
$ deciduous doc attach 5ee7c7c7 test-output.txt -d "cargo test retry output"
Attached document 1 to node 6 (test-output.txt)
  Description: cargo test retry output
$ deciduous remote status | tail -2
In sync: no writes waiting, and every node, edge and document matches field by field.
Themes and tags are not sent to the server, so they are not compared.
```

The node can be named by local id, `change_id` prefix, or server UUID (`deciduous doc attach de6ccf4c-877b-... file`). The bytes are uploaded to the server, which checks them against their SHA-256, and the attachment is recorded on the node. An agent then sees it in `show_node`:

```json
"documents": [{ "id": "38c69819-818d-4a4f-906c-1e8c6101b889", "filename": "test-output.txt", "mime_type": "text/plain" }]
```

and anyone with the token can download it from `<url>/documents/<id>`, where `<url>` is `url` under `[remote]` in `.deciduous/config.toml`, including any path prefix the server's proxy uses, such as `/deciduous-mcp`:

```console
$ curl -s -i -H "Authorization: Bearer <token>" https://deciduous.example.com/documents/38c69819-818d-4a4f-906c-1e8c6101b889
HTTP/1.1 200 OK
cache-control: no-store, private, max-age=0
content-type: text/plain

test retry ... ok
4 passed; 0 failed
```

Without the token: 401. An unknown id: 404. Behind a Caddy that has only a `handle_path` block ([Run a server](run-a-server.md#3-put-https-in-front-of-it)), leaving out the path prefix gets an empty `200`, not an error. One file can be at most 64 MiB; the server refuses a larger upload with 413. `get_graph` lists the workspace's documents with size, description and node.

Older pages said `doc attach` was local only and that configuring a remote did not upload. That stopped being true; in 1.1.0 the upload happens before the command exits, like every other CLI write.

What does not work in 1.1.0:

- **Documents do not come down.** `deciduous remote pull` in another clone fetched every node and edge and no documents: `deciduous doc list` there said `No documents found.`, and `remote status` in that clone still said "every node, edge and document matches", which was not so. Read other people's attachments with `show_node` and `GET /documents/<id>`.
- **A detached document is still served.** `deciduous doc detach 3` reached the server: the document disappeared from `get_graph`. But `show_node` on its node still listed it, and `GET /documents/<id>` still returned its bytes with 200. If you attached a secret, detaching it does not take it off the server. Whoever runs the server has to delete it from the database: [Remove a document that should never have been attached](operating.md#remove-a-document-that-should-never-have-been-attached).

The token grants access to every workspace on the server, documents included ([Architecture](architecture.md)). Do not attach `.env` files, credential stores, connection strings, or output you have not read.

`deciduous doc describe` reaches the server, like `attach` and `detach`: after `doc describe 1 "second description"`, `get_graph` showed the new description. `doc list`, `doc show`, `doc open` and `doc gc` operate on this checkout's copy in `.deciduous/documents/`.

## Deriving the graph from a transcript

Claude Code writes every prompt, edit, command and result of a session to a JSONL file under `~/.claude/projects/<project>/`. `deciduous transcript import` derives nodes from it: a goal per prompt with the prompt verbatim, actions from edit runs and commits, outcomes from test and build runs with their exit status, and an outcome from the assistant's closing report. Each node carries `metadata.evidence.kind`: `proven` (a tool call and its result) or `inferred` (the assistant said so).

What it cannot derive is an option or a decision. Whether one approach was chosen over another is a judgement about prose, so those still have to be logged as they happen.

Start with the list and a dry run. `transcript list` reads the directory for the current project:

```console
$ deciduous transcript list
/Users/me/.claude/projects/-Users-me-code-my-app
new      6494dc2d    2 prompts  2026-11-08  <session title>
new      305de803    1 prompts  2026-11-05  <session title>

$ deciduous transcript import --dry-run 6494dc2d
Would import <session title> (6494dc2d, 2 prompts)
  goals 2 · actions 11 · outcomes 4 proven, 1 inferred · observations 0 · edges 17
  10 files hashed · 3 subagent runs skipped (their transcripts are separate files) · 15 deciduous MCP writes skipped (already in the graph)
```

It takes a session id (looked up in this project's directory) or a path to a `.jsonl`. The import leaves out every deciduous MCP call the session made, reads such as `query_nodes` included, and every shell command that runs `deciduous`, on the assumption that they already wrote to the graph. It does not check. Imported into a different workspace or server than the session wrote to, or after a write that failed, those records are simply missing; the count still says "already in the graph". Subagents' transcripts are separate files and are not followed. Actions record a hash of each file they touched; `--no-hash` skips that.

In a directory Claude Code has never run in, 1.1.0's `transcript list` fails with the raw `No such file or directory (os error 2)` for the directory it expected (the next release says Claude Code has not run there). Pass `--dir` to `list`; `import` has no `--dir`, so give it the `.jsonl` path.

A real import, into a workspace on the server:

```console
$ deciduous transcript import ~/.claude/projects/<project>/305de803-....jsonl
Imported <session title> (305de803, 1 prompts)
  goals 1 · actions 0 · outcomes 0 proven, 1 inferred · observations 0 · edges 1
  1 deciduous MCP writes skipped (already in the graph)
  root goal #1: deciduous show 1
$ deciduous remote status | tail -2
In sync: no writes waiting, and every node, edge and document matches field by field.
```

The nodes went to the server like any CLI write. On the server, the goal's metadata:

```json
{ "branch": "main",
  "evidence": { "kind": "proven", "source": "transcript", "session": "305de803-...", "uuids": ["fd7edbe1-..."] },
  "session": { "id": "305de803-...", "transcript": "/Users/me/.claude/projects/.../305de803-....jsonl", "turns": 1 } }
```

and the outcome's `evidence.kind` was `inferred`, from the assistant's report at the end of the turn. The nodes keep the transcript's timestamps (`created_at` 2026-11-05 for a session from that day), while `branch` is the current checkout's.

Importing the same transcript again changes nothing:

```text
  goals 0 · actions 0 · outcomes 0 proven, 0 inferred · observations 0 · edges 0
  2 already present · 1 deciduous MCP writes skipped (already in the graph)
```

Read what you are about to share. A goal holds the prompt verbatim and an inferred outcome holds the assistant's report in full; both can contain paths, hostnames and anything pasted into the session, and on a shared server every token holder can read them. Run `--dry-run` first, and import into a workspace whose readers should see that session.

The viewer does not yet show `metadata.evidence`; `deciduous show <id>` prints it (`Evidence: inferred transcript, session 305de803`), and `show_node` returns it in `metadata`.

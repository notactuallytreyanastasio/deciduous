# Give each finding evidence

Record enough evidence for another agent to check a claim without reconstructing your whole session. Put the test command and result beside the action it verifies. Keep a rejected experiment's failure reason connected to the decision it changed.

## Record the check you ran

An outcome should say what ran, on which code, and what remains untested. Avoid "all tests pass" if you ran one file.

```javascript
add_node({
  workspace: "example-app", branch: "agent-tests",
  node_type: "outcome", status: "completed",
  title: "Request validation passes 8 integration cases",
  description: "npm test -- tests/items.test.ts: 8 passed, 0 failed. Covers malformed input and the 422 response body. Browser behavior was not checked.",
  files: ["tests/items.test.ts"]
})
add_edge({
  workspace: "example-app", branch: "agent-tests",
  from_node_id: "ACTION_ID", to_node_id: "OUTCOME_ID",
  rationale: "Integration results for this implementation"
})
```

These are MCP calls. Replace `ACTION_ID` and `OUTCOME_ID` with real server UUIDs. Add `commit` to `add_node` when you have a real Git SHA; the server cannot resolve `HEAD` against your working directory.

A file path is context, not an upload. A commit reference is a string in metadata, not proof that the commit exists on the reader's machine. Make the referenced code accessible through the team's repository.

## Distinguish a source from agreement

Use a `took_from` edge when a finding changes your approach. Include why you adopted it and which part you verified. Reusing a decision does not prove it correct in a new context.

```javascript
log_observation({
  workspace: "example-app", branch: "agent-tests",
  related_to: "ACTION_ID",
  took_from: "SOURCE_DECISION_ID",
  title: "Reuse the field error codes in the request tests",
  why: "The API decision defines stable codes; tests should not depend on prose messages.",
  description: "Read the source decision and confirmed the handler emits the documented codes."
})
```

The server writes this observation and its edges in one transaction. Both references must resolve in the same workspace. A node UUID or full `change_id` works; a display prefix does not.

## Documents in the shared server

The Postgres service stores attachment metadata and document bytes separately. Byte content is keyed by SHA-256 and deduplicated in Postgres. Existing local attachments can move to the server through the [upgrade/import procedure](upgrading.md).

`show_node` lists a node's document IDs, filenames, and MIME types. `get_graph` includes document metadata. Authorized clients can fetch bytes at `GET /documents/{document-id}` or by content hash. A `410` means metadata exists but content is missing; a `404` means the requested document was not found. The download response uses private, no-store caching headers.

The current shared MCP tool set has no attachment-upload tool. `deciduous doc attach` is a local SQLite command; configuring a remote does not make it upload. `remote push` sends graph metadata but not blob bytes, and `remote pull` does not download attachment metadata or content into SQLite. Do not mistake a listed filename for a verified remote copy.

## Bring existing attachments across safely

Use the upgrade scripts before normal team writing begins. They must transfer both the graph rows and each attachment's bytes, then verify downloads by hash. The server's blob endpoint, `PUT /blob/{sha256}`, verifies that the bytes match the named hash; an upload by itself does not create a node attachment.

Keep the original `.deciduous/documents` files until the migration checks and backup are complete. Review unresolved edges and missing-content reports rather than treating a successful HTTP import as proof that all evidence arrived.

Avoid bulk-pushing a stale local graph to attach one new file after the team has started writing on the server. Import can replace matching node records. For new evidence, record a concise result plus a stable repository reference until you have a reviewed attachment-ingestion path.

## Local attachment tools

For the [solo/offline](solo.md) workflow, the CLI supports:

```bash
deciduous doc attach 42 ./evidence/validation-output.txt \
  -d "Sanitized integration test output"
deciduous doc list 42
deciduous doc show 7
```

`42` and `7` are example local IDs; use the IDs from your own graph and document listing. These commands do not operate on server UUIDs. `doc detach` soft-deletes an attachment; `doc gc` can remove orphaned files, so review and back up before cleanup.

## Review before sharing

Prompts and attachments can contain customer records, private paths, or credentials. Sanitize command output and screenshots before recording them. Do not attach `.env` files, credential stores, database connection strings, or token-bearing event URLs.

The current bearer token grants access across the server. Workspace naming and headers do not create a separate permission boundary for attachments. Use separate deployments when different groups must not see each other's evidence.

Static graph exports can publish the same sensitive text without the MCP server's authentication. Review exported JSON and generated writeups before committing them or publishing a docs site.

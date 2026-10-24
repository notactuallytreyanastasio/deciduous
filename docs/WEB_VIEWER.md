# Web Viewer

`deciduous serve` puts the local graph in a browser. The viewer is one React
page (`web/src/App.tsx`, built with Vite and embedded in the binary as
`src/viewer.html`) that groups the graph into narratives, draws the one you
pick as a tree and a D3 graph, and shows a node's full record beside it. It
reads the local database, so with a `[remote]` run `deciduous remote pull`
first to see what agents wrote on the server.

```
$ deciduous serve
🌳 Deciduous
   Graph viewer: http://localhost:3000
   Press Ctrl+C to stop
```

`-p <port>` changes the port. The server binds `127.0.0.1` and does not open a
browser for you.

---

## What is on the page

**Narratives.** The left panel lists narratives: connected trees of the graph,
each rooted at a node, as cards with the root's title, size and date range.
A selector in the header chooses how the graph is cut into them:

| Mode | Narratives are |
|---|---|
| Major Narratives (10+ nodes) | goal-rooted trees with ten or more nodes (the default) |
| All Goals | one tree per goal |
| By Git Branch | one per `metadata.branch` |
| By Key Decisions (3+ edges) | one per node with three or more edges, a hub |

Click a card to focus it: the left panel becomes an expandable tree (type,
confidence, id, title per row, edge labels on the way down) and the right
panel splits into the selected node's detail on top and a D3 graph of the
narrative below. The graph can go full screen, with its own node list and a
"commits only" filter.

**Detail panel.** For the selected node: type and title; status, confidence,
branch, commit (linked to GitHub, using `[github] commit_repo` from
`config.toml` when set), created time and change id; the files it names; its
themes as coloured chips; its attached documents (filename, size, MIME type,
description, an `(AI)` mark for descriptions the `claude` CLI wrote; listed,
not linked: the page does not fetch the file); the
description; the verbatim prompt; incoming and outgoing edges with their type
and rationale; and the raw metadata.

**Search.** Full text over titles and descriptions, newest first, with a
toggle per node type and a chip per theme. A result shows which narratives
contain it.

**Ask AI.** A dialogue that sends your question plus what is on screen (the
selected node, the visible nodes, the current branch, the focused narrative)
to `POST /api/ask`, which runs `claude -p` on the machine serving the page
and returns its Markdown answer. Without the Claude Code CLI installed the
panel shows `Claude CLI not found`. Questions and answers are kept in the
local database (`qa_interactions`) and listed through `/api/qa`.

**Edge styles.** `took_from` draws dash-dot; the legend lists every edge and
node type.

## What it does not show

- `metadata.evidence` (proven or inferred), `metadata.consulted` and
  `metadata.file_hashes` from transcript imports. They appear in the raw
  metadata block only; `deciduous show` prints them properly. CHANGELOG 1.1.0
  lists this under "Not done".
- The message board. Messages are not graph and have no API route here.
- Live updates. The page loads the graph once; reload it to see new nodes.
  There is no polling and no event stream from `serve`.
- Document bytes. `GET /api/documents/file/<id>` serves them, but nothing in
  the page calls it; `deciduous doc open <id>` does.
- Roadmap items, the command log and git history have routes (below) but no
  view in the current page.

## Data

The page asks, in order, `/api/graph`, `./graph-data.json`, `/graph-data.json`
and uses the first that answers with `{ok, data}` or a bare `{nodes, ...}`.

Routes `deciduous serve` answers:

| Route | Returns |
|---|---|
| `GET /api/graph` | `{ok, data: {nodes, edges, themes, node_themes, documents, config?}}`; `config` only when `[github] commit_repo` is set |
| `GET /api/documents?node_id=N` | documents attached to node N |
| `GET /api/documents/file/<id>` | the file's bytes |
| `POST /api/ask` | `{question, context}` → `claude -p`'s answer |
| `GET /api/qa`, `GET /api/qa/search?q=`, `GET`/`DELETE /api/qa/<id>` | stored questions and answers |
| `GET /api/git-history` | commits for every node that names one |
| `GET /api/roadmap`, `POST /api/roadmap/checkbox` | ROADMAP.md items; toggle one |
| `GET /api/commands` | the command log (empty in 1.1.0 projects) |
| `GET /api/sync-status` | whether `graph.json` exists and what `sync` would import or export |
| anything else | the embedded page |

### A static copy

`deciduous sync` stopped writing `docs/graph-data.json` in 1.0.5, and `init`
no longer sets up GitHub Pages. The page still reads `./graph-data.json` when
there is no API, so a static copy is the viewer HTML next to a graph export:

```bash
deciduous graph > docs/graph-data.json    # {nodes, edges, themes, node_themes[, documents]}
cp src/viewer.html docs/demo/index.html   # the embedded page; this repository keeps one there
```

The Ask panel needs the server and does not work from a static copy. The `docs/graph-data.json` in this repository is such a hand-made export
(1465 nodes, 1313 edges when it was last written), not something a command
maintains.

## Building it

```bash
cd web
npm install
npm run dev            # Vite dev server against a running `deciduous serve`
npm run typecheck      # tsc --noEmit
npm run test           # vitest
npm run build          # tsc && vite build -> web/dist/index.html, one self-contained file
```

Three Vite configs:

| Config | Output | For |
|---|---|---|
| `vite.config.ts` | `web/dist/index.html`, single file, `base: './'` | the embedded viewer |
| `vite.embed.config.ts` | `web/dist-embed/` | the same, kept separately |
| `vite.pages.config.ts` | `../docs/demo/`, `base: '/deciduous/demo/'` | the static demo for this repository's GitHub Pages |

After a change under `web/src`, the built file has to be copied into the two
places it is served from, then the binary rebuilt:

```bash
cp web/dist/index.html src/viewer.html
cp web/dist/index.html docs/demo/index.html
cargo build --release
```

`src/viewer.html` is `include_str!`ed by `src/serve.rs`. It is a build product;
do not edit it by hand.

## Types

`web/src/types/graph.ts` mirrors `src/db.rs`: `NODE_TYPES` (seven, including
`revisit`), `EDGE_TYPES` (seven, including `took_from`), `NodeMetadata`
(`confidence`, `commit`, `prompt`, `files`, `branch`, GitHub fields, and
anything else as `unknown`), `Theme`, `NodeThemeAssociation`, `NodeDocument`.
The generated `DecisionNode` and `DecisionEdge` come from `types/generated/`.

One mismatch to know about: `NODE_STATUSES` in `graph.ts` lists four statuses
(`pending`, `active`, `completed`, `rejected`) while the binary accepts six
(`superseded` and `abandoned` too). Nodes with those statuses load and display;
the type is narrower than the data.

# Web Viewer

The web viewer is a React/TypeScript application that provides multiple ways to explore the decision graph.

---

## Access Methods

### Local Development Server

```bash
deciduous serve --port 3000
```

Opens a browser to `http://localhost:3000`. Data refreshes every 30 seconds.

### GitHub Pages (Static)

```bash
deciduous sync
git add docs/
git commit -m "Update decision graph"
git push
```

The graph is then available at `https://<user>.github.io/<repo>/`

---

## Views

The viewer provides multiple browsing modes, each optimized for different questions:

### Archaeology View (Default)

**Question:** How did this system evolve?

Shows:
- **Narratives**: Story arcs extracted from the graph
- **Pivots**: Decision points where direction changed
- **Evolution timeline**: How understanding developed

This is the default because it shows the **why** behind the current state.

### DAG View

**Question:** What's connected to what?

Shows:
- Hierarchical layout (goals at top, outcomes at bottom)
- All edges visualized
- Click nodes to see details

Uses dagre for automatic layout.

### Chains View

**Question:** Show me everything related to X.

Shows:
- Connected components grouped together
- Chains rooted at goals
- Session grouping (4-hour gaps)

### Timeline View

**Question:** What happened when?

Shows:
- Chronological list of nodes and git commits
- Merged timeline of decisions and code changes
- Commit messages linked to decision nodes

### Story View

**Question:** What's the full context for this goal?

Shows:
- Full decision tree starting from a goal
- All descendants (actions, outcomes, observations)
- Complete narrative for one feature

### Graph View

**Question:** I want to explore freely.

Shows:
- Force-directed graph (D3)
- Interactive pan/zoom
- Click to select nodes

### Roadmap View

**Question:** What work is planned?

Shows:
- ROADMAP.md items parsed and displayed
- GitHub Issue sync status
- Completion checkboxes
- Links to related decision nodes

### Log View

**Question:** What commands were run?

Shows:
- deciduous CLI command history
- Timestamps and results
- Useful for debugging

---

## Key Algorithms

### Chain Building (`web/src/utils/graphProcessing.ts`)

```typescript
function buildChains(graphData: GraphData): Chain[]
```

1. Find root nodes:
   - All `goal` nodes
   - Any node with no incoming edges

2. Sort roots: goals first, then by creation time

3. BFS from each root:
   - Follow **both** outgoing and incoming edges
   - This captures the full connected component
   - No node limit (MAX_CHAIN_NODES = 0)

4. Sort chains: newest first

**Why bidirectional?** A node might be linked TO (incoming) but not FROM the root. We want the full component.

### Session Grouping (`web/src/utils/graphProcessing.ts`)

```typescript
function buildSessions(nodes: DecisionNode[], chains: Chain[]): Session[]
```

1. Sort all nodes by creation time
2. Walk through nodes:
   - If gap > 4 hours, start new session
   - Otherwise, extend current session
3. Associate chains with sessions (by first node time)
4. Reverse to show newest first

**Why 4 hours?** Represents a typical "work session" - lunch break or end of day creates a natural gap.

### Pivot Detection (`web/src/utils/archaeologyProcessing.ts`)

```typescript
function findPivots(graphData: GraphData): Pivot[]
```

1. Find all `revisit` nodes
2. For each revisit:
   - Get incoming edges → observations, old decisions
   - Get outgoing edges → new decisions
3. Build pivot context:
   - `before`: What was the old approach?
   - `after`: What's the new approach?
   - `reasons`: What observations caused the pivot?

**Why revisit nodes?** They explicitly mark direction changes, making pivots queryable.

### Path Tracing (`web/src/utils/graphProcessing.ts`)

```typescript
function tracePath(nodeId: number, graphData: GraphData): DecisionNode[]
```

1. Start at given node
2. Follow incoming edges backwards
3. Build path from root to node
4. Return ordered array

**Use case:** "How did we get to this outcome?"

---

## Data Sources

### Local Server (`deciduous serve`)

| Endpoint | Returns |
|----------|---------|
| `GET /api/graph` | `{ nodes: [...], edges: [...] }` |
| `GET /api/git-history` | Git commits with hashes and messages |
| `GET /api/roadmap` | Parsed ROADMAP.md items |
| `GET /api/commands` | CLI command history |
| `GET /api/documents?node_id=N` | Documents attached to node N |
| `GET /api/documents/file/<id>` | Serve document file content |

### Static Files (GitHub Pages)

| File | Content |
|------|---------|
| `./graph-data.json` | Decision graph |
| `./git-history.json` | Git commits |
| `./roadmap-items.json` | Roadmap items |

The viewer auto-detects which mode based on hostname/port.

---

## TypeScript Types

### Core Types (`web/src/types/graph.ts`)

```typescript
// Node in the decision graph
interface DecisionNode {
  id: number;
  change_id: string;
  node_type: NodeType;  // 'goal' | 'decision' | 'action' | ...
  title: string;
  description: string | null;
  status: NodeStatus;   // 'active' | 'superseded' | ...
  created_at: string;
  updated_at: string;
  metadata_json: string | null;
}

// Edge connecting nodes
interface DecisionEdge {
  id: number;
  from_node_id: number;
  to_node_id: number;
  edge_type: EdgeType;  // 'leads_to' | 'chosen' | ...
  rationale: string | null;
  created_at: string;
}

// Connected component
interface Chain {
  root: DecisionNode;
  nodes: DecisionNode[];
  edges: DecisionEdge[];
}

// Time-grouped nodes
interface Session {
  startTime: number;
  endTime: number;
  nodes: DecisionNode[];
  chains: Chain[];
}
```

### Document Attachment Type

```typescript
// Document attached to a decision node
interface NodeDocument {
  id: number;
  change_id: string;
  node_id: number;
  node_change_id: string;
  content_hash: string;
  original_filename: string;
  storage_filename: string;
  mime_type: string;
  file_size: number;
  description: string | null;
  description_source: 'none' | 'manual' | 'ai';
  attached_at: string;
  attached_by: string | null;
  detached_at: string | null;
}
```

### Metadata Access

Node metadata is stored as JSON. Helper functions extract it:

```typescript
// Get commit hash linked to node
getCommit(node: DecisionNode): string | null

// Get branch name
getBranch(node: DecisionNode): string | null

// Get confidence level (0-100)
getConfidence(node: DecisionNode): number | null

// Get verbatim prompt
getPrompt(node: DecisionNode): string | null

// Get associated files
getFiles(node: DecisionNode): string[]

// Get active (non-detached) documents for a node
getNodeDocuments(nodeId: number, graphData: GraphData): NodeDocument[]

// Format file size to human-readable string
formatFileSize(bytes: number): string
```

---

## Development

### Build Process

```bash
cd web
npm install
npm run dev      # Development server
npm run build    # Production build
```

### Embedding in Rust Binary

After building:

```bash
cp web/dist/index.html src/viewer.html
cp web/dist/index.html docs/demo/index.html
cargo build --release
```

The viewer is embedded as a string in `serve.rs` and served at `/`.

### Build Configurations

| Config | Output | Purpose |
|--------|--------|---------|
| `vite.config.ts` | Standard SPA | Development |
| `vite.embed.config.ts` | Single HTML file | Embed in Rust binary |
| `vite.pages.config.ts` | Static assets | GitHub Pages |

---

## Styling

- Node colors by type (green for goals, blue for decisions, etc.)
- Edge colors by type
- Responsive layout
- Dark mode support via CSS variables

---

## URL Structure

Deep linking is supported:

| URL | View |
|-----|------|
| `/` | Archaeology (default) |
| `/dag` | DAG view |
| `/dag/:nodeId` | DAG with node selected |
| `/chains` | Chains view |
| `/chains/:chainId` | Specific chain |
| `/timeline` | Timeline view |
| `/graph` | Force-directed graph |
| `/roadmap` | Roadmap view |
| `/story` | Story view |

---

## Performance Notes

- **Polling interval**: 30 seconds when running locally
- **No SSE yet**: Disabled until deciduous serve supports it
- **Chain limit removed**: MAX_CHAIN_NODES = 0 shows full components
- **Session gap**: 4 hours (configurable via SESSION_GAP_MS constant)

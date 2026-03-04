# Project Instructions

## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### Available Slash Commands

| Command | Purpose |
|---------|---------|
| `/decision` | Manage decision graph - add nodes, link edges, sync |
| `/recover` | Recover context from decision graph on session start |
| `/work` | Start a work transaction - creates goal node before implementation |
| `/document` | Generate comprehensive documentation for a file or directory |
| `/build-test` | Build the project and run the test suite |
| `/serve-ui` | Start the decision graph web viewer |
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Multi-user sync - pull events, rebuild, push |

### Available Skills

| Skill | Purpose |
|-------|---------|
| `/pulse` | Map current design as decisions (Now mode) |
| `/narratives` | Understand how the system evolved (History mode) |
| `/archaeology` | Transform narratives into queryable graph |

### The Node Flow Rule - CRITICAL

The canonical flow through the decision graph is:

```
goal -> options -> decision -> actions -> outcomes
```

- **Goals** lead to **options** (possible approaches to explore)
- **Options** lead to a **decision** (choosing which option to pursue)
- **Decisions** lead to **actions** (implementing the chosen approach)
- **Actions** lead to **outcomes** (results of the implementation)
- **Observations** attach anywhere relevant
- Goals do NOT lead directly to decisions -- there must be options first
- Options do NOT come after decisions -- options come BEFORE decisions
- Decision nodes should only be created when an option is actually chosen, not prematurely

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
AUDIT regularly -> Check for missing connections
```

### Behavioral Triggers - MUST LOG WHEN:

| Trigger | Log Type | Example |
|---------|----------|---------|
| User asks for a new feature | `goal` **with -p** | "Add dark mode" |
| Exploring possible approaches | `option` | "Use Redux for state" |
| Choosing between approaches | `decision` | "Choose state management" |
| About to write/edit code | `action` | "Implementing Redux store" |
| Something worked or failed | `outcome` | "Redux integration successful" |
| Notice something interesting | `observation` | "Existing code uses hooks" |

### Document Attachments

Attach files (images, PDFs, diagrams, specs, screenshots) to decision graph nodes for rich context.

```bash
# Attach a file to a node
deciduous doc attach <node_id> <file_path>
deciduous doc attach <node_id> <file_path> -d "Architecture diagram"
deciduous doc attach <node_id> <file_path> --ai-describe

# List documents
deciduous doc list              # All documents
deciduous doc list <node_id>    # Documents for a specific node

# Manage documents
deciduous doc show <doc_id>     # Show document details
deciduous doc describe <doc_id> "Updated description"
deciduous doc describe <doc_id> --ai   # AI-generate description
deciduous doc open <doc_id>     # Open in default application
deciduous doc detach <doc_id>   # Soft-delete (recoverable)
deciduous doc gc                # Remove orphaned files from disk
```

**When to suggest document attachment:**

| Situation | Action |
|-----------|--------|
| User shares an image or screenshot | Ask: "Want me to attach this to the current goal/action node?" |
| User references an external document | Ask: "Should I attach a copy to the decision graph?" |
| Architecture diagram is discussed | Suggest attaching it to the relevant goal node |
| Files not in the project are dropped in | Attach to the most relevant active node |

**Do NOT aggressively prompt for documents.** Only suggest when files are directly relevant to a decision node. Files are stored in `.deciduous/documents/` with content-hash naming for deduplication.

### CRITICAL: Capture VERBATIM User Prompts

**Prompts must be the EXACT user message, not a summary.** When a user request triggers new work, capture their full message word-for-word.

**BAD - summaries are useless for context recovery:**
```bash
# DON'T DO THIS - this is a summary, not a prompt
deciduous add goal "Add auth" -p "User asked: add login to the app"
```

**GOOD - verbatim prompts enable full context recovery:**
```bash
# Use --prompt-stdin for multi-line prompts
deciduous add goal "Add auth" -c 90 --prompt-stdin << 'EOF'
I need to add user authentication to the app. Users should be able to sign up
with email/password, and we need OAuth support for Google and GitHub. The auth
should use JWT tokens with refresh token rotation.
EOF

# Or use the prompt command to update existing nodes
deciduous prompt 42 << 'EOF'
The full verbatim user message goes here...
EOF
```

**When to capture prompts:**
- Root `goal` nodes: YES - the FULL original request
- Major direction changes: YES - when user redirects the work
- Routine downstream nodes: NO - they inherit context via edges

**Updating prompts on existing nodes:**
```bash
deciduous prompt <node_id> "full verbatim prompt here"
cat prompt.txt | deciduous prompt <node_id>  # Multi-line from stdin
```

Prompts are viewable in the web viewer.

### CRITICAL: Maintain Connections

**The graph's value is in its CONNECTIONS, not just nodes.**

| When you create... | IMMEDIATELY link to... |
|-------------------|------------------------|
| `outcome` | The action that produced it |
| `action` | The decision that spawned it |
| `decision` | The option(s) it chose between |
| `option` | Its parent goal |
| `observation` | Related goal/action |
| `revisit` | The decision/outcome being reconsidered |

**Root `goal` nodes are the ONLY valid orphans.**

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting

# Metadata flags
# -c, --confidence 0-100   Confidence level
# -p, --prompt "..."       Store the user prompt (use when semantically meaningful)
# -f, --files "a.rs,b.rs"  Associate files
# -b, --branch <name>      Git branch (auto-detected)
# --commit <hash|HEAD>     Link to git commit (use HEAD for current commit)
# --date "YYYY-MM-DD"      Backdate node (for archaeology)

# Branch filtering
deciduous nodes --branch main
deciduous nodes -b feature-auth
```

### CRITICAL: Link Commits to Actions/Outcomes

**After every git commit, link it to the decision graph!**

```bash
git commit -m "feat: add auth"
deciduous add action "Implemented auth" -c 90 --commit HEAD
deciduous link <goal_id> <action_id> -r "Implementation"
```

The `--commit HEAD` flag captures the commit hash and links it to the node. The web viewer will show commit messages, authors, and dates.

### Git History & Deployment

```bash
# Export graph AND git history for web viewer
deciduous sync

# This creates:
# - docs/graph-data.json (decision graph)
# - docs/git-history.json (commit info for linked nodes)
```

To deploy to GitHub Pages:
1. `deciduous sync` to export
2. Push to GitHub
3. Settings > Pages > Deploy from branch > /docs folder

Your graph will be live at `https://<user>.github.io/<repo>/`

### Branch-Based Grouping

Nodes are auto-tagged with the current git branch. Configure in `.deciduous/config.toml`:
```toml
[branch]
main_branches = ["main", "master"]
auto_detect = true
```

### Audit Checklist (Before Every Sync)

1. Does every **outcome** link back to what caused it?
2. Does every **action** link to why you did it?
3. Any **dangling outcomes** without parents?

### Git Staging Rules - CRITICAL

**NEVER use broad git add commands that stage everything:**
- ❌ `git add -A` - stages ALL changes including untracked files
- ❌ `git add .` - stages everything in current directory
- ❌ `git add -a` or `git commit -am` - auto-stages all tracked changes
- ❌ `git add *` - glob patterns can catch unintended files

**ALWAYS stage files explicitly by name:**
- ✅ `git add src/main.rs src/lib.rs`
- ✅ `git add Cargo.toml Cargo.lock`
- ✅ `git add .claude/commands/decision.md`

**Why this matters:**
- Prevents accidentally committing sensitive files (.env, credentials)
- Prevents committing large binaries or build artifacts
- Forces you to review exactly what you're committing
- Catches unintended changes before they enter git history

### Session Start Checklist

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
deciduous doc list        # Any attached documents to review?
git status                # Current state
```

### Multi-User Sync

Sync decisions with teammates via event logs:

```bash
# Check sync status
deciduous events status

# Apply teammate events (after git pull)
deciduous events rebuild

# Compact old events periodically
deciduous events checkpoint --clear-events
```

Events auto-emit on add/link/status commands. Git merges event files automatically.

# Elixir Development Rules

A comprehensive set of rules for AI agents working on Elixir/Phoenix/OTP codebases.

---

## Table of Contents

- [Project Guidelines](#project-guidelines)
- [Architecture](#architecture)
- [Elixir Core Rules](#elixir-core-rules)
- [Code Style Rules](#code-style-rules)
- [OTP Rules](#otp-rules)
- [Phoenix Framework](#phoenix-framework)
- [Phoenix HTML & HEEx](#phoenix-html--heex)
- [Phoenix LiveView](#phoenix-liveview)
- [Ecto & Database](#ecto--database)
- [Testing](#testing)
- [Database Querying (PostgreSQL)](#database-querying-postgresql)

---

## Project Guidelines

- Use `mix precommit` alias when you are done with all changes and fix any pending issues
- Use the already included `:req` (`Req`) library for HTTP requests. **Avoid** `:httpoison`, `:tesla`, and `:httpc`. Req is the preferred HTTP client for Phoenix apps
- For boolean conditions:
  - If there is a single condition, use `if`/`else` (don't use a `case` statement that matches on `true`/`false`)
  - If there are multiple conditions, use `cond`

---

## Architecture

### Three-Layer Architecture

1. **UI Layer**: User interaction and guidance
2. **Business Logic (BL) Layer**: Policy enforcement and orchestration
3. **Data Layer (DL)**: External system interfaces

### Business Logic Layer Requirements

- All entry points must require and validate user permissions
- Must use Bodyguard for permission checks
- Must log actions for audit purposes
- Should validate incoming data form/structure
- Internal functions can be pure and simple once permissions and data are validated

### Feature-Focused Layout

- Features should be complete functional units
- Shared functionality goes in "components" (UI) or "common" (BL)
- Each feature maintains its own directory structure

### Permission Handling

- **UI Layer**: Guides user decisions based on permissions
- **BL Layer**: Hard enforcement of permissions
- **DL Layer**: No application permission handling

#### MyAppWeb Organization

All new UI code goes into `your_app_web/`:

```
your_app_web/
├── components/              # Shared domain components
├── common/                  # Cross-cutting concerns, utilities, helpers
└── live/
    ├── admin/
```

**Key Rules:**

- `your_app_web/components/`: Domain components reused across teams/features
- `your_app_web/common/`: Cross-cutting concerns and shared helpers
- `your_app_web/live/<team>/<feature>/components/`: Components specific to a single feature

#### API Services Organization

Organize business logic by domain groups and features:

```
api_services/
└── lib/
    └── api_services/
        └── <group>/                  # Business domain (sales, marketing, etc.)
            ├── common/               # Shared concerns within group
            └── <project_or_area>/    # Specific project or functional area
                ├── <feature>.ex      # Entry point module
                └── <feature>/        # Supporting code
```

#### Entry Point Responsibilities

Entry point modules (`<feature>.ex`) must handle:

1. **Authorization**: Verify user permissions
2. **Logging**: Record user access attempts
3. **Validation**: Clean up or reject questionable data at the boundary

### Module Naming Convention

**CRITICAL**: All module names MUST match their file location exactly.

**Example:**

- **File:** `apps/api/lib/your_app_web/live/marketing/forecasting/forecasting_live.ex`
- **Module:** `MyAppWeb.Live.Marketing.Forecasting.ForecastingLive`

### File Naming and Placement

Avoid redundant path segments in filenames:

```
# WRONG - redundant
apps/api_services/lib/api_services/azure/azure_cli.ex

# CORRECT - clean
apps/api_services/lib/api_services/azure/cli.ex
```

### Test Organization

Test files MUST mirror source code structure exactly.

- **Source:** `apps/api/lib/your_app_web/live/marketing/forecasting/forecasting_live.ex`
- **Test:** `apps/api/test/your_app_web/live/marketing/forecasting/forecasting_live_test.exs`

---

## Elixir Core Rules

### Pattern Matching

- Use pattern matching over conditional logic when possible
- Prefer to match on function heads instead of using `if`/`else` or `case` in function bodies
- `%{}` matches ANY map, not just empty maps. Use `map_size(map) == 0` guard to check for truly empty maps

### Lists

- Elixir lists **do not support index-based access via the access syntax**

  ```elixir
  # INVALID
  mylist[0]

  # VALID
  Enum.at(mylist, 0)
  ```

- Prefer to prepend to lists `[new | list]` not `list ++ [new]`

### Immutability and Rebinding

- Variables are immutable but can be rebound. For block expressions like `if`, `case`, `cond`, etc., you *must* bind the result to a variable:

  ```elixir
  # INVALID - rebinding inside the if has no effect outside
  if connected?(socket) do
    socket = assign(socket, :val, val)
  end

  # VALID - rebind the result of the if
  socket =
    if connected?(socket) do
      assign(socket, :val, val)
    end
  ```

### Error Handling

- Use `{:ok, result}` and `{:error, reason}` tuples for operations that can fail
- Avoid raising exceptions for control flow
- Use `with` for chaining operations that return `{:ok, _}` or `{:error, _}`
- Elixir has no `return` statement, nor early returns. The last expression in a block is always returned

### Function Design

- Use guard clauses: `when is_binary(name) and byte_size(name) > 0`
- Prefer multiple function clauses over complex conditional logic
- Name functions descriptively: `calculate_total_price/2` not `calc/2`
- Predicate function names should NOT start with `is_` and should end in a question mark (`valid?/1`). Names like `is_thing` should be reserved for guards
- Prefer `Enum` functions like `Enum.reduce` over recursion
- When recursion is necessary, prefer to use pattern matching in function heads for base case detection

### Data Structures

- Use structs over maps when the shape is known: `defstruct [:name, :age]`
- Prefer keyword lists for options: `[timeout: 5000, retries: 3]`
- Use maps for dynamic key-value data
- **Never** use map access syntax (`changeset[:field]`) on structs as they do not implement the Access behaviour by default. Use `my_struct.field` instead

### Common Mistakes to Avoid

- Don't use `Enum` functions on large collections when `Stream` is more appropriate
- Avoid nested `case` statements - refactor to a single `case`, `with`, or separate functions
- Don't use `String.to_atom/1` on user input (memory leak risk)
- Using the process dictionary is typically a sign of unidiomatic code
- Only use macros if explicitly requested
- **Never** nest multiple modules in the same file (causes cyclic dependencies and compilation errors)

### Standard Library

- Elixir's standard library has everything necessary for date and time manipulation. Use `Time`, `Date`, `DateTime`, and `Calendar`. **Never** install additional date/time dependencies unless asked (except `date_time_parser` for parsing)
- There are many useful standard library functions - prefer to use them where possible

### Concurrency

- Use `Task.async_stream(collection, callback, options)` for concurrent enumeration with back-pressure. Most of the time you will want to pass `timeout: :infinity` as option

### Mix Tasks

- Use `mix help` to list available mix tasks
- Use `mix help task_name` to get docs for an individual task
- Read the docs and options fully before using tasks
- To debug test failures, run tests in a specific file with `mix test test/my_test.exs` or run all previously failed tests with `mix test --failed`
- `mix deps.clean --all` is **almost never needed**. **Avoid** using it unless you have good reason

---

## Code Style Rules

### Aliases

1. No compound aliases - each module gets its own alias line:

   ```elixir
   # WRONG
   alias Optimizer.Ecto.{Customer, Repo}

   # CORRECT
   alias Optimizer.Ecto.Customer
   alias Optimizer.Ecto.Repo
   ```

2. Aliases must be sorted alphabetically

### Avoid Redundant `assign_new` with `attr` Defaults

**Never** use `assign_new/3` in a component function when the `attr` already has a `default` value. The `attr` macro's `default` option already ensures the attribute exists with the specified default value.

`assign_new/3` is only necessary when:

- You need to lazily compute a default value that wasn't provided via attr
- The attribute doesn't have a default specified in the attr declaration
- You need to derive a value from other assigns

### Avoid Unnecessary Defensive Formatting

**Never** add defensive formatting functions when the data type is already known and guaranteed.

```elixir
# WRONG - Overly defensive
defp format_warning(warning) when is_binary(warning), do: warning
defp format_warning(warning), do: inspect(warning)

# CORRECT - Trust known types, just use the value directly
{warning}
```

Only use defensive formatting when:

- Data comes from external/untrusted sources
- The type is genuinely uncertain
- Multiple types are intentionally supported

### Pipe Chain Style

**Always** start pipe chains with raw values rather than function calls:

```elixir
# WRONG
Enum.take(list, 5) |> Enum.shuffle() |> pick_winner()

# CORRECT
list |> Enum.take(5) |> Enum.shuffle() |> pick_winner()
```

### Avoid Duplicate Logic Blocks

Never repeat validation, transformation, or business logic that already exists elsewhere. Signs of duplicate logic:

- Same validation appearing in multiple functions
- Identical data transformation logic
- Repeated error checking
- Parallel conditional structures checking the same thing

**Decision Framework:**

1. Is this validation/transformation needed in both places? If only one needs it, remove the duplicate. If both need it, extract to a shared function.
2. What's the responsibility of each function? Don't mix responsibilities.
3. Where should the check happen? Validate at entry points, transform in dedicated functions.

### No Backward Compatibility Functions

When changing a function signature, **always** update all call sites rather than maintaining backward compatibility wrapper functions.

---

## OTP Rules

### GenServer Best Practices

- Keep state simple and serializable
- Handle all expected messages explicitly
- Use `handle_continue/2` for post-init work
- Implement proper cleanup in `terminate/2` when necessary

### Process Communication

- Use `GenServer.call/3` for synchronous requests expecting replies
- Use `GenServer.cast/2` for fire-and-forget messages
- When in doubt, use `call` over `cast`, to ensure back-pressure
- Set appropriate timeouts for `call/3` operations

### Fault Tolerance

- Set up processes such that they can handle crashing and being restarted by supervisors
- Use `:max_restarts` and `:max_seconds` to prevent restart loops

### Task and Async

- Use `Task.Supervisor` for better fault tolerance
- Handle task failures with `Task.yield/2` or `Task.shutdown/2`
- Set appropriate task timeouts
- Use `Task.async_stream/3` for concurrent enumeration with back-pressure

### OTP Primitives

- Elixir's builtin OTP primitives like `DynamicSupervisor` and `Registry` require names in the child spec:

  ```elixir
  {DynamicSupervisor, name: MyApp.MyDynamicSup}
  DynamicSupervisor.start_child(MyApp.MyDynamicSup, child_spec)
  ```

---

## Phoenix Framework

### Router

- `scope` blocks include an optional alias which is prefixed for all routes within the scope. **Always** be mindful of this to avoid duplicate module prefixes
- You **never** need to create your own `alias` for route definitions. The `scope` provides the alias:

  ```elixir
  scope "/admin", AppWeb.Admin do
    pipe_through :browser
    live "/users", UserLive, :index
  end
  # UserLive route points to AppWeb.Admin.UserLive
  ```

### Deprecated Modules

- `Phoenix.View` no longer is needed or included with Phoenix, don't use it

---

## Phoenix HTML & HEEx

- Phoenix templates **always** use `~H` or `.html.heex` files (HEEx), **never** use `~E`
- **Always** use `Phoenix.Component.form/1` and `Phoenix.Component.inputs_for/1` to build forms. **Never** use `Phoenix.HTML.form_for` or `Phoenix.HTML.inputs_for` (outdated)
- **Always** use `Phoenix.Component.to_form/2`: `assign(socket, form: to_form(...))` and `<.form for={@form} id="msg-form">`
- **Always** add unique DOM IDs to key elements (forms, buttons, etc.) for testability
- For "app wide" template imports, use the `html_helpers` block in `my_app_web.ex`

### Conditionals in Templates

- Elixir supports `if/else` but **does NOT support `if/else if` or `if/elsif`**. **Always** use `cond` or `case` for multiple conditionals:

  ```elixir
  # NEVER do this
  <%= if condition do %>
    ...
  <% else if other_condition %>
    ...
  <% end %>

  # ALWAYS do this
  <%= cond do %>
    <% condition -> %>
      ...
    <% condition2 -> %>
      ...
    <% true -> %>
      ...
  <% end %>
  ```

### Curly Braces in HEEx

HEEx requires special annotation for literal curly braces. Use `phx-no-curly-interpolation` on the parent tag:

```heex
<code phx-no-curly-interpolation>
  let obj = {key: "val"}
</code>
```

### Class Lists

Use list `[...]` syntax for class attributes. **Always** use this for multiple class values:

```heex
<a class={[
  "px-2 text-white",
  @some_flag && "py-5",
  if(@other_condition, do: "border-red-500", else: "border-blue-100"),
]}>Text</a>
```

**Never** omit the `[` and `]` - it's invalid without them.

### Iteration

**Never** use `<% Enum.each %>` for generating template content. **Always** use:

```heex
<%= for item <- @collection do %>
  ...
<% end %>
```

### Comments

HEEx HTML comments use `<%!-- comment --%>`. **Always** use this syntax.

### Interpolation

- Use `{...}` for interpolation within tag attributes and for simple values within tag bodies
- Use `<%= ... %>` for block constructs (if, cond, case, for) within tag bodies

```heex
<%!-- CORRECT --%>
<div id={@id}>
  {@my_assign}
  <%= if @condition do %>
    {@another_assign}
  <% end %>
</div>

<%!-- INVALID - will cause syntax errors --%>
<div id="<%= @invalid_interpolation %>">
  {if @invalid_block_construct do}
  {end}
</div>
```

---

## Phoenix LiveView

### Navigation

- **Never** use deprecated `live_redirect` and `live_patch`. **Always** use:
  - Templates: `<.link navigate={href}>` and `<.link patch={href}>`
  - LiveViews: `push_navigate` and `push_patch`

### Component Preference

- **Avoid LiveComponents** unless you have a strong, specific need for them
- Default to functional components

### Naming

- LiveViews should be named with a `Live` suffix: `AppWeb.WeatherLive`
- The default `:browser` scope is already aliased with the `AppWeb` module: `live "/weather", WeatherLive`

### Streams

- **Always** use LiveView streams for collections instead of assigning regular lists (avoids memory ballooning):

  ```elixir
  stream(socket, :messages, [new_msg])                    # append
  stream(socket, :messages, [new_msg], reset: true)       # reset
  stream(socket, :messages, [new_msg], at: -1)            # prepend
  stream_delete(socket, :messages, msg)                    # delete
  ```

- Templates must set `phx-update="stream"` on the parent element with a DOM id:

  ```heex
  <div id="messages" phx-update="stream">
    <div :for={{id, msg} <- @streams.messages} id={id}>
      {msg.text}
    </div>
  </div>
  ```

- Streams are NOT enumerable. To filter/refresh, refetch data and re-stream with `reset: true`
- Streams do not support counting. Track counts via a separate assign
- When updating an assign that changes streamed items, you MUST re-stream those items
- **Never** use deprecated `phx-update="append"` or `phx-update="prepend"`

### Empty States with Streams

```heex
<div id="tasks" phx-update="stream">
  <div class="hidden only:block">No tasks yet</div>
  <div :for={{id, task} <- @streams.tasks} id={id}>
    {task.name}
  </div>
</div>
```

### JavaScript Interop

- When using `phx-hook="MyHook"` with JS-managed DOM, you **must** also set `phx-update="ignore"`
- **Always** provide a unique DOM id alongside `phx-hook`

#### Colocated JS Hooks (Inline)

**Never** write raw `<script>` tags in HEEx. **Always** use colocated js hook script tags:

```heex
<input type="text" id="user-phone" phx-hook=".PhoneNumber" />
<script :type={Phoenix.LiveView.ColocatedHook} name=".PhoneNumber">
  export default {
    mounted() {
      this.el.addEventListener("input", e => {
        let match = this.el.value.replace(/\D/g, "").match(/^(\d{3})(\d{3})(\d{4})$/)
        if(match) {
          this.el.value = `${match[1]}-${match[2]}-${match[3]}`
        }
      })
    }
  }
</script>
```

- Colocated hook names **MUST** start with a `.` prefix (e.g., `.PhoneNumber`)

#### External Hooks

Place in `assets/js/` and pass to the LiveSocket constructor:

```javascript
const MyHook = {
  mounted() { ... }
}
let liveSocket = new LiveSocket("/live", Socket, {
  hooks: { MyHook }
});
```

#### push_event

**Always** return or rebind the socket on `push_event/3`:

```elixir
socket = push_event(socket, "my_event", %{...})
# or
{:noreply, push_event(socket, "my_event", %{...})}
```

Client-side handling:

```javascript
// Receive from server
this.handleEvent("my_event", data => console.log(data));

// Push to server with reply
this.pushEvent("my_event", {one: 1}, reply => console.log(reply));
```

### Forms

#### Creating Forms

From params:

```elixir
def handle_event("submitted", params, socket) do
  {:noreply, assign(socket, form: to_form(params))}
end
```

From changesets:

```elixir
%MyApp.Users.User{}
|> Ecto.Changeset.change()
|> to_form()
```

#### Form Template Rules

**Always** use a form assigned via `to_form/2` and the `<.input>` component:

```heex
<%!-- CORRECT --%>
<.form for={@form} id="my-form">
  <.input field={@form[:field]} type="text" />
</.form>

<%!-- NEVER do this --%>
<.form for={@changeset} id="my-form">
  <.input field={@changeset[:field]} type="text" />
</.form>
```

- You are **FORBIDDEN** from accessing the changeset in the template
- **Never** use `<.form let={f} ...>`, **always** use `<.form for={@form} ...>`
- Always give forms an explicit, unique DOM ID

### LiveView Tests

- Use `Phoenix.LiveViewTest` module and `LazyHTML` for assertions
- Form tests use `render_submit/2` and `render_change/2`
- **Always** reference key element IDs from your templates in tests
- **Never** test against raw HTML. **Always** use `element/2`, `has_element/2`: `assert has_element?(view, "#my-form")`
- Focus on testing outcomes rather than implementation details
- When facing test failures with element selectors, debug with LazyHTML:

  ```elixir
  html = render(view)
  document = LazyHTML.from_fragment(html)
  matches = LazyHTML.filter(document, "your-selector")
  IO.inspect(matches, label: "Matches")
  ```

---

## Ecto & Database

### General

- **Always** preload Ecto associations in queries when they'll be accessed in templates
- Remember `import Ecto.Query` when writing `seeds.exs`
- `Ecto.Schema` fields always use the `:string` type, even for `:text` columns: `field :name, :string`
- `Ecto.Changeset.validate_number/2` **does NOT support** the `:allow_nil` option
- You **must** use `Ecto.Changeset.get_field(changeset, :field)` to access changeset fields
- Fields set programmatically (like `user_id`) must NOT be listed in `cast` calls for security. Set them explicitly when creating the struct
- **Always** use `mix ecto.gen.migration migration_name_using_underscores` to generate migration files

### Embedded Schemas for Structured JSON Data

When a data structure has a known, stable shape and will be serialized to a JSON/JSONB column, **always** define an `embedded_schema`:

```elixir
defmodule MyApp.WizardState do
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key false
  embedded_schema do
    field :optimization_goal, Ecto.Enum, values: [:budget_first, :goal_first], default: :budget_first
    field :tolerance, Ecto.Enum, values: [:conservative, :moderate, :flexible], default: :flexible
    field :selected_start_year, :integer
  end

  def changeset(metadata \\ %__MODULE__{}, attrs) do
    cast(metadata, attrs, [...])
  end

  def hydrate_from_draft(metadata) when is_map(metadata) do
    changeset(metadata) |> apply_changes()
  end
end
```

**Where to put the embedded schema:**

- **Domain data** (shared across layers): `optimizer_ecto` with `embeds_one` on the parent schema
- **UI-only state** (wizard state, form drafts): The UI-layer module that uses it

**When a plain `:map` is fine:**

- Truly dynamic/schemaless data
- Pass-through data your application never inspects
- External API responses stored verbatim

### Schema Guidelines

**What belongs in a schema:**

- Embedded schema definition
- Basic changeset operation
- Schema-specific business rules or calculations

**What does NOT belong in a schema:**

- Everything else belongs in business logic under the domain it serves

---

## Testing

### General

- Never mock Ecto repositories - use the actual testing database
- Use the real Repo module in tests instead of creating mocks
- **Always use `start_supervised!/1`** to start processes in tests (guarantees cleanup)
- **Avoid** `Process.sleep/1` and `Process.alive?/1` in tests:

  ```elixir
  # Instead of sleeping, monitor and wait
  ref = Process.monitor(pid)
  assert_receive {:DOWN, ^ref, :process, ^pid, :normal}

  # Instead of sleeping to synchronize, use :sys.get_state
  _ = :sys.get_state(pid)
  ```

- Run tests: `mix test test/my_test.exs` or specific line: `mix test path/to/test.exs:123`
- Limit failures: `mix test --max-failures n`
- Tag tests: `@tag` and `mix test --only tag`
- Test exceptions: `assert_raise ArgumentError, fn -> invalid_function() end`

### Mocking with Mox

**CRITICAL: Never use `:meck` for mocking. Use the established Mox-based patterns.**

- Mock behavior callbacks using `Mox.expect/4`, not entire modules
- Don't create new mock modules unless explicitly requested. Use existing mocks (e.g., `MockWarehouse`)
- Pattern match on queries: `"SELECT MAX" <> _`
- Use pin operator `^variable` to match expected bound parameters
- Return appropriate result types (e.g., lists of maps with string keys for Snowflake)

```elixir
defmodule MyModuleTest do
  use Optimizer.ApiServices.ServicesCase, async: false
  import Mox

  test "handles snowflake response correctly" do
    expect(MockWarehouse, :execute, 1, fn "SELECT MAX(creation_date)" <> _, [] ->
      [%{"max_creation_date" => "2025-10-01"}]
    end)

    result = MyModule.function_that_queries_snowflake()
    assert result == expected_value
  end
end
```

### Date Boundary Testing

When testing date-dependent functionality, always test multiple boundaries:

- **Beginning of year**: First day/week of fiscal year
- **End of year**: Last day/week (important for 53-week years)
- **Mid-year**: A date in the middle

```elixir
start_of_year = Calendar.new(year: 2025, week: 1)
total_weeks = Calendar.total_weeks_in_year(2025)
end_of_year = Calendar.new(year: 2025, week: total_weeks)
mid_year = Calendar.new(year: 2025, week: 20)
```

---

## Database Querying (PostgreSQL)

You can access the database to run queries for validation. The test database runs in Docker:

```bash
psql "postgresql://pepsico:development@localhost:9650/rtb_proxy_repo" -c "SELECT ..."
```

Use this to validate data state (user_id, join records, etc.) that plays into program logic.


<!-- deciduous:start -->
## Decision Graph Workflow

**THIS IS MANDATORY. Log decisions IN REAL-TIME, not retroactively.**

### Available Slash Commands

| Command | Purpose |
|---------|---------|
| `/decision` | Manage decision graph - add nodes, link edges, sync |
| `/recover` | Recover context from decision graph on session start |
| `/work` | Start a work transaction - creates goal node before implementation |
| `/document` | Generate comprehensive documentation for a file or directory |
| `/build-test` | Build the project and run the test suite |
| `/serve-ui` | Start the decision graph web viewer |
| `/sync-graph` | Export decision graph to GitHub Pages |
| `/decision-graph` | Build a decision graph from commit history |
| `/sync` | Multi-user sync - pull events, rebuild, push |

### Available Skills

| Skill | Purpose |
|-------|---------|
| `/pulse` | Map current design as decisions (Now mode) |
| `/narratives` | Understand how the system evolved (History mode) |
| `/archaeology` | Transform narratives into queryable graph |

### The Node Flow Rule - CRITICAL

The canonical flow through the decision graph is:

```
goal -> options -> decision -> actions -> outcomes
```

- **Goals** lead to **options** (possible approaches to explore)
- **Options** lead to a **decision** (choosing which option to pursue)
- **Decisions** lead to **actions** (implementing the chosen approach)
- **Actions** lead to **outcomes** (results of the implementation)
- **Observations** attach anywhere relevant

### The Core Rule

```
BEFORE you do something -> Log what you're ABOUT to do
AFTER it succeeds/fails -> Log the outcome
CONNECT immediately -> Link every node to its parent
AUDIT regularly -> Check for missing connections
```

### Quick Commands

```bash
deciduous add goal "Title" -c 90 -p "User's original request"
deciduous add action "Title" -c 85
deciduous link FROM TO -r "reason"  # DO THIS IMMEDIATELY!
deciduous serve   # View live (auto-refreshes every 30s)
deciduous sync    # Export for static hosting
```

### Session Start Checklist

```bash
deciduous check-update    # Update needed? Run 'deciduous update' if yes
deciduous nodes           # What decisions exist?
deciduous edges           # How are they connected? Any gaps?
deciduous doc list        # Any attached documents to review?
git status                # Current state
```
<!-- deciduous:end -->

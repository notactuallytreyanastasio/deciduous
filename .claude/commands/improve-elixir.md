# Improve Elixir

Systematically improve Elixir code quality across the project. These are often quick builds and one-offs — this skill enforces discipline where speed would otherwise erode it.

## Mission

Write pure, testable functions. Eliminate warnings. Follow Credo strictly. Structure code as a functional core with an imperative shell. Find dead code and remove it.

---

## Priority Order

When improving a codebase, tackle issues in this sequence:

1. **Compiler warnings** — free wins with explicit fix instructions
2. **Dead code** — find and delete unused functions
3. **Credo strict mode** — fix every violation
4. **Documentation** — add missing `@moduledoc` and `@doc`
5. **Code formatting** — run `mix format`
6. **Structural improvements** — duplicated logic, defensive code, compatibility shims
7. **Test coverage** — add tests for public APIs

### Quick Wins

Almost always worth doing immediately:
- Eliminate every compiler warning
- Remove dead code
- Add missing module aliases (one per line, alphabetical)
- Remove inline CSS
- Run `mix format`
- Add `@moduledoc` and `@doc` where missing

---

## Workflow Checklist

- Always create a branch for improvements and open a pull request
- Eliminate every compiler warning
- Install and run Credo in strict mode
- Run `mix format` before committing
- Add `@moduledoc` to every module, `@doc` to public functions
- Write tests for every major public API function
- Remove dead code
- Remove inline CSS

---

## Code Style Rules

### Alias Style

Each module gets its own alias line. No compound aliases. Alphabetical order, always.

```elixir
# Wrong — compound alias
alias Optimizer.Ecto.{Customer, Repo}

# Right — one per line, sorted
alias Optimizer.Ecto.Customer
alias Optimizer.Ecto.Repo
```

### Pipe Chains Start with Data

Begin pipe chains with the raw value, not a function call. Data flows left to right.

```elixir
# Wrong — starts with function call
Enum.take(list, 5) |> Enum.shuffle() |> pick_winner()

# Right — starts with data
list |> Enum.take(5) |> Enum.shuffle() |> pick_winner()
```

Exception: when starting with a function call genuinely reads better for complex logic. When in doubt, start with data.

### Formatting

Run `mix format` as part of every improvement pass. It's non-negotiable.

### Documentation

When improving modules, add documentation if missing:
- `@moduledoc` at the top of every module
- `@doc` for every public function
- Keep it brief and clear

---

## Code Smells to Watch For

These anti-patterns accumulate in quick builds. Hunt them down and eliminate them.

### Don't Duplicate `attr` Defaults with `assign_new`

When an `attr` declares a `default`, that default is already guaranteed. Adding `assign_new` for the same key is redundant noise.

```elixir
# Wrong — redundant assign_new
attr :warnings, :list, default: [], doc: "List of warning messages"
def warning_list(assigns) do
  assigns = assign_new(assigns, :warnings, fn -> [] end)
  # ...
end

# Right — trust the attr default
attr :warnings, :list, default: [], doc: "List of warning messages"
def warning_list(assigns) do
  # ...
end
```

`assign_new/3` is appropriate only when:
- You need to lazily compute a value not covered by an `attr` default
- The attribute has no default declared
- You need to derive a value from other assigns

```elixir
# Valid use — computing a derived value
attr :user_id, :integer, required: true
def user_profile(assigns) do
  assigns = assign_new(assigns, :user_name, fn ->
    fetch_user_name(assigns.user_id)
  end)
  # ...
end
```

### Don't Add Defensive Formatting for Known Types

If the type is known and guaranteed, don't wrap it in safety checks. Trust the data contract.

```elixir
# Wrong — defensive formatting for a known string list
defp format_warning(warning) when is_binary(warning), do: warning
defp format_warning(warning), do: inspect(warning)

# Right — the type is already known
~H"""
<li :for={warning <- @warnings}>{warning}</li>
"""
```

Defensive formatting belongs at system boundaries — external APIs, user input, genuinely uncertain types. Not inside modules that control their own data.

### One Source of Truth for Logic

Never duplicate validation, transformation, or business logic across functions. Duplicates drift apart and breed bugs.

Watch for:
- The same validation in multiple functions
- Identical MapSet/filter/map operations in different places
- Parallel conditional structures checking the same condition

```elixir
# Wrong — validation duplicated across two functions
def validate_hierarchy(...) do
  hierarchy = profile_hierarchy |> flatten_hierarchy() |> MapSet.new(...)
  keys = MapSet.new(data, fn {taxonomy, _} -> ... end)
  keys |> MapSet.difference(hierarchy) |> Enum.map(...)
end

def add_missing_hierarchies(...) do
  # Same validation logic repeated here
  hierarchy = profile_hierarchy |> flatten_hierarchy() |> MapSet.new(...)
  keys = MapSet.new(parsed.data, fn {taxonomy, _} -> ... end)
  # ...
end

# Right — validate once, trust downstream
defp error_level_validations(...) do
  with :ok <- Parser.validate_hierarchy(%{data: parsed.data}, profile_hierarchy) do
    []
  else
    {:error, errors} when is_list(errors) -> errors
  end
end

# This function only adds missing data — no re-validation
def add_missing_hierarchies(parsed, profile_hierarchy, template_type) do
  parsed
  |> populate_missing_brands(missing_brands, template_type)
  |> Map.update!(:warnings, fn warnings -> warnings ++ missing_warnings end)
  |> then(&{:ok, &1})
end
```

When you find duplicate logic, ask:
1. Does both places actually need this? If only one does, delete the duplicate.
2. If both need it, extract to a shared function.
3. Validate at boundaries. Transform in dedicated functions. Don't mix the two.

### Update Call Sites, Don't Add Compatibility Shims

When changing a function signature, update every call site. Don't leave behind deprecated wrapper functions.

```elixir
# Wrong — backward compatibility layer
def template_change_summary(%{data: parsed, year: year}, profile, user, formatter, type) do
  template_change_summary(parsed, year, profile, user, formatter, type)
end

# Right — one signature, all callers updated
def template_change_summary(parsed, year, profile, user, formatter, template_type) do
  # ...
end
```

One signature. Zero compatibility layers. The codebase stays clean.

---

## Compiler Warnings

Treat warnings as errors. The project accumulates them. Each one carries its own fix instructions — read and apply them.

---

## Credo

Strict mode, every rule. Don't negotiate with Credo.

---

## Functional Code

Pure functions driven by data. Every function should be testable in isolation. When adding or refactoring a function, write the test first — strict TDD. Also test the composition of functions, not just individual units.

---

## Elixir Language Fundamentals

### Core Syntax

- Lists do not support bracket access. Use `Enum.at/2`, pattern matching, or `List` functions:

      mylist = ["blue", "green"]
      Enum.at(mylist, 0)

- Variables are immutable but rebindable. Block expressions like `if`, `case`, `cond` must bind their result to use it:

      # Wrong — rebinding inside the block is lost
      if connected?(socket) do
        socket = assign(socket, :val, val)
      end

      # Right — bind the block's result
      socket =
        if connected?(socket) do
          assign(socket, :val, val)
        end

- Never nest multiple modules in the same file — it causes cyclic dependencies
- Never use map access syntax (`changeset[:field]`) on structs. Use dot access (`my_struct.field`) or higher-level APIs like `Ecto.Changeset.get_field/2`
- Elixir's standard library handles dates and times. Use `Time`, `Date`, `DateTime`, and `Calendar`. Only add `date_time_parser` for parsing, and only if asked
- Don't use `String.to_atom/1` on user input — atoms are never garbage collected
- Predicate functions end with `?` and don't start with `is_`. Reserve `is_` for guards
- OTP primitives like `DynamicSupervisor` and `Registry` require names in their child spec: `{DynamicSupervisor, name: MyApp.MyDynamicSup}`
- Use `Task.async_stream/3` for concurrent enumeration with back-pressure. Almost always pass `timeout: :infinity`

### Pattern Matching

- Prefer pattern matching over conditionals. Match on function heads instead of `if`/`case` in the body
- `%{}` matches any map, not just empty ones. Use `map_size(map) == 0` to check for empty

### Error Handling

- Use `{:ok, result}` and `{:error, reason}` tuples for fallible operations
- Don't raise exceptions for control flow
- Use `with` for chaining operations that return ok/error tuples

### Things That Will Bite You

- No `return` statement, no early returns. The last expression is always the return value
- Don't use `Enum` on large collections when `Stream` is appropriate
- Don't nest `case` statements — refactor to `with` or separate functions
- Don't use `String.to_atom/1` on user input
- Lists can't be indexed with brackets. Use pattern matching or `Enum`
- Prefer `Enum.reduce` over manual recursion. When recursion is necessary, use pattern matching in function heads for the base case
- The process dictionary is a code smell
- Only use macros if explicitly asked
- The standard library is rich — use it

### Function Design

- Use guards: `when is_binary(name) and byte_size(name) > 0`
- Prefer multiple function clauses over complex conditionals
- Name functions clearly: `calculate_total_price/2`, not `calc/2`
- Predicates end with `?`, don't start with `is_`. Reserve `is_` for guards

### Data Structures

- Use structs when the shape is known: `defstruct [:name, :age]`
- Use keyword lists for options: `[timeout: 5000, retries: 3]`
- Use maps for dynamic key-value data
- Prepend to lists: `[new | list]`, not `list ++ [new]`

### Debugging

Use `dbg()` for debugging in development (built into Elixir 1.14+). It's far better than `IO.inspect/2`:

```elixir
# dbg pipelines automatically
list
|> Enum.filter(&(&1 > 0))
|> Enum.map(&(&1 * 2))
|> dbg()
```

Never leave `dbg()` in committed code.

---

## Testing Guidelines

### General Testing

Test coverage starts from zero, so begin simply. Write integration tests first to cover the messy parts, then layer in unit tests as the code improves. Prefer real assertions over mocks.

### Test Process Management

- Use `start_supervised!/1` to start processes — it guarantees cleanup
- Never use `Process.sleep/1` or `Process.alive?/1` in tests
  - To wait for a process: use `Process.monitor/1` and assert on the DOWN message:

        ref = Process.monitor(pid)
        assert_receive {:DOWN, ^ref, :process, ^pid, :normal}

  - To synchronize before the next call: use `:sys.get_state/1` to ensure prior messages are handled

### Database Testing

- Never mock Ecto repositories. Use the actual test database
- The test environment already has a configured test database
- Use the real Repo module. Tests interact with the database directly

### Date Boundary Testing

When testing date-dependent logic — especially fiscal calendars — always test multiple boundaries:

- **Start of year**: First day/week, to catch year-boundary transitions
- **End of year**: Last day/week, especially for 53-week years
- **Mid-year**: A date in the middle, to verify normal operation

```elixir
start_of_year = FiscalCalendar.new(year: 2025, week: 1)

total_weeks = FiscalCalendar.total_weeks_in_year(2025)
end_of_year = FiscalCalendar.new(year: 2025, week: total_weeks)

mid_year = FiscalCalendar.new(year: 2025, week: 20)
```

Fiscal calendars have real edge cases: 53-week years, year-boundary transitions, period 13 and quarter 4 special handling, and week numbering within periods. Test the boundaries or bugs will find them for you.

### Running Tests

- Specific file: `mix test test/my_test.exs`
- Specific test: `mix test path/to/test.exs:123`
- Limit failures: `mix test --max-failures n`
- Failed tests only: `mix test --failed`
- Tag and filter: `@tag` + `mix test --only tag`
- Assert exceptions: `assert_raise ArgumentError, fn -> invalid_function() end`
- Full docs: `mix help test`

---

## Phoenix & LiveView

### Phoenix Router Rules

- `scope` blocks include an optional alias that prefixes all routes within. Be mindful of this to avoid duplicate module prefixes
- You never need your own `alias` in routes — the scope provides it:

      scope "/admin", AppWeb.Admin do
        pipe_through :browser
        live "/users", UserLive, :index
      end

  This points to `AppWeb.Admin.UserLive`.

- `Phoenix.View` no longer exists. Don't use it

### Phoenix LiveView Rules

- Never use `live_redirect` or `live_patch` (deprecated). Use `<.link navigate={href}>`, `<.link patch={href}>`, `push_navigate`, and `push_patch`
- Avoid LiveComponents unless you have a strong reason
- Name LiveViews with a `Live` suffix: `AppWeb.WeatherLive`. The default `:browser` scope already aliases `AppWeb`, so routes are just `live "/weather", WeatherLive`

### Streams

Always use streams for collections. Never assign raw lists — they balloon memory.

```elixir
stream(socket, :messages, [new_msg])                    # append
stream(socket, :messages, [new_msg], reset: true)       # replace all
stream(socket, :messages, [new_msg], at: -1)            # prepend
stream_delete(socket, :messages, msg)                    # delete
```

Template pattern:

```heex
<div id="messages" phx-update="stream">
  <div :for={{id, msg} <- @streams.messages} id={id}>
    {msg.text}
  </div>
</div>
```

Streams are not enumerable — no `Enum.filter/2`. To filter or refresh, refetch and re-stream with `reset: true`:

```elixir
def handle_event("filter", %{"filter" => filter}, socket) do
  messages = list_messages(filter)
  {:noreply,
   socket
   |> assign(:messages_empty?, messages == [])
   |> stream(:messages, messages, reset: true)}
end
```

Streams don't support counting or empty states directly. Track counts with separate assigns. For empty states, use Tailwind:

```heex
<div id="tasks" phx-update="stream">
  <div class="hidden only:block">No tasks yet</div>
  <div :for={{id, task} <- @stream.tasks} id={id}>
    {task.name}
  </div>
</div>
```

When an assign change should affect streamed items, re-stream those items:

```elixir
def handle_event("edit_message", %{"message_id" => message_id}, socket) do
  message = Chat.get_message!(message_id)
  edit_form = to_form(Chat.change_message(message, %{content: message.content}))
  {:noreply,
   socket
   |> stream_insert(:messages, message)
   |> assign(:editing_message_id, String.to_integer(message_id))
   |> assign(:edit_form, edit_form)}
end
```

Never use the deprecated `phx-update="append"` or `phx-update="prepend"`.

### JavaScript Interop

- Any element with `phx-hook` that manages its own DOM must also have `phx-update="ignore"`
- Always provide a unique DOM `id` alongside `phx-hook`

**Colocated JS hooks** — for inline scripts in HEEx:

```heex
<input type="text" id="user-phone-number" phx-hook=".PhoneNumber" />
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

Colocated hook names must start with `.` (e.g., `.PhoneNumber`). Never write raw `<script>` tags in HEEx.

**External hooks** — defined in `assets/js/` and passed to the LiveSocket constructor:

```javascript
const MyHook = {
  mounted() { ... }
}
let liveSocket = new LiveSocket("/live", Socket, {
  hooks: { MyHook }
});
```

**Pushing events** — always return or rebind the socket on `push_event/3`:

```elixir
socket = push_event(socket, "my_event", %{...})
```

Client-side, handle pushed events in hooks with `this.handleEvent`, and push to the server with `this.pushEvent`.

### LiveView Tests

- Use `Phoenix.LiveViewTest` and `LazyHTML` for assertions
- Drive form tests with `render_submit/2` and `render_change/2`
- Plan tests step by step: start with content existence, then add interaction tests
- Reference element IDs from your templates in tests — use `element/2`, `has_element/2`, and selectors
- Never test against raw HTML. Always use structured selectors: `assert has_element?(view, "#my-form")`
- Prefer testing for element presence over text content, which changes
- Test outcomes, not implementation details
- When selectors fail, debug with `LazyHTML`:

      html = render(view)
      document = LazyHTML.from_fragment(html)
      matches = LazyHTML.filter(document, "your-complex-selector")
      IO.inspect(matches, label: "Matches")

### Forms

Create forms from params with `to_form/1`:

```elixir
def handle_event("submitted", %{"user" => user_params}, socket) do
  {:noreply, assign(socket, form: to_form(user_params, as: :user))}
end
```

Create forms from changesets — `:as` is computed automatically:

```elixir
%MyApp.Users.User{}
|> Ecto.Changeset.change()
|> to_form()
```

Template pattern — always use the form assign, never the changeset directly:

```heex
<.form for={@form} id="todo-form" phx-change="validate" phx-submit="save">
  <.input field={@form[:field]} type="text" />
</.form>
```

Rules:
- Always give forms an explicit, unique DOM `id`
- Never pass a changeset to `<.form for={...}>` — always use `to_form/2` first
- Never use `<.form let={f} ...>` — use `<.form for={@form} ...>` and drive references from `@form[:field]`

---

## Mix & Tooling

### Mix Tasks

- Read docs before using tasks: `mix help task_name`
- Debug test failures with `mix test test/my_test.exs` or `mix test --failed`
- `mix deps.clean --all` is almost never what you want. Avoid it unless you have a specific reason
- `mix help` lists available tasks
- `mix help task_name` for documentation
- Read the docs before running unfamiliar tasks

### Formatting

Run `mix format` before every commit. No exceptions.

---

## Summary

Compiler warnings first. Credo strict mode. No duplicated logic. No compatibility shims. Test at boundaries. Document public APIs. Trust the data contract. Don't negotiate.

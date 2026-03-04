---
description: TDD-first Elixir development - write tests before implementation
arguments:
  - name: FEATURE
    description: The feature or function to implement
    required: true
  - name: MODULE
    description: Target module path (e.g., MyApp.Users.Auth)
    required: false
---

# TDD-First Elixir Development

**Write the test FIRST, then implement to make it pass.**

## The TDD Cycle

```
RED → GREEN → REFACTOR
 │       │        │
 │       │        └── Clean up without changing behavior
 │       └── Write minimum code to pass
 └── Write a failing test first
```

## Step 1: Understand the Requirement

Before writing anything, clarify:
- What is the expected input/output?
- What are the edge cases?
- What errors should be handled?

## Step 2: Write the Test First

Create or update the test file at the mirror path:
- Source: `lib/my_app/users/auth.ex`
- Test: `test/my_app/users/auth_test.exs`

```elixir
defmodule MyApp.Users.AuthTest do
  use ExUnit.Case, async: true
  # Or for DB tests: use MyApp.DataCase

  alias MyApp.Users.Auth

  describe "authenticate/2" do
    test "returns user when credentials are valid" do
      # Arrange
      user = insert_user(email: "test@example.com", password: "secret123")

      # Act
      result = Auth.authenticate("test@example.com", "secret123")

      # Assert
      assert {:ok, ^user} = result
    end

    test "returns error when password is invalid" do
      insert_user(email: "test@example.com", password: "secret123")

      assert {:error, :invalid_credentials} = Auth.authenticate("test@example.com", "wrong")
    end

    test "returns error when user not found" do
      assert {:error, :not_found} = Auth.authenticate("nobody@example.com", "any")
    end
  end
end
```

## Step 3: Run the Test (RED)

```bash
mix test test/my_app/users/auth_test.exs
```

The test MUST fail. If it passes, the test isn't testing anything new.

## Step 4: Implement Minimum Code (GREEN)

Write just enough code to make the test pass. Don't over-engineer.

```elixir
defmodule MyApp.Users.Auth do
  def authenticate(email, password) do
    case Repo.get_by(User, email: email) do
      nil -> {:error, :not_found}
      user -> verify_password(user, password)
    end
  end

  defp verify_password(user, password) do
    if Bcrypt.verify_pass(password, user.password_hash) do
      {:ok, user}
    else
      {:error, :invalid_credentials}
    end
  end
end
```

## Step 5: Run Tests Again (GREEN)

```bash
mix test test/my_app/users/auth_test.exs
```

All tests must pass.

## Step 6: Refactor (REFACTOR)

Clean up the code while keeping tests green:
- Extract helper functions
- Improve naming
- Remove duplication
- Add typespecs

```bash
mix test  # Must still pass after refactoring
```

## Step 7: Run Full Quality Suite

```bash
mix precommit
# Runs: format --check-formatted, credo --strict, dialyzer, test --warnings-as-errors
```

---

# Elixir Testing Rules

## Test Structure

```elixir
describe "function_name/arity" do
  test "specific behavior being tested" do
    # Arrange - set up test data
    # Act - call the function
    # Assert - verify the result
  end
end
```

## Assertion Patterns

```elixir
# Pattern match for structure
assert {:ok, %User{id: id}} = create_user(attrs)
assert is_integer(id)

# Pin operator for exact values
user = get_user(1)
assert {:ok, ^user} = fetch_user(1)

# Refute for negative assertions
refute User.admin?(regular_user)

# Assert raises
assert_raise ArgumentError, fn -> parse_date!("invalid") end
assert_raise ArgumentError, ~r/invalid format/, fn -> parse_date!("bad") end
```

## Database Tests

```elixir
# Use DataCase for tests that need the database
use MyApp.DataCase, async: false  # async: false for DB tests

# Always use start_supervised!/1 for processes
{:ok, pid} = start_supervised!(MyWorker)

# Never use Process.sleep - use monitors instead
ref = Process.monitor(pid)
assert_receive {:DOWN, ^ref, :process, ^pid, :normal}, 5000
```

## Mocking with Mox

```elixir
# Define mock in test_helper.exs
Mox.defmock(MockHTTP, for: HTTPBehaviour)

# Use in tests
import Mox

setup :verify_on_exit!

test "fetches data from API" do
  expect(MockHTTP, :get, fn "/users/1" ->
    {:ok, %{status: 200, body: ~s({"name": "Alice"})}}
  end)

  assert {:ok, %{name: "Alice"}} = Users.fetch(1)
end
```

## LiveView Tests

```elixir
import Phoenix.LiveViewTest

test "updates counter on click" do
  {:ok, view, _html} = live(conn, "/counter")

  assert has_element?(view, "#count", "0")

  view |> element("#increment") |> render_click()

  assert has_element?(view, "#count", "1")
end

# Form tests
test "submits form successfully" do
  {:ok, view, _html} = live(conn, "/users/new")

  view
  |> form("#user-form", user: %{name: "Alice", email: "alice@example.com"})
  |> render_submit()

  assert has_element?(view, ".flash-info", "User created")
end
```

---

# Common Patterns

## Testing Return Values

```elixir
# Use {:ok, result} / {:error, reason} tuples
test "successful operation returns ok tuple" do
  assert {:ok, result} = MyModule.do_thing()
  assert result.field == expected
end

test "failed operation returns error tuple" do
  assert {:error, :not_found} = MyModule.find_missing()
end
```

## Testing Side Effects

```elixir
test "sends email on registration" do
  # Capture emails sent during test
  assert {:ok, user} = Accounts.register(%{email: "new@example.com"})

  assert_email_sent(to: "new@example.com", subject: "Welcome!")
end
```

## Testing Concurrency

```elixir
test "handles concurrent requests" do
  tasks = for i <- 1..10 do
    Task.async(fn -> MyModule.process(i) end)
  end

  results = Task.await_many(tasks)

  assert Enum.all?(results, &match?({:ok, _}, &1))
end
```

---

# Anti-Patterns to Avoid

## DON'T: Test Implementation Details

```elixir
# BAD - testing internal state
test "sets internal flag" do
  {:ok, pid} = MyServer.start_link()
  state = :sys.get_state(pid)
  assert state.internal_flag == true  # Don't test internals!
end

# GOOD - test observable behavior
test "responds correctly after initialization" do
  {:ok, pid} = MyServer.start_link()
  assert MyServer.ready?(pid) == true
end
```

## DON'T: Use Sleep for Synchronization

```elixir
# BAD
Process.sleep(100)
assert something_happened()

# GOOD
assert_receive {:event, :completed}, 5000
```

## DON'T: Test Framework Behavior

```elixir
# BAD - testing that Ecto works
test "changeset validates required fields" do
  changeset = User.changeset(%User{}, %{})
  assert changeset.valid? == false
  assert "can't be blank" in errors_on(changeset).email
end

# GOOD - test your validation logic
test "email must be valid format" do
  changeset = User.changeset(%User{}, %{email: "not-an-email"})
  assert "must be a valid email" in errors_on(changeset).email
end
```

---

# Quick Reference

```bash
# Run single test file
mix test test/my_app/feature_test.exs

# Run specific test by line
mix test test/my_app/feature_test.exs:42

# Run previously failed tests
mix test --failed

# Run with coverage
mix test --cover

# Run with trace (verbose)
mix test --trace

# Limit failures
mix test --max-failures 3

# Run tests matching pattern
mix test --only feature_name
```

---

**Now implement $FEATURE using TDD:**

1. Create test file at correct path
2. Write failing test for expected behavior
3. Run test (confirm RED)
4. Implement minimum code
5. Run test (confirm GREEN)
6. Refactor if needed
7. Run `mix precommit`

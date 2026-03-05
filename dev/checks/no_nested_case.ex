defmodule Deciduex.Checks.NoNestedCase do
  @moduledoc """
  Credo check that disallows nested `case` expressions.

  Instead of:
      case x do
        :foo ->
          case y do
            :bar -> ...
          end
      end

  Prefer:
      case {x, y} do
        {:foo, :bar} -> ...
      end

  Or extract to separate functions.
  """

  use Credo.Check,
    base_priority: :high,
    category: :refactor,
    explanations: [
      check: """
      Nested `case` expressions increase cyclomatic complexity and make code
      harder to read and maintain.

      Consider:
      1. Pattern matching on tuples: `case {x, y} do`
      2. Extracting inner cases to helper functions
      3. Using `with` for sequential validations
      """
    ]

  @doc false
  @impl true
  def run(%SourceFile{} = source_file, params) do
    issue_meta = IssueMeta.for(source_file, params)

    Credo.Code.prewalk(source_file, &traverse(&1, &2, issue_meta, 0))
  end

  defp traverse({:case, _meta, [_expr, [do: clauses]]} = ast, issues, issue_meta, _depth) do
    new_issues = find_nested_cases_in_clauses(clauses, issue_meta)
    {ast, new_issues ++ issues}
  end

  defp traverse(ast, issues, _issue_meta, _depth), do: {ast, issues}

  defp find_nested_cases_in_clauses(clauses, issue_meta) do
    clauses
    |> List.wrap()
    |> Enum.flat_map(&find_nested_cases_in_clause(&1, issue_meta))
  end

  defp find_nested_cases_in_clause({:->, _meta, [_patterns, body]}, issue_meta) do
    find_case_expressions(body, issue_meta)
  end

  defp find_nested_cases_in_clause(_ast, _issue_meta), do: []

  defp find_case_expressions(ast, issue_meta) do
    {_ast, issues} =
      Macro.prewalk(ast, [], fn
        {:case, meta, _args} = node, acc ->
          issue = issue_for(issue_meta, meta[:line])
          {node, [issue | acc]}

        node, acc ->
          {node, acc}
      end)

    issues
  end

  defp issue_for(issue_meta, line_no) do
    format_issue(
      issue_meta,
      message:
        "Avoid nested `case` expressions. Use tuple matching `case {x, y} do` or extract to functions.",
      trigger: "case",
      line_no: line_no
    )
  end
end

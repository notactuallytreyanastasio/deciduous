defmodule Deciduex.Checks.NoIfInsideCase do
  @moduledoc """
  Credo check that disallows `if` expressions inside `case` clauses.

  Instead of:
      case x do
        :foo -> if condition, do: a, else: b
      end

  Prefer:
      case {x, condition} do
        {:foo, true} -> a
        {:foo, false} -> b
      end

  Or extract to a separate function.
  """

  use Credo.Check,
    base_priority: :high,
    category: :refactor,
    explanations: [
      check: """
      `if` expressions inside `case` clauses make the control flow harder to follow.

      Consider pattern matching on the additional condition, or extracting the
      conditional logic to a helper function.
      """
    ]

  @doc false
  @impl true
  def run(%SourceFile{} = source_file, params) do
    issue_meta = IssueMeta.for(source_file, params)

    Credo.Code.prewalk(source_file, &traverse(&1, &2, issue_meta))
  end

  defp traverse({:case, _meta, _args} = ast, issues, issue_meta) do
    new_issues = find_if_in_case_clauses(ast, issue_meta)
    {ast, new_issues ++ issues}
  end

  defp traverse(ast, issues, _issue_meta), do: {ast, issues}

  defp find_if_in_case_clauses({:case, _meta, [_expr, [do: clauses]]}, issue_meta) do
    clauses
    |> List.wrap()
    |> Enum.flat_map(&find_if_in_clause(&1, issue_meta))
  end

  defp find_if_in_case_clauses(_ast, _issue_meta), do: []

  defp find_if_in_clause({:->, _meta, [_patterns, body]}, issue_meta) do
    find_if_expressions(body, issue_meta)
  end

  defp find_if_in_clause(_ast, _issue_meta), do: []

  defp find_if_expressions(ast, issue_meta) do
    {_ast, issues} =
      Macro.prewalk(ast, [], fn
        {:if, meta, _args} = node, acc ->
          issue = issue_for(issue_meta, meta[:line], "if")
          {node, [issue | acc]}

        node, acc ->
          {node, acc}
      end)

    issues
  end

  defp issue_for(issue_meta, line_no, trigger) do
    format_issue(
      issue_meta,
      message: "Avoid `if` inside `case` clauses. Use pattern matching or extract to a function.",
      trigger: trigger,
      line_no: line_no
    )
  end
end

defmodule Deciduex.Checks.NoNestedCase do
  @moduledoc false

  use Credo.Check,
    base_priority: :high,
    category: :refactor,
    explanations: [
      check: """
      Avoid nested `case` inside `case` blocks.

      Extract the inner case into a separate function to reduce complexity
      and improve readability.
      """
    ]

  @impl true
  def run(%SourceFile{} = source_file, params) do
    issue_meta = IssueMeta.for(source_file, params)

    Credo.Code.prewalk(source_file, &traverse(&1, &2, issue_meta))
  end

  defp traverse({:case, _meta, [_expr, [do: clauses]]} = ast, issues, issue_meta) do
    new_issues =
      clauses
      |> List.wrap()
      |> Enum.flat_map(fn
        {:->, _arrow_meta, [_patterns, body]} ->
          find_case_in_body(body, issue_meta)

        _ ->
          []
      end)

    {ast, issues ++ new_issues}
  end

  defp traverse(ast, issues, _issue_meta), do: {ast, issues}

  defp find_case_in_body({:case, meta, _args}, issue_meta) do
    [
      format_issue(issue_meta,
        message: "Avoid nested `case` inside `case` — extract inner case to a separate function.",
        line_no: meta[:line]
      )
    ]
  end

  defp find_case_in_body({:__block__, _meta, exprs}, issue_meta) do
    Enum.flat_map(exprs, &find_case_in_body(&1, issue_meta))
  end

  defp find_case_in_body(_other, _issue_meta), do: []
end

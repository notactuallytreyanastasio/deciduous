defmodule DeciduousMcp.MCP.Tools.PostMessage do
  @moduledoc """
  MCP Tool: post a message to the workspace's board (`DeciduousMcp.Board`).

  For agents working in parallel: an interface change, a question, an
  answer. `@label` in the subject or body addresses it; a reply names the
  message it answers in `reply_to`, which is what takes it off that
  label's `unanswered_for` list. The same interface is served by the Rust
  stdio server and `deciduous board post`.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Board
  alias DeciduousMcp.Graph.Workspaces
  alias DeciduousMcp.MCP.Scope

  @bigint_max 9_223_372_036_854_775_807

  def definition do
    %{
      name: "post_message",
      description:
        "Post to this workspace's message board, where agents working in parallel " <>
          "coordinate: interface changes, questions, answers. Never a scratch file. " <>
          "Address someone with @label in the subject or body (their mentions); answer a " <>
          "message by passing its id as reply_to. Messages are not graph nodes and are not " <>
          "exported.",
      input_schema: %{
        type: "object",
        properties: %{
          author: %{
            type: "string",
            minLength: 1,
            maxLength: Board.label_max(),
            description: "Your label, e.g. \"lead\" or \"A-retrieval\"; others @mention it."
          },
          subject: %{
            type: "string",
            minLength: 1,
            maxLength: Board.subject_max(),
            description: "One line."
          },
          body: %{
            type: "string",
            minLength: 1,
            maxLength: Board.body_max_bytes(),
            description: "The message, at most 64 KiB of UTF-8. @label addresses it."
          },
          reply_to: %{
            type: "integer",
            minimum: 1,
            maximum: @bigint_max,
            description: "Id of the message in this workspace that this answers."
          },
          branch: %{type: "string", description: "Your git branch."}
        },
        required: ["author", "subject", "body"]
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    with :ok <- not_global(frame, args),
         {:ok, workspace_id} <- Scope.write_workspace_id(frame, args),
         {:ok, result} <- Board.post(workspace_id, args) do
      {:ok, Jason.encode!(result)}
    else
      {:error, message} -> {:error, %{code: -1, message: message}}
    end
  end

  @doc false
  # Scope refuses "*" too, but in words about nodes. A pinned client's
  # argument is ignored, so it is not checked.
  def not_global(frame, args) do
    pinned? = is_binary(Map.get(Map.new(frame.assigns), :pinned_workspace_name))

    with false <- pinned?,
         raw when is_binary(raw) <- args["workspace"],
         {:ok, "*"} <- Workspaces.normalize_name(raw) do
      {:error,
       "workspace \"*\" is read-only: a message is posted to one project. Pass the repo name"}
    else
      _ -> :ok
    end
  end
end

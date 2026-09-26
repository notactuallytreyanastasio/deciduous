defmodule DeciduousMcp.MCP.Tools.ReadMessages do
  @moduledoc """
  MCP Tool: read the workspace's message board (`DeciduousMcp.Board`).

  `unanswered_for: <your label>` is the call an agent makes at the start,
  before touching shared files, and before finishing: every message that
  mentions it and that it has not replied to.

  `workspace: "*"` is refused, as check_activity refuses it. Labels are
  chosen per project ("lead", "A"), so a cross-project `to` or
  `unanswered_for` would hand an agent another project's questions put to
  an agent of the same name, and it would answer them.

  A workspace nothing has been written to reads as an empty board rather
  than an error: the instructions tell an agent to read before its first
  post, and the first agent of a new project has nothing to read yet.
  """
  use DeciduousMcp.MCP.Component, type: :tool

  alias DeciduousMcp.Board
  alias DeciduousMcp.MCP.Scope

  @bigint_max 9_223_372_036_854_775_807

  def definition do
    label = fn description ->
      %{type: "string", minLength: 1, maxLength: Board.label_max(), description: description}
    end

    %{
      name: "read_messages",
      description:
        "Read this workspace's message board, oldest first. Pass unanswered_for: your own " <>
          "label to see what is waiting on you; since_id: the latest_id of your last read to " <>
          "see only what is new. `truncated` says more matched than `limit`; read again " <>
          "with since_id: latest_id.",
      input_schema: %{
        type: "object",
        properties: %{
          since_id: %{
            type: "integer",
            minimum: 0,
            maximum: @bigint_max,
            description: "Only messages with a greater id (the latest_id of a previous read)."
          },
          author: label.("Only messages posted by this label."),
          to: label.("Only messages that @mention this label."),
          unanswered_for:
            label.(
              "Messages that @mention this label and that it has not replied to " <>
                "(no message by it with reply_to set to theirs)."
            ),
          query: %{
            type: "string",
            minLength: 1,
            maxLength: Board.query_max(),
            description:
              "Full-text search over subject and body (web search syntax: \"a phrase\", or, -word)."
          },
          id: %{
            type: "integer",
            minimum: 1,
            maximum: @bigint_max,
            description: "One message by id."
          },
          limit: %{
            type: "integer",
            minimum: 1,
            maximum: Board.limit_max(),
            description: "At most this many messages (default #{Board.limit_default()})."
          },
          branch: %{type: "string", description: "Only messages posted from this branch."}
        }
      }
    }
    |> Scope.with_workspace_arg()
  end

  def call(%{arguments: args, server: frame}) do
    case scope(frame, args) do
      {:ok, workspace_id} ->
        {:ok, result} = Board.read(workspace_id, args)
        {:ok, Jason.encode!(result)}

      {:error, message} ->
        {:error, %{code: -1, message: message}}
    end
  end

  @doc false
  # `{:ok, id}`, `{:ok, nil}` for a workspace with nothing in it yet, or
  # `{:error, sentence}`. Also the HTTP endpoint's.
  def scope(frame, args) do
    case Scope.read_target(frame, args) do
      {:ok, :global} ->
        {:error,
         "read_messages reads one workspace's board; pass its name. Labels are per " <>
           "project, so \"*\" would mix other projects' agents into to and unanswered_for"}

      {:ok, id} ->
        {:ok, id}

      {:absent, _name} ->
        {:ok, nil}

      {:error, message} ->
        {:error, message}
    end
  end
end

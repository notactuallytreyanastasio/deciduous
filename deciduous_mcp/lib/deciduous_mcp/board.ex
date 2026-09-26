defmodule DeciduousMcp.Board do
  @moduledoc """
  The message board: how agents working in parallel on one workspace tell
  each other about interface changes, ask questions and answer them,
  through the server rather than a scratch markdown file that one agent's
  worktree can see and another's cannot.

  Messages are coordination, not graph. They are not exported, imported or
  synced; `GET /export` does not carry them.

  Both surfaces, the MCP tools `post_message` / `read_messages` and the
  HTTP endpoints `POST /messages` / `GET /messages`, check their arguments
  against the same schema (`check_args/2`) and call `post/2` and `read/2`,
  so a refusal reads the same from either.
  """
  import Ecto.Query

  alias DeciduousMcp.MCP.ArgCheck
  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.AgentMessage

  @subject_max 300
  @body_max_bytes 65_536
  @label_max 100
  @query_max 1000
  @limit_default 50
  @limit_max 200

  def subject_max, do: @subject_max
  def body_max_bytes, do: @body_max_bytes
  def label_max, do: @label_max
  def query_max, do: @query_max
  def limit_default, do: @limit_default
  def limit_max, do: @limit_max

  # --- mentions ---------------------------------------------------------------

  # A label starts and ends with a letter or digit and may hold `_ . -`
  # between; so "ask @lead." is "lead", and a label is at least two
  # characters. The lookbehind is what keeps an email address out: the '@'
  # must open the text or follow something that cannot be part of a local
  # part ([A-Za-z0-9_.]).
  @mention ~r/(?<![A-Za-z0-9_.])@([A-Za-z0-9][A-Za-z0-9_.-]*[A-Za-z0-9])/

  @doc """
  The labels a message mentions: `@label` over `subject <> " " <> body`,
  de-duplicated, in order of first appearance.

      iex> DeciduousMcp.Board.mentions("@lead ready?", "cc @board-cli. mail a@b.com @lead")
      ["lead", "board-cli"]
  """
  @spec mentions(String.t(), String.t()) :: [String.t()]
  def mentions(subject, body) do
    @mention
    |> Regex.scan(subject <> " " <> body, capture: :all_but_first)
    |> Enum.map(&hd/1)
    |> Enum.uniq()
  end

  # --- arguments --------------------------------------------------------------

  @doc """
  Holds `args` (string keys, as sent) to a tool module's advertised schema:
  unknown keys, then types, bounds and blanks. The MCP path runs the same
  checks in `DeciduousMcp.MCP.Component`; the HTTP endpoints call this so
  they refuse what the tools refuse, in the same words.
  """
  def check_args(module, args) do
    definition = module.definition()
    name = definition[:name]

    schema =
      definition[:input_schema]
      |> ArgCheck.with_limits()
      |> ArgCheck.closed()

    with :ok <- ArgCheck.unknown_arguments(name, schema, args) do
      ArgCheck.check(schema, args, name)
    end
  end

  # --- post -------------------------------------------------------------------

  @doc """
  Writes one message to `workspace_id`. `args` has already passed
  `check_args/2`. Returns `{:ok, %{id, mentions, reply_to, created_at}}` or
  `{:error, sentence}`.
  """
  def post(workspace_id, args) do
    subject = args["subject"]
    body = args["body"]

    with :ok <- body_size(body),
         :ok <- reply_target(workspace_id, args["reply_to"]) do
      message =
        Repo.insert!(%AgentMessage{
          workspace_id: workspace_id,
          branch: blank_to_nil(args["branch"]),
          author: args["author"],
          subject: subject,
          body: body,
          mentions: mentions(subject, body),
          reply_to: args["reply_to"]
        })

      {:ok,
       %{
         id: message.id,
         mentions: message.mentions,
         reply_to: message.reply_to,
         created_at: iso(message.created_at)
       }}
    end
  end

  # The schema bounds characters, as every limit in ArgCheck does; the body
  # limit is bytes, the unit "64 KiB" is in, so it is checked here too.
  defp body_size(body) do
    size = byte_size(body)

    if size > @body_max_bytes,
      do: {:error, "body is #{size} bytes; the limit is #{@body_max_bytes} (64 KiB)"},
      else: :ok
  end

  # Another workspace's message is answered exactly as a missing one is:
  # saying "that one is in another workspace" would tell a pinned client
  # which ids exist elsewhere.
  defp reply_target(_workspace_id, nil), do: :ok

  defp reply_target(workspace_id, id) do
    exists =
      Repo.exists?(from m in AgentMessage, where: m.workspace_id == ^workspace_id and m.id == ^id)

    if exists,
      do: :ok,
      else: {:error, "reply_to: no message #{id} in this workspace"}
  end

  # --- read -------------------------------------------------------------------

  @doc """
  Messages in `workspace_id` matching `args`, ascending by id, at most
  `limit`. `workspace_id` may be `nil` for a workspace nothing has been
  written to, which has no messages.

  `latest_id` is the id of the last message returned, or, when none is,
  the `since_id` asked for (0 when none): always the right `since_id` for
  the next poll. `truncated` says more matched than `limit`.
  """
  def read(workspace_id, args) do
    since = args["since_id"] || 0
    limit = args["limit"] || @limit_default

    rows =
      case workspace_id do
        nil ->
          []

        id ->
          id
          |> filtered(args)
          |> where([m], m.id > ^since)
          |> order_by([m], asc: m.id)
          |> limit(^(limit + 1))
          |> Repo.all()
      end

    {page, rest} = Enum.split(rows, limit)

    {:ok,
     %{
       messages: Enum.map(page, &to_map/1),
       latest_id: if(page == [], do: since, else: List.last(page).id),
       truncated: rest != []
     }}
  end

  defp filtered(workspace_id, args) do
    from(m in AgentMessage, as: :m, where: m.workspace_id == ^workspace_id)
    |> filter(:id, args["id"])
    |> filter(:branch, args["branch"])
    |> filter(:author, args["author"])
    |> filter(:to, args["to"])
    |> filter(:unanswered_for, args["unanswered_for"])
    |> filter(:query, args["query"])
  end

  defp filter(q, _key, nil), do: q
  defp filter(q, :id, id), do: where(q, [m], m.id == ^id)
  defp filter(q, :branch, b), do: where(q, [m], m.branch == ^b)
  defp filter(q, :author, a), do: where(q, [m], m.author == ^a)

  # `@>` rather than `= ANY`, so the GIN index on mentions serves it.
  defp filter(q, :to, label),
    do: where(q, [m], fragment("? @> ?::text[]", m.mentions, ^[label]))

  # Addressed to `label`, and `label` has not replied to it. A reply by
  # anyone else does not count: the question was put to `label`.
  defp filter(q, :unanswered_for, label) do
    where(
      q,
      [m],
      fragment("? @> ?::text[]", m.mentions, ^[label]) and
        not exists(
          from r in AgentMessage,
            where:
              r.workspace_id == parent_as(:m).workspace_id and r.reply_to == parent_as(:m).id and
                r.author == ^label
        )
    )
  end

  # The expression is the index's, character for character, or the planner
  # cannot use it.
  defp filter(q, :query, text) do
    where(
      q,
      [m],
      fragment(
        "to_tsvector('english', ? || ' ' || ?) @@ websearch_to_tsquery('english', ?)",
        m.subject,
        m.body,
        ^text
      )
    )
  end

  defp to_map(m) do
    %{
      id: m.id,
      branch: m.branch,
      author: m.author,
      subject: m.subject,
      body: m.body,
      mentions: m.mentions,
      reply_to: m.reply_to,
      created_at: iso(m.created_at)
    }
  end

  @doc "True when the workspace holds at least one message."
  def any?(workspace_id),
    do: Repo.exists?(from m in AgentMessage, where: m.workspace_id == ^workspace_id)

  defp iso(%DateTime{} = dt), do: DateTime.to_iso8601(dt)

  defp blank_to_nil(nil), do: nil
  defp blank_to_nil(s) when is_binary(s), do: if(String.trim(s) == "", do: nil, else: s)
end

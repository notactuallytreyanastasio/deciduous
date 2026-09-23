defmodule DeciduousMcp.Schema.Node do
  @moduledoc """
  A decision graph node. This is the core entity in Deciduous.

  Node types follow the decision flow:
    goal → option → decision → action → outcome

  With observations and revisits attaching anywhere relevant.

  The `change_id` field preserves compatibility with the Rust CLI's sync system.
  The `metadata` JSONB field stores flexible data: confidence (0-100), commit hash,
  user prompt, associated files, and git branch.
  """
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @foreign_key_type :binary_id

  # `feedback` and `done` are not part of the documented vocabulary, but both
  # exist in graphs on disk (one feedback node; 20 nodes carrying status
  # "done"). They are accepted rather than folded into their obvious synonyms:
  # rewriting a recorded status to `completed` on the way in would make the
  # import lossy in a way nothing downstream could detect.
  @node_types ~w(goal decision option action outcome observation revisit feedback)
  @statuses ~w(pending active completed rejected superseded abandoned done)

  schema "decision_nodes" do
    field :change_id, :string
    field :node_type, :string
    field :title, :string
    field :description, :string
    field :status, :string, default: "pending"
    field :metadata, :map, default: %{}
    field :deleted_at, :utc_datetime_usec

    belongs_to :workspace, DeciduousMcp.Schema.Workspace
    belongs_to :created_by, DeciduousMcp.Schema.User, foreign_key: :created_by_id

    has_many :edges_from, DeciduousMcp.Schema.Edge, foreign_key: :from_node_id
    has_many :edges_to, DeciduousMcp.Schema.Edge, foreign_key: :to_node_id
    has_many :documents, DeciduousMcp.Schema.Document

    many_to_many :themes, DeciduousMcp.Schema.Theme,
      join_through: DeciduousMcp.Schema.NodeTheme

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(node, attrs) do
    node
    |> cast(attrs, [
      :change_id, :node_type, :title, :description, :status,
      :metadata, :workspace_id, :created_by_id, :deleted_at
    ])
    |> validate_required([:change_id, :node_type, :title, :workspace_id])
    |> validate_inclusion(:node_type, @node_types)
    |> validate_inclusion(:status, @statuses)
    |> validate_metadata()
    |> unique_constraint([:workspace_id, :change_id])
  end

  def update_changeset(node, attrs) do
    node
    |> cast(attrs, [:title, :description, :status, :metadata, :deleted_at])
    |> validate_inclusion(:status, @statuses)
    |> validate_metadata()
  end

  defp validate_metadata(changeset) do
    case get_change(changeset, :metadata) do
      nil ->
        changeset

      metadata when is_map(metadata) ->
        validate_confidence(changeset, metadata)

      _ ->
        add_error(changeset, :metadata, "must be a map")
    end
  end

  # A string confidence ("999") passed the old number-only guard and was
  # stored; so did `true`. Anything present must be a number in range --
  # but only when this change sets it. update_node merges a patch into the
  # stored map, so a value stored before this check existed (or by an
  # import that skipped it) rode along into every later changeset, and
  # `{metadata: {files: "a"}}` was refused over a confidence it never
  # touched. A key the change leaves as it was is not this change's to
  # judge.
  defp validate_confidence(changeset, metadata) do
    stored = changeset.data.metadata || %{}
    c = metadata["confidence"]

    cond do
      Map.has_key?(stored, "confidence") and stored["confidence"] === c -> changeset
      message = confidence_error(c) -> add_error(changeset, :metadata, message)
      true -> changeset
    end
  end

  @doc """
  Nil for an acceptable confidence (absent, or a number from 0 to 100),
  otherwise the message saying what is wrong with it. Shared with the bulk
  import, which writes with insert_all and never runs a changeset.
  """
  def confidence_error(nil), do: nil
  def confidence_error(c) when is_number(c) and c >= 0 and c <= 100, do: nil

  def confidence_error(c),
    do: "confidence must be a number between 0 and 100, got #{inspect(c)}"

  # Convenience accessors for metadata fields
  def confidence(%__MODULE__{metadata: %{"confidence" => c}}), do: c
  def confidence(_), do: nil

  def commit(%__MODULE__{metadata: %{"commit" => c}}), do: c
  def commit(_), do: nil

  def prompt(%__MODULE__{metadata: %{"prompt" => p}}), do: p
  def prompt(_), do: nil

  def branch(%__MODULE__{metadata: %{"branch" => b}}), do: b
  def branch(_), do: nil

  def files(%__MODULE__{metadata: %{"files" => f}}) when is_list(f), do: f
  def files(_), do: []

  def node_types, do: @node_types
  def statuses, do: @statuses
end

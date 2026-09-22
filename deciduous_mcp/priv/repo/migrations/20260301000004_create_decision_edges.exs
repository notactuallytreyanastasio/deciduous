defmodule DeciduousMcp.Repo.Migrations.CreateDecisionEdges do
  use Ecto.Migration

  def change do
    create table(:decision_edges, primary_key: false) do
      add :id, :binary_id, primary_key: true
      add :workspace_id, references(:workspaces, type: :binary_id, on_delete: :delete_all),
        null: false
      add :from_node_id, references(:decision_nodes, type: :binary_id, on_delete: :delete_all),
        null: false
      add :to_node_id, references(:decision_nodes, type: :binary_id, on_delete: :delete_all),
        null: false

      # For sync with CLI (stores the Rust CLI change_ids)
      add :from_change_id, :string
      add :to_change_id, :string

      # Edge semantics
      add :edge_type, :string, null: false, default: "leads_to"
      add :weight, :float, default: 1.0
      add :rationale, :text

      timestamps(type: :utc_datetime_usec)
    end

    create index(:decision_edges, [:workspace_id])
    create index(:decision_edges, [:from_node_id])
    create index(:decision_edges, [:to_node_id])

    # Prevent duplicate edges of the same type between the same nodes
    create unique_index(:decision_edges, [:from_node_id, :to_node_id, :edge_type])
  end
end

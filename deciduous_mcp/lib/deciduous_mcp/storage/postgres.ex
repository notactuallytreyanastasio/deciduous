defmodule DeciduousMcp.Storage.Postgres do
  @moduledoc """
  Document bytes in a Postgres `bytea`, keyed by content hash.
  """
  @behaviour DeciduousMcp.Storage

  import Ecto.Query

  alias DeciduousMcp.Repo
  alias DeciduousMcp.Schema.DocumentBlob

  @impl true
  def put(hash, content, opts) do
    row = %{
      content_hash: String.downcase(hash),
      content: content,
      byte_size: byte_size(content),
      mime_type: opts[:mime_type],
      inserted_at: DateTime.utc_now()
    }

    # Identical content under the same hash is the normal case on re-import, so
    # a conflict is a no-op rather than a rewrite of bytes that already match.
    Repo.insert_all(DocumentBlob, [row],
      on_conflict: :nothing,
      conflict_target: [:content_hash]
    )

    :ok
  end

  @impl true
  def get(hash) do
    query = from b in DocumentBlob, where: b.content_hash == ^String.downcase(hash), select: b.content

    case Repo.one(query) do
      nil -> {:error, :not_found}
      content -> {:ok, content}
    end
  end

  @impl true
  def exists?(hash) do
    Repo.exists?(from b in DocumentBlob, where: b.content_hash == ^String.downcase(hash))
  end

  @impl true
  def delete(hash) do
    Repo.delete_all(from b in DocumentBlob, where: b.content_hash == ^String.downcase(hash))
    :ok
  end
end

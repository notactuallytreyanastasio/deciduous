defmodule DeciduousMcp.Tokens do
  @moduledoc """
  Named bearer tokens: one per person or agent.

  `DECIDUOUS_MCP_TOKEN` stays what it was, the token the server will not
  boot without, and becomes the only one that can issue or revoke the
  others. A named token can do everything else the shared one can. It does
  not narrow which workspaces its holder can read or write; what it adds is
  a name on what the holder writes (`DeciduousMcp.Activity`) and a way to
  take one holder's access away.

  Only the SHA-256 of a token is stored. The token itself is returned once,
  by `create/1`.
  """

  alias DeciduousMcp.Repo

  @prefix "dcd_"
  # How stale last_used_at may be: a write per request to say a token was
  # used a second ago is a write nobody reads.
  @touch_after_seconds 60

  @doc """
  Issues a token for `name`. `{:ok, token}`, or `{:error, message}` when the
  name is not usable or has been issued before, revoked or not.
  """
  def create(name) do
    with {:ok, name} <- check_name(name) do
      token = @prefix <> Base.url_encode64(:crypto.strong_rand_bytes(32), padding: false)

      result =
        Repo.query!(
          "INSERT INTO api_tokens (name, token_hash) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING",
          [name, hash(token)]
        )

      case result.num_rows do
        1 ->
          {:ok, token}

        0 ->
          {:error, "a token named #{inspect(name)} has been issued before; names are not reused"}
      end
    end
  end

  @doc "The name a presented token belongs to, or `:error`."
  def verify(presented) when is_binary(presented) do
    if String.starts_with?(presented, @prefix) do
      rows =
        Repo.query!(
          "SELECT name, last_used_at FROM api_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
          [hash(presented)]
        ).rows

      case rows do
        [[name, last_used]] ->
          touch(name, last_used)
          {:ok, name}

        _ ->
          :error
      end
    else
      :error
    end
  end

  def verify(_), do: :error

  @doc "Revokes `name`'s token. `:ok`, or `{:error, message}` when there is none to revoke."
  def revoke(name) when is_binary(name) do
    result =
      Repo.query!(
        "UPDATE api_tokens SET revoked_at = (now() AT TIME ZONE 'UTC') WHERE name = $1 AND revoked_at IS NULL",
        [name]
      )

    case result.num_rows do
      1 -> :ok
      0 -> {:error, "no live token is named #{inspect(name)}"}
    end
  end

  @doc "Every token ever issued, oldest first, without its hash."
  def list do
    Repo.query!(
      "SELECT name, created_at, revoked_at, last_used_at FROM api_tokens ORDER BY created_at, name",
      []
    ).rows
    |> Enum.map(fn [name, created, revoked, used] ->
      %{name: name, created_at: iso(created), revoked_at: iso(revoked), last_used_at: iso(used)}
    end)
  end

  defp touch(name, last_used) do
    stale? =
      is_nil(last_used) or
        DateTime.diff(DateTime.utc_now(), as_utc(last_used), :second) >= @touch_after_seconds

    if stale? do
      Repo.query!(
        "UPDATE api_tokens SET last_used_at = (now() AT TIME ZONE 'UTC') WHERE name = $1",
        [name]
      )
    end

    :ok
  end

  # A name is shown in check_activity and typed in a revoke: letters,
  # digits and the punctuation of an email address or a host name.
  defp check_name(name) when is_binary(name) do
    if Regex.match?(~r/\A[A-Za-z0-9][A-Za-z0-9._@+-]{0,99}\z/, name) do
      {:ok, name}
    else
      {:error,
       "a token name is 1 to 100 of letters, digits, '.', '_', '@', '+' and '-', starting with a letter or digit"}
    end
  end

  defp check_name(_), do: {:error, "a token needs a name"}

  defp hash(token), do: :crypto.hash(:sha256, token)

  defp iso(nil), do: nil
  defp iso(t), do: t |> as_utc() |> DateTime.to_iso8601()

  defp as_utc(%NaiveDateTime{} = naive), do: DateTime.from_naive!(naive, "Etc/UTC")
  defp as_utc(%DateTime{} = dt), do: dt
end

defmodule DeciduousMcp.Graph.CommonWords do
  @moduledoc """
  Common English words, the prior `DeciduousMcp.Graph.Retrieval` puts on a
  question word the graph never recorded.

  A word no node contains is evidence that the question is about something
  the graph does not have only when the word would have been expected to
  show up if the graph had that topic. The graph cannot say which words
  those are: on the 56-node eval fixture, 13 of 36 answerable questions
  contain a word no node has ("algorithm", "much", "trying", "shipped"), and
  "What algorithm do we use for API rate limiting?" has the same document
  frequencies as "What rate limiting did we pick for websocket
  connections?". What separates them is the language: "algorithm" is an
  ordinary word whose absence from a small graph means nothing, "websocket"
  is a term of art.

  The list is SCOWL (http://wordlist.aspell.net/) at size 35, the size
  SCOWL itself calls a small dictionary: 42,061 lowercased forms, with
  inflections and contractions (apostrophes removed, as extract_terms
  writes them). The cutoff is SCOWL's own size class, not fitted to any
  question set. Its copyright notice is at the top of
  `priv/common_english_words.txt`.
  """

  @path Path.join([__DIR__, "..", "..", "..", "priv", "common_english_words.txt"])
        |> Path.expand()
  @external_resource @path

  @words @path
         |> File.read!()
         |> String.split("\n", trim: true)
         |> Enum.reject(&String.starts_with?(&1, "#"))
         |> MapSet.new()

  @doc """
  True when `word` (lowercase) is a common English word. A hyphenated word
  is common when every part is ("per-ip" is not: "ip" is not in the list;
  "well-known" is).
  """
  def common?(word) when is_binary(word) do
    MapSet.member?(@words, word) or
      (String.contains?(word, "-") and
         word
         |> String.split("-", trim: true)
         |> then(&(&1 != [] and Enum.all?(&1, fn p -> MapSet.member?(@words, p) end))))
  end

  @doc "How many words the list holds."
  def size, do: MapSet.size(@words)
end

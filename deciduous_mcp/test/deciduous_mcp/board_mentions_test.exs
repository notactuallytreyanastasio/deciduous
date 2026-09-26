defmodule DeciduousMcp.BoardMentionsTest do
  @moduledoc """
  Mention extraction, against the rules of the board interface (board post
  16): `@([A-Za-z0-9][A-Za-z0-9_.-]*[A-Za-z0-9])` over subject <> " " <>
  body, de-duplicated in order of first appearance, a trailing '.' or '-'
  not part of the label, and no mention inside an email address. The Rust
  side implements the same rules; these cases are the contract.
  """
  use ExUnit.Case, async: true

  import DeciduousMcp.Board, only: [mentions: 2]

  doctest DeciduousMcp.Board

  test "subject and body both count, subject first" do
    assert mentions("for @lead", "and @board-cli") == ["lead", "board-cli"]
  end

  test "de-duplicated, in order of first appearance" do
    assert mentions("@bb @aa", "@aa @bb @cc @aa") == ["bb", "aa", "cc"]
  end

  test "a trailing '.' or '-' is not part of the label" do
    assert mentions("", "ask @lead.") == ["lead"]
    assert mentions("", "ask @lead-") == ["lead"]
    assert mentions("", "ask @lead...") == ["lead"]
    assert mentions("", "ask @lead_") == ["lead"]
  end

  test "'_', '.' and '-' inside a label are kept" do
    assert mentions("", "@A-retrieval @x.y @snake_case @v1.0.9") ==
             ["A-retrieval", "x.y", "snake_case", "v1.0.9"]
  end

  test "an email address is not a mention" do
    assert mentions("", "mail bob@example.com or bob.smith@example.com") == []
    assert mentions("", "a_b@host") == []
  end

  test "any other character before '@' is fine, and so is start of text" do
    assert mentions("@start", "(@paren) \"@quoted\" ,@comma\n@newline -@dash @@double") ==
             ["start", "paren", "quoted", "comma", "newline", "dash", "double"]
  end

  test "the subject and body are joined with a space, so a label cannot span them" do
    assert mentions("hi @le", "ad") == ["le"]
    assert mentions("x", "y") == []
  end

  test "a label is at least two characters, as the pattern says" do
    assert mentions("", "@a @b1 @") == ["b1"]
  end

  test "a label ends at the first character the pattern does not take" do
    assert mentions("", "@lead's @lead, @lead: @lead/x") == ["lead"]
  end

  test "case is kept, and labels differing in case are different labels" do
    assert mentions("", "@Lead @lead") == ["Lead", "lead"]
  end

  test "letters outside ASCII are not label characters" do
    assert mentions("", "@café") == ["caf"]
    assert mentions("", "é@lead") == ["lead"]
  end
end

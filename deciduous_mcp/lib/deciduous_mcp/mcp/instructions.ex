defmodule DeciduousMcp.MCP.Instructions do
  @moduledoc """
  The text every client receives in the `initialize` result's
  `instructions`, which Claude Code and other clients put in the model's
  context for the whole session.

  This replaces the log-loop hook removed in 1.0.3. The hook refused tool
  calls until the agent wrote to the graph, and could not be satisfied from
  inside a session once its counter stopped resetting. Instructions cannot
  block anything. Every client that connects reads them at the start of
  every session, with no project configuration, which is where the hook
  was weakest.

  Keep it short: every session pays for it in context.
  """

  @text """
  deciduous is this project's decision graph: why the code is the way it is. Write to it while you work, at these moments, not afterwards:

  - The user asks for something: a goal, with their message verbatim in `prompt`.
  - You see more than one way to do it: options. You pick one: a decision whose title names what it beat.
  - You change code or config: an action. It works or fails: an outcome with the number or the error.
  - You learn a fact that changes the plan: an observation. An earlier choice turns out wrong: a revisit.

  One step, one call: `capture_conversation_turn` writes a step's goal, observations, decision, action and outcome together, all or nothing. Pass `parent_node_id` to put it under the goal you are working on. For a single node, `add_node` with `parent_id` creates and links it in one call. Never send `add_edge` in the same batch as the `add_node` whose id it needs.

  Pass `workspace` (the repository root's directory name, also from a worktree) and `branch` (the current branch) on every write. Do not log your own reading, searching or planning.

  At the start of a session, read before writing: `query_nodes` for this branch, `ask_graph` for the topic. If the work continues an existing goal, attach to it. When a line of work ends: `close_thread`, then `find_orphans`.
  """

  @doc "The instructions text sent on initialize."
  @spec text() :: String.t()
  def text, do: String.trim(@text)
end

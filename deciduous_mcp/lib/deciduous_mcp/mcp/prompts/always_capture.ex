defmodule DeciduousMcp.MCP.Prompts.AlwaysCapture do
  @moduledoc """
  MCP Prompt: deciduous_always_on

  Injects system-level instructions into any connecting MCP client, telling
  the AI assistant to automatically capture conversation reasoning into the
  decision graph after every meaningful exchange.

  This is the MCP-native way to make Deciduous "always-on" — any client that
  supports MCP prompts will receive these instructions without needing
  CLAUDE.md or other out-of-band configuration.
  """
  use Hermes.Server.Component, type: :prompt

  @impl true
  def definition do
    %{
      name: "deciduous_always_on",
      description:
        "System instructions for automatically capturing conversation reasoning " <>
          "into the Deciduous decision graph. Enable this to have every conversation's " <>
          "thoughts, decisions, and outcomes tracked automatically.",
      arguments: [
        %{
          name: "verbosity",
          description:
            "How much to capture: 'full' logs every exchange, 'decisions_only' logs only " <>
              "decision points and outcomes, 'minimal' logs only major milestones.",
          required: false
        }
      ]
    }
  end

  @impl true
  def call(%{arguments: args, server: _frame}) do
    verbosity = (args || %{})["verbosity"] || "full"
    instructions = build_instructions(verbosity)

    {:ok,
     %{
       messages: [
         %{
           role: "system",
           content: %{
             type: "text",
             text: instructions
           }
         }
       ]
     }}
  end

  defp build_instructions("full") do
    """
    # Deciduous Decision Graph — Always-On Capture

    You have access to a Deciduous decision graph via MCP tools. You MUST use these
    tools to capture the reasoning from every meaningful conversation exchange.

    ## After Each Substantive Exchange

    Call `capture_conversation_turn` with a structured summary of what happened:

    - **goal**: If the user requested something new, capture it as a goal
    - **observations**: Things you noticed, learned, or discovered
    - **options_considered**: Approaches you evaluated (mark which was chosen)
    - **decision**: When you chose between approaches, log the choice and rationale
    - **action**: What you actually did or implemented
    - **outcome**: The result — did it work? What happened?

    ## Quick-Fire Tools

    For specific moments, use the dedicated tools:

    - `log_decision` — When choosing between approaches (creates decision + option nodes atomically)
    - `log_observation` — When you notice something interesting or learn a constraint
    - `close_thread` — When a line of work reaches a conclusion (logs outcome + lessons + next steps)

    ## Querying the Graph

    Use `ask_graph` to answer questions about the project's history:
    - "What did we decide about authentication?"
    - "What goals are still pending?"
    - "Trace how we got to the current caching approach"

    ## The Node Flow Rule

    Always follow the canonical flow: goal → options → decision → action → outcome
    - Goals lead to options (possible approaches)
    - Options lead to a decision (choosing which to pursue)
    - Decisions lead to actions (implementation)
    - Actions lead to outcomes (results)
    - Observations attach anywhere relevant

    ## Threading

    Use `parent_node_id` to connect conversation turns into a coherent thread.
    After creating a goal, pass its ID as the parent for subsequent turns about
    that goal. This builds the graph's connective tissue.

    ## Capture Rules

    1. Log BEFORE you do something (goal/decision), not just after
    2. Log the OUTCOME after it succeeds or fails
    3. CONNECT every node — orphan nodes lose context
    4. Capture the user's VERBATIM prompt on root goal nodes
    5. Don't log trivial exchanges (greetings, clarifications) — only substantive work
    """
  end

  defp build_instructions("decisions_only") do
    """
    # Deciduous Decision Graph — Decision Capture Mode

    You have access to a Deciduous decision graph. Log decision points and outcomes:

    - Use `log_decision` when choosing between approaches
    - Use `close_thread` when work reaches a conclusion
    - Use `log_observation` for important constraints or learnings
    - Use `ask_graph` to query project history

    Only log when actual decisions are made or outcomes are reached. Skip routine
    implementation details.
    """
  end

  defp build_instructions("minimal") do
    """
    # Deciduous Decision Graph — Milestone Capture

    You have access to a Deciduous decision graph. Log major milestones only:

    - Use `capture_conversation_turn` for significant project events
    - Use `close_thread` when major goals are completed
    - Use `ask_graph` to query project history

    Only log when something truly significant happens — new features started,
    major pivots, project milestones reached.
    """
  end

  defp build_instructions(_), do: build_instructions("full")
end

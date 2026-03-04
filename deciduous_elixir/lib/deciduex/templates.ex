defmodule Deciduex.Templates do
  @moduledoc """
  Template rendering for deciduous init/update.

  Embeds templates at compile time from priv/templates/ and renders them
  with variable substitution.
  """

  @template_dir Path.join(:code.priv_dir(:deciduex), "templates")

  # Claude commands
  @decision_md File.read!(Path.join(@template_dir, "claude/commands/decision.md"))
  @recover_md File.read!(Path.join(@template_dir, "claude/commands/recover.md"))
  @work_md File.read!(Path.join(@template_dir, "claude/commands/work.md"))
  @document_md File.read!(Path.join(@template_dir, "claude/commands/document.md"))
  @build_test_md File.read!(Path.join(@template_dir, "claude/commands/build-test.md"))
  @serve_ui_md File.read!(Path.join(@template_dir, "claude/commands/serve-ui.md"))
  @sync_graph_md File.read!(Path.join(@template_dir, "claude/commands/sync-graph.md"))
  @sync_md File.read!(Path.join(@template_dir, "claude/commands/sync.md"))
  @decision_graph_md File.read!(Path.join(@template_dir, "claude/commands/decision-graph.md"))

  # Claude skills
  @pulse_skill File.read!(Path.join(@template_dir, "claude/skills/pulse.md"))
  @narratives_skill File.read!(Path.join(@template_dir, "claude/skills/narratives.md"))
  @archaeology_skill File.read!(Path.join(@template_dir, "claude/skills/archaeology.md"))

  # Claude hooks
  @claude_hook_action File.read!(Path.join(@template_dir, "claude/hooks/require-action-node.sh"))
  @claude_hook_commit File.read!(Path.join(@template_dir, "claude/hooks/post-commit-reminder.sh"))

  # Config files
  @default_config File.read!(Path.join(@template_dir, "config/deciduous.toml"))
  @claude_md_section File.read!(Path.join(@template_dir, "config/claude-md-section.md"))
  @settings_json File.read!(Path.join(@template_dir, "config/settings.json"))
  @agents_toml File.read!(Path.join(@template_dir, "config/agents.toml"))

  # Workflows
  @deploy_workflow File.read!(Path.join(@template_dir, "workflows/deploy-decision-graph.yml"))
  @cleanup_workflow File.read!(Path.join(@template_dir, "workflows/cleanup-decision-graphs.yml"))

  # OpenCode commands
  @opencode_decision File.read!(Path.join(@template_dir, "opencode/commands/decision.md"))
  @opencode_recover File.read!(Path.join(@template_dir, "opencode/commands/recover.md"))
  @opencode_work File.read!(Path.join(@template_dir, "opencode/commands/work.md"))
  @opencode_document File.read!(Path.join(@template_dir, "opencode/commands/document.md"))
  @opencode_build_test File.read!(Path.join(@template_dir, "opencode/commands/build-test.md"))
  @opencode_serve_ui File.read!(Path.join(@template_dir, "opencode/commands/serve-ui.md"))
  @opencode_sync_graph File.read!(Path.join(@template_dir, "opencode/commands/sync-graph.md"))
  @opencode_sync File.read!(Path.join(@template_dir, "opencode/commands/sync.md"))
  @opencode_decision_graph File.read!(Path.join(@template_dir, "opencode/commands/decision-graph.md"))

  # OpenCode skills
  @opencode_pulse File.read!(Path.join(@template_dir, "opencode/skills/pulse.md"))
  @opencode_narratives File.read!(Path.join(@template_dir, "opencode/skills/narratives.md"))
  @opencode_archaeology File.read!(Path.join(@template_dir, "opencode/skills/archaeology.md"))

  # OpenCode agents/tools/plugins
  @opencode_agent File.read!(Path.join(@template_dir, "opencode/agents/deciduous.md"))
  @opencode_tool File.read!(Path.join(@template_dir, "opencode/tools/deciduous.ts"))
  @opencode_plugin_action File.read!(Path.join(@template_dir, "opencode/plugins/require-action-node.ts"))
  @opencode_plugin_commit File.read!(Path.join(@template_dir, "opencode/plugins/post-commit-reminder.ts"))

  # Windsurf
  @windsurf_rules File.read!(Path.join(@template_dir, "windsurf/rules/deciduous.md"))
  @windsurf_hooks_json File.read!(Path.join(@template_dir, "windsurf/hooks.json"))
  @windsurf_hook_action File.read!(Path.join(@template_dir, "windsurf/hooks/require-action-node.sh"))
  @windsurf_hook_commit File.read!(Path.join(@template_dir, "windsurf/hooks/post-commit-reminder.sh"))

  @doc "Get template content by name"
  def get(name, assigns \\ %{})

  # Claude commands
  def get(:decision_md, assigns), do: render(@decision_md, assigns)
  def get(:recover_md, assigns), do: render(@recover_md, assigns)
  def get(:work_md, assigns), do: render(@work_md, assigns)
  def get(:document_md, assigns), do: render(@document_md, assigns)
  def get(:build_test_md, assigns), do: render(@build_test_md, assigns)
  def get(:serve_ui_md, assigns), do: render(@serve_ui_md, assigns)
  def get(:sync_graph_md, assigns), do: render(@sync_graph_md, assigns)
  def get(:sync_md, assigns), do: render(@sync_md, assigns)
  def get(:decision_graph_md, assigns), do: render(@decision_graph_md, assigns)

  # Claude skills
  def get(:pulse_skill, assigns), do: render(@pulse_skill, assigns)
  def get(:narratives_skill, assigns), do: render(@narratives_skill, assigns)
  def get(:archaeology_skill, assigns), do: render(@archaeology_skill, assigns)

  # Claude hooks
  def get(:claude_hook_action, assigns), do: render(@claude_hook_action, assigns)
  def get(:claude_hook_commit, assigns), do: render(@claude_hook_commit, assigns)

  # Config
  def get(:default_config, assigns), do: render(@default_config, assigns)
  def get(:claude_md_section, assigns), do: render(@claude_md_section, assigns)
  def get(:settings_json, assigns), do: render(@settings_json, assigns)
  def get(:agents_toml, assigns), do: render(@agents_toml, assigns)

  # Workflows
  def get(:deploy_workflow, assigns), do: render(@deploy_workflow, assigns)
  def get(:cleanup_workflow, assigns), do: render(@cleanup_workflow, assigns)

  # OpenCode commands
  def get(:opencode_decision, assigns), do: render(@opencode_decision, assigns)
  def get(:opencode_recover, assigns), do: render(@opencode_recover, assigns)
  def get(:opencode_work, assigns), do: render(@opencode_work, assigns)
  def get(:opencode_document, assigns), do: render(@opencode_document, assigns)
  def get(:opencode_build_test, assigns), do: render(@opencode_build_test, assigns)
  def get(:opencode_serve_ui, assigns), do: render(@opencode_serve_ui, assigns)
  def get(:opencode_sync_graph, assigns), do: render(@opencode_sync_graph, assigns)
  def get(:opencode_sync, assigns), do: render(@opencode_sync, assigns)
  def get(:opencode_decision_graph, assigns), do: render(@opencode_decision_graph, assigns)

  # OpenCode skills
  def get(:opencode_pulse, assigns), do: render(@opencode_pulse, assigns)
  def get(:opencode_narratives, assigns), do: render(@opencode_narratives, assigns)
  def get(:opencode_archaeology, assigns), do: render(@opencode_archaeology, assigns)

  # OpenCode agents/tools/plugins
  def get(:opencode_agent, assigns), do: render(@opencode_agent, assigns)
  def get(:opencode_tool, assigns), do: render(@opencode_tool, assigns)
  def get(:opencode_plugin_action, assigns), do: render(@opencode_plugin_action, assigns)
  def get(:opencode_plugin_commit, assigns), do: render(@opencode_plugin_commit, assigns)

  # Windsurf
  def get(:windsurf_rules, assigns), do: render(@windsurf_rules, assigns)
  def get(:windsurf_hooks_json, assigns), do: render(@windsurf_hooks_json, assigns)
  def get(:windsurf_hook_action, assigns), do: render(@windsurf_hook_action, assigns)
  def get(:windsurf_hook_commit, assigns), do: render(@windsurf_hook_commit, assigns)

  @doc """
  List all Claude command files to write.
  Returns list of {relative_path, template_name}.
  """
  def claude_commands do
    [
      {".claude/commands/decision.md", :decision_md},
      {".claude/commands/recover.md", :recover_md},
      {".claude/commands/work.md", :work_md},
      {".claude/commands/document.md", :document_md},
      {".claude/commands/build-test.md", :build_test_md},
      {".claude/commands/serve-ui.md", :serve_ui_md},
      {".claude/commands/sync-graph.md", :sync_graph_md},
      {".claude/commands/sync.md", :sync_md},
      {".claude/commands/decision-graph.md", :decision_graph_md}
    ]
  end

  @doc """
  List all Claude skill files to write.
  """
  def claude_skills do
    [
      {".claude/skills/pulse.md", :pulse_skill},
      {".claude/skills/narratives.md", :narratives_skill},
      {".claude/skills/archaeology.md", :archaeology_skill}
    ]
  end

  @doc """
  List all Claude hook files to write (executable).
  """
  def claude_hooks do
    [
      {".claude/hooks/require-action-node.sh", :claude_hook_action},
      {".claude/hooks/post-commit-reminder.sh", :claude_hook_commit}
    ]
  end

  @doc """
  List all OpenCode command files to write.
  """
  def opencode_commands do
    [
      {".opencode/commands/decision.md", :opencode_decision},
      {".opencode/commands/recover.md", :opencode_recover},
      {".opencode/commands/work.md", :opencode_work},
      {".opencode/commands/document.md", :opencode_document},
      {".opencode/commands/build-test.md", :opencode_build_test},
      {".opencode/commands/serve-ui.md", :opencode_serve_ui},
      {".opencode/commands/sync-graph.md", :opencode_sync_graph},
      {".opencode/commands/sync.md", :opencode_sync},
      {".opencode/commands/decision-graph.md", :opencode_decision_graph}
    ]
  end

  @doc """
  List all OpenCode skill files to write.
  """
  def opencode_skills do
    [
      {".opencode/skills/pulse.md", :opencode_pulse},
      {".opencode/skills/narratives.md", :opencode_narratives},
      {".opencode/skills/archaeology.md", :opencode_archaeology}
    ]
  end

  @doc """
  List all Windsurf files to write.
  """
  def windsurf_files do
    [
      {".windsurf/rules/deciduous.md", :windsurf_rules},
      {".windsurf/hooks.json", :windsurf_hooks_json},
      {".windsurf/hooks/require-action-node.sh", :windsurf_hook_action},
      {".windsurf/hooks/post-commit-reminder.sh", :windsurf_hook_commit}
    ]
  end

  @doc """
  List GitHub workflow files to write.
  """
  def workflow_files do
    [
      {".github/workflows/deploy-decision-graph.yml", :deploy_workflow},
      {".github/workflows/cleanup-decision-graphs.yml", :cleanup_workflow}
    ]
  end

  # Simple template rendering - just string substitution
  defp render(template, assigns) when is_map(assigns) and map_size(assigns) == 0 do
    template
  end

  defp render(template, assigns) when is_map(assigns) do
    Enum.reduce(assigns, template, fn {key, value}, acc ->
      String.replace(acc, "${#{key}}", to_string(value))
    end)
  end
end

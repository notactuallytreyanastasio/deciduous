// OpenCode Plugin: Post-Commit Reminder
// Reminds to link commits to the decision graph after git commit
// This ensures commits are connected to the reasoning that led to them

import type { Plugin } from "@opencode-ai/plugin"

export const PostCommitReminder: Plugin = async ({ $ }) => {
  return {
    "tool.execute.after": async (input) => {
      // Only check bash tool
      if (input.tool !== "bash") {
        return
      }

      // Check if deciduous is initialized
      const fs = await import("fs")
      if (!fs.existsSync(".deciduous")) {
        return
      }

      // Check if this was a git commit command
      const command = input.args?.command || ""
      if (!command.match(/^git commit/)) {
        return
      }

      try {
        // Get the latest commit info
        const hashResult = await $`git rev-parse --short HEAD 2>/dev/null`.quiet()
        const msgResult = await $`git log -1 --format=%s 2>/dev/null`.quiet()

        const commitHash = hashResult.stdout.toString().trim()
        const commitMsg = msgResult.stdout.toString().trim().slice(0, 50)

        // Show reminder
        console.log(`
+===================================================================+
|  DECIDUOUS: Link this commit to the decision graph!               |
+===================================================================+
|  Commit: ${commitHash} "${commitMsg}"
|                                                                   |
|  Run NOW:                                                         |
|    deciduous add outcome "What was accomplished" -c 95 --commit HEAD
|    deciduous link <action_id> <outcome_id> -r "Implementation complete"
|                                                                   |
|  Or if this was an action (not outcome):                          |
|    deciduous add action "What was done" -c 90 --commit HEAD       |
+===================================================================+
`)
      } catch (error) {
        // If git commands fail, skip the reminder
      }
    }
  }
}

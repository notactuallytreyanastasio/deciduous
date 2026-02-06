// OpenCode Plugin: Require Action Node
// Checks for recent action/goal nodes before file edits
// This enforces the decision graph workflow: log BEFORE you code

import type { Plugin } from "@opencode-ai/plugin"

export const RequireActionNode: Plugin = async ({ $ }) => {
  return {
    "tool.execute.before": async (input, output) => {
      // Only check on edit and write tools
      if (input.tool !== "edit" && input.tool !== "write") {
        return
      }

      try {
        // Get recent nodes from deciduous
        const result = await $`deciduous nodes 2>/dev/null | head -20`.quiet()
        const stdout = result.stdout.toString()

        const lines = stdout.trim().split("\n").filter((l: string) => l.trim())

        // Check for any goal or action node
        let hasRecentNode = false
        for (const line of lines) {
          if (line.match(/goal|action/i)) {
            hasRecentNode = true
            break
          }
        }

        if (!hasRecentNode && lines.length > 2) {
          // Show a toast reminder but don't block
          console.log(`
╔═══════════════════════════════════════════════════════════════════╗
║  DECIDUOUS: Consider adding a goal/action node!                   ║
╠═══════════════════════════════════════════════════════════════════╣
║  Before editing files, log what you're about to do:               ║
║    deciduous add action "Your action description" -c 85           ║
╚═══════════════════════════════════════════════════════════════════╝
`)
        }
      } catch (error) {
        // If deciduous isn't available, continue silently
      }
    }
  }
}

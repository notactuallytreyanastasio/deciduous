// OpenCode Tool: Deciduous Decision Graph
// Provides deciduous CLI integration for OpenCode

import type { Tool } from "@opencode-ai/tool"

export const DecisionGraphTool: Tool = {
  name: "deciduous",
  description: "Manage the deciduous decision graph for tracking design decisions",

  async execute(args: { command: string }) {
    const { execSync } = await import("child_process")

    try {
      const result = execSync(`deciduous ${args.command}`, {
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"]
      })
      return { success: true, output: result }
    } catch (error: any) {
      return { success: false, error: error.message }
    }
  }
}

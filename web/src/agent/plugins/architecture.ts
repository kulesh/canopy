/**
 * Architecture Plugin (Chat Agent)
 *
 * Provides the chat agent with awareness of the architecture notebook
 * and codebase context. Scanning and change proposals are handled by
 * dedicated agents (scanner.ts, proposer.ts).
 *
 * Contributions:
 * - System prompt: codebase context
 * - Skills: rescan-components (triggers scanner agent re-analysis)
 */

import type { Plugin, PluginContext, Skill } from "../plugins.js";

// --- System prompt ---

function chatContext(ctx: PluginContext): string {
  if (!ctx.projectHandle) {
    return `No project directory is currently open. Ask the user to open a project using the folder button in the header bar.`;
  }

  return `A project is open and its architecture has been (or is being) scanned into a notebook panel. You can answer questions about the codebase using your general knowledge and the context from our conversation.

You have tools for exploring the codebase:
- **list_directory(path?)**: List files and subdirectories
- **read_file(path)**: Read a file's contents with line numbers`;
}

// --- Skills ---

function createSkills(): Skill[] {
  return [
    {
      id: "rescan-components",
      label: "Re-scan Components",
      prompt(params) {
        const names = params.names as string[];
        return [
          `The following components were recently modified: ${names.join(", ")}.`,
          ``,
          `Please re-analyze these components and provide an updated architecture notebook that reflects the current state. Include all cells from the previous notebook, updating the summaries of changed components. Emit a full canopy-notebook fence.`,
        ].join("\n");
      },
    },
  ];
}

// --- Plugin ---

const architecturePlugin: Plugin = {
  id: "architecture",
  label: "Architecture Analysis",

  available(_ctx: PluginContext): boolean {
    return true;
  },

  systemPrompt(ctx: PluginContext): string {
    return chatContext(ctx);
  },

  skills(_ctx: PluginContext): Skill[] {
    return createSkills();
  },
};

export default architecturePlugin;

/**
 * Architecture Plugin (Chat Agent)
 *
 * Provides the chat agent with awareness of the architecture notebook
 * and the ability to propose code changes. The scanning responsibility
 * has moved to the dedicated scanner agent (see scanner.ts).
 *
 * Remaining contributions:
 * - System prompt: codebase context + change proposal format
 * - Skills: propose-changes, rescan-components
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

const CHANGE_PROPOSAL_FORMAT = `## Change Proposals

When the user edits a cell's description and you propose code changes, include a structured change proposal alongside your explanation. Wrap it in a canopy-changes code fence:

\`\`\`canopy-changes
{
  "proposals": [
    {
      "cell_id": "the-cell-id-that-was-edited",
      "summary": "Brief description of what changes are needed",
      "changes": [
        {
          "file_path": "src/path/to/file.ts",
          "description": "What this specific file change does",
          "before": "// optional: the relevant code before the change",
          "after": "// optional: the relevant code after the change"
        }
      ]
    }
  ]
}
\`\`\`

Rules for change proposals:
1. The \`cell_id\` must match an existing cell in the notebook
2. Include concrete file paths and descriptions for each change
3. Use \`before\`/\`after\` snippets to show the key diff — keep them short
4. If the change affects multiple cells, include multiple proposals
5. Always explain your reasoning in conversational text alongside the structured fence`;

// --- Skills ---

function createSkills(): Skill[] {
  return [
    {
      id: "propose-changes",
      label: "Propose Changes",
      prompt(params) {
        const kind = params.kind as string;
        const name = params.name as string;
        const oldSummary = params.oldSummary as string;
        const newSummary = params.newSummary as string;
        const filePaths = params.filePaths as string[] | undefined;

        const fileSection =
          filePaths && filePaths.length > 0
            ? `\nRelevant files: ${filePaths.join(", ")}`
            : "";

        return [
          `The user edited the ${kind} "${name}".`,
          ``,
          `Previous description:`,
          `> ${oldSummary}`,
          ``,
          `New description:`,
          `> ${newSummary}`,
          `${fileSection}`,
          ``,
          `Review the relevant code and propose changes to align the implementation with the updated intent. Explain what you would change and why, then emit a canopy-changes fence with your structured proposal.`,
        ].join("\n");
      },
    },
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
    return [chatContext(ctx), CHANGE_PROPOSAL_FORMAT].join("\n\n");
  },

  skills(_ctx: PluginContext): Skill[] {
    return createSkills();
  },
};

export default architecturePlugin;

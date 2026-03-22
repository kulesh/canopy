/**
 * Architecture Plugin
 *
 * Gives the agent the ability to analyze codebases and present
 * findings as structured C4-style notebooks. Provides:
 *
 * - System prompt: scanning strategy, notebook format, change proposal format
 * - Skills: scan-architecture, propose-changes, rescan-components
 *
 * No tools — the agent uses filesystem tools to explore; this plugin
 * tells it what to look for and how to present what it finds.
 */

import type { Plugin, PluginContext, Skill } from "../plugins.js";

// --- System prompt fragments ---

function scanningStrategy(ctx: PluginContext): string {
  if (!ctx.projectHandle) {
    return `No project directory is currently open. Ask the user to open a project using the folder button in the header bar.`;
  }

  return `You have tools for exploring the codebase:

- **list_directory(path?)**: List files and subdirectories. Start with the root (no path) to see the project structure, then drill into interesting directories.
- **read_file(path)**: Read a file's contents with line numbers. Use this to understand implementation details.

## Scanning Strategy

When asked to analyze architecture:
1. Start with \`list_directory()\` to see the top-level structure
2. Read orientation files first: README, package.json, Cargo.toml, go.mod, etc.
3. List source directories to understand the module layout
4. Read key source files — entry points, module roots, type definitions
5. Don't read every file. Sample representative files from each subsystem.
6. Focus on boundaries: what talks to what, what depends on what
7. Emit a canopy-notebook fence when you have enough signal`;
}

const NOTEBOOK_FORMAT = `## Structured Output

When the user asks you to "show the architecture," "analyze this project," or similar requests for architectural overview, you MUST return a structured notebook alongside your explanation. Wrap the notebook in a canopy-notebook code fence:

\`\`\`canopy-notebook
{
  "cells": [
    {
      "id": "unique-id",
      "kind": "system" | "container" | "component" | "code_unit",
      "name": "Human-Readable Name",
      "summary": "One to three sentences describing what this does, why it exists, and how it fits into the larger system.",
      "children": ["child-id-1", "child-id-2"],
      "dependencies": ["sibling-id-that-this-depends-on"],
      "file_paths": ["src/relevant/path.ts"],
      "provenance": { "source": "ai" }
    }
  ],
  "root_ids": ["top-level-system-id"]
}
\`\`\`

Rules for structured output:
1. Every cell referenced in \`children\` or \`dependencies\` must exist in the \`cells\` array
2. Use kebab-case IDs derived from the component name (e.g., "auth-service", "jwt-validator")
3. Summaries should be human-readable — describe intent and responsibility, not implementation
4. Include file_paths only for concrete components (not for system-level or abstract containers)
5. Start with a system cell, decompose into containers, then components
6. Only go to code_unit depth when the user asks to drill down

Always include conversational text before or after the notebook fence to explain your findings. The notebook is the structured view; the text is the narrative.`;

const CHANGE_PROPOSAL_FORMAT = `## Change Proposals

When the user edits a cell's description and you propose code changes, you MUST include a structured change proposal alongside your explanation. Wrap it in a canopy-changes code fence:

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
3. Use \`before\`/\`after\` snippets to show the key diff — keep them short (relevant lines only, not entire files)
4. If the change affects multiple cells, include multiple proposals
5. Always explain your reasoning in conversational text alongside the structured fence`;

// --- Skills ---

function createSkills(): Skill[] {
  return [
    {
      id: "scan-architecture",
      label: "Scan Architecture",
      prompt(params) {
        const projectName = params.projectName as string;
        return (
          `Analyze the architecture of this project ("${projectName}"). ` +
          `Start by exploring the directory structure and reading key files, ` +
          `then present your findings as a canopy-notebook.`
        );
      },
    },
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
    // Always active — skills are useful even without a project.
    // The system prompt adapts based on whether a project is open.
    return true;
  },

  systemPrompt(ctx: PluginContext): string {
    return [scanningStrategy(ctx), NOTEBOOK_FORMAT, CHANGE_PROPOSAL_FORMAT].join(
      "\n\n",
    );
  },

  skills(_ctx: PluginContext): Skill[] {
    return createSkills();
  },
};

export default architecturePlugin;

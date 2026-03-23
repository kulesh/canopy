/**
 * Propose Changes Tool
 *
 * The proposer agent's structured output contract. When a user edits
 * a cell's description, the proposer agent analyzes relevant code and
 * delivers its change proposals via this tool — not via markdown fences.
 *
 * Same pattern as present_notebook (ADR-014: tool as output contract).
 */

import { Type } from "@sinclair/typebox";
import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import { isValidChangeSet } from "../../notebook/types.js";
import type { ChangeSet } from "../../notebook/types.js";
import type { NotebookStore } from "../../notebook/store.js";

// --- Schema ---

const FileChangeSchema = Type.Object({
  file_path: Type.String({ description: "Path to the file that needs to change" }),
  description: Type.String({ description: "What this specific file change does" }),
  before: Type.Optional(Type.String({ description: "Relevant code before the change (short snippet)" })),
  after: Type.Optional(Type.String({ description: "Relevant code after the change (short snippet)" })),
});

const ChangeProposalSchema = Type.Object({
  cell_id: Type.String({ description: "ID of the notebook cell this proposal is for" }),
  summary: Type.String({ description: "Brief description of what changes are needed" }),
  changes: Type.Array(FileChangeSchema, { description: "Concrete file changes" }),
});

const ProposeChangesSchema = Type.Object({
  proposals: Type.Array(ChangeProposalSchema, {
    description: "One or more change proposals, each linked to a notebook cell.",
  }),
});

// --- Tool factory ---

function textResult(text: string): AgentToolResult<void> {
  return { content: [{ type: "text", text }], details: undefined as void };
}

export function proposeChangesTool(store: NotebookStore): AgentTool<any> {
  return {
    name: "propose_changes",
    description:
      "Present your proposed code changes as structured proposals. " +
      "Call this tool ONCE when you have finished analyzing the code " +
      "and are ready to propose changes. Each proposal must reference " +
      "an existing cell_id from the notebook.",
    label: "Propose Changes",
    parameters: ProposeChangesSchema,
    async execute(
      _toolCallId: string,
      params: { proposals: any[] },
    ): Promise<AgentToolResult<void>> {
      if (!isValidChangeSet(params)) {
        return textResult(
          "Error: Invalid change set. Ensure every proposal has " +
          "cell_id, summary, and a changes array with file_path and description.",
        );
      }

      const changeSet = params as ChangeSet;
      store.loadChanges(changeSet);

      const cellCount = changeSet.proposals.length;
      const fileCount = changeSet.proposals.reduce(
        (sum, p) => sum + p.changes.length, 0,
      );
      return textResult(
        `Changes proposed: ${cellCount} cell(s), ${fileCount} file change(s).`,
      );
    },
  };
}

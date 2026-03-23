/**
 * Present Notebook Tool
 *
 * The scanner agent's structured output contract. Instead of hoping
 * the LLM emits JSON in the right markdown fence, the agent calls
 * this tool to deliver its architecture analysis. The tool validates
 * the schema and loads the notebook directly into the store.
 *
 * This is the "tool as output contract" pattern from ADR-014.
 */

import { Type } from "@sinclair/typebox";
import type { AgentTool, AgentToolResult } from "@mariozechner/pi-agent-core";
import {
  type NotebookWire,
  notebookFromWire,
  isValidNotebookWire,
  normalizeCell,
} from "../../notebook/types.js";
import type { NotebookStore } from "../../notebook/store.js";

// --- Schema ---

const CellSchema = Type.Object({
  id: Type.String({ description: "Unique kebab-case identifier (e.g., 'auth-service')" }),
  kind: Type.Union([
    Type.Literal("system"),
    Type.Literal("container"),
    Type.Literal("component"),
    Type.Literal("code_unit"),
  ], { description: "Abstraction level in the C4 hierarchy" }),
  name: Type.String({ description: "Human-readable name" }),
  summary: Type.String({ description: "1-3 sentence description of intent and responsibility" }),
  children: Type.Array(Type.String(), { description: "IDs of child cells" }),
  dependencies: Type.Array(Type.String(), { description: "IDs of sibling cells this depends on" }),
  file_paths: Type.Array(Type.String(), { description: "Relevant source file paths" }),
});

const PresentNotebookSchema = Type.Object({
  cells: Type.Array(CellSchema, {
    description: "All architecture cells. Every ID in children/dependencies must exist here.",
  }),
  root_ids: Type.Array(Type.String(), {
    description: "IDs of top-level cells (typically one system cell)",
  }),
});

// --- Tool factory ---

function textResult(text: string): AgentToolResult<void> {
  return { content: [{ type: "text", text }], details: undefined as void };
}

export function presentNotebookTool(store: NotebookStore): AgentTool<any> {
  return {
    name: "present_notebook",
    description:
      "Present your architecture analysis as a structured notebook. " +
      "Call this tool ONCE when you have finished exploring the codebase " +
      "and are ready to present your findings. Every cell referenced in " +
      "children or dependencies must exist in the cells array.",
    label: "Present Notebook",
    parameters: PresentNotebookSchema,
    async execute(
      _toolCallId: string,
      params: { cells: any[]; root_ids: string[] },
    ): Promise<AgentToolResult<void>> {
      if (!isValidNotebookWire(params)) {
        return textResult(
          "Error: Invalid notebook structure. Ensure every cell has " +
          "id, kind, name, summary, children, dependencies, and file_paths.",
        );
      }

      const wire: NotebookWire = {
        cells: params.cells.map(normalizeCell),
        root_ids: params.root_ids,
      };

      const notebook = notebookFromWire(wire);
      store.load(notebook);

      return textResult(
        `Notebook loaded: ${notebook.cells.size} cells, ` +
        `${wire.root_ids.length} root(s).`,
      );
    },
  };
}

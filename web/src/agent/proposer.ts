/**
 * Proposer Agent
 *
 * A single-responsibility agent that analyzes code and proposes changes
 * in response to a cell edit. Runs headless — no chat panel, no
 * conversation history. Its only output channel is the `propose_changes`
 * tool, which validates and loads proposals directly into the
 * NotebookStore.
 *
 * Same pattern as the scanner agent (ADR-014).
 */

import { Agent } from "@mariozechner/pi-agent-core";
import type { AgentTool, AgentEvent } from "@mariozechner/pi-agent-core";
import type { Model } from "@mariozechner/pi-ai";
import { createStreamFn, getAppStorage } from "@mariozechner/pi-web-ui";
import type { NotebookStore } from "../notebook/store.js";
import type { CellEdit } from "../notebook/store.js";
import type { NotebookCell } from "../notebook/types.js";
import { proposeChangesTool } from "./tools/propose-changes.js";

// We import the filesystem plugin for read_file access.
import filesystemPlugin from "./plugins/filesystem.js";

// --- System prompt ---

const PROPOSER_SYSTEM_PROMPT = `You are a code change proposer. Your job is to analyze source files and propose concrete changes that align a codebase with updated architectural intent.

## How to Work

1. Read the relevant source files using read_file
2. Understand the current implementation
3. Determine what changes would align the code with the new description
4. Call propose_changes with your structured proposals

## How to Propose

Structure each proposal with:
- **cell_id**: The notebook cell this change is for
- **summary**: Brief description of what needs to change
- **changes**: Array of file-level changes, each with:
  - file_path: Which file to modify
  - description: What the change does
  - before/after: Short code snippets showing the key diff (optional but helpful)

Rules:
- Focus on the minimal set of changes needed
- Each file change should have a clear, specific description
- Use before/after snippets for the most important diffs
- You MUST call propose_changes exactly once when done. Do not output proposals as text.`;

// --- Proposer agent factory ---

export interface ProposerAgentOptions {
  projectHandle: FileSystemDirectoryHandle;
  notebookStore: NotebookStore;
  model: Model<any>;
}

export interface ProposerHandle {
  /** The underlying agent instance. */
  agent: Agent;
  /** Propose changes for a cell edit. Returns when the agent finishes. */
  propose(cell: NotebookCell, edit: CellEdit): Promise<void>;
  /** Abort an in-progress proposal. */
  abort(): void;
  /** Subscribe to agent events (for progress tracking). */
  subscribe(fn: (event: AgentEvent) => void): () => void;
}

export function createProposerAgent(options: ProposerAgentOptions): ProposerHandle {
  const { projectHandle, notebookStore, model } = options;

  // Collect tools: filesystem (read_file only needed, but list_directory is useful for context) + propose_changes
  const ctx = { projectHandle };
  const fsTools: AgentTool<any>[] = filesystemPlugin.tools?.(ctx) ?? [];
  const changesTool = proposeChangesTool(notebookStore);
  const tools = [...fsTools, changesTool];

  const agent = new Agent({
    initialState: {
      systemPrompt: PROPOSER_SYSTEM_PROMPT,
      model,
      thinkingLevel: "off",
      tools,
      messages: [],
    },
    // API key resolution
    getApiKey: async (provider: string) => {
      const key = await getAppStorage().providerKeys.get(provider);
      return key ?? undefined;
    },
    // Proxy support
    streamFn: createStreamFn(async () => {
      const storage = getAppStorage();
      const enabled = await storage.settings.get<boolean>("proxy.enabled");
      return enabled
        ? (await storage.settings.get<string>("proxy.url")) || undefined
        : undefined;
    }),
  });

  return {
    agent,

    async propose(cell: NotebookCell, edit: CellEdit): Promise<void> {
      const fileSection =
        cell.file_paths.length > 0
          ? `\nRelevant files: ${cell.file_paths.join(", ")}`
          : "";

      await agent.prompt(
        [
          `The user edited the ${cell.kind} "${cell.name}".`,
          ``,
          `Previous description:`,
          `> ${edit.oldSummary}`,
          ``,
          `New description:`,
          `> ${edit.newSummary}`,
          `${fileSection}`,
          ``,
          `Review the relevant code and propose changes to align the implementation with the updated intent.`,
        ].join("\n"),
      );
    },

    abort(): void {
      agent.abort();
    },

    subscribe(fn: (event: AgentEvent) => void): () => void {
      return agent.subscribe(fn);
    },
  };
}

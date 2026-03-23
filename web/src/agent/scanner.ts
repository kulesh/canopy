/**
 * Scanner Agent
 *
 * A single-responsibility agent that explores a codebase and presents
 * its architecture as a structured notebook. Runs headless — no chat
 * panel, no conversation history. Its only output channel is the
 * `present_notebook` tool, which validates and loads data directly
 * into the NotebookStore.
 *
 * See ADR-014: Composable Single-Responsibility Agents.
 */

import { Agent } from "@mariozechner/pi-agent-core";
import type { AgentTool, AgentEvent } from "@mariozechner/pi-agent-core";
import type { Model } from "@mariozechner/pi-ai";
import { createStreamFn, getAppStorage } from "@mariozechner/pi-web-ui";
import type { NotebookStore } from "../notebook/store.js";
import { presentNotebookTool } from "./tools/present-notebook.js";

// --- Filesystem tools (extracted from plugin for reuse) ---

// We import the filesystem plugin to get its tools. The plugin
// already encapsulates list_directory and read_file.
import filesystemPlugin from "./plugins/filesystem.js";

// --- System prompt ---

const SCANNER_SYSTEM_PROMPT = `You are a codebase architecture scanner. Your job is to explore a project's file structure and source code, then present your findings as a structured architecture notebook.

## How to Work

1. Start with list_directory() to see the top-level structure
2. Read orientation files: README, package.json, Cargo.toml, go.mod, etc.
3. List source directories to understand the module layout
4. Read key source files — entry points, module roots, type definitions
5. Don't read every file. Sample representative files from each subsystem.
6. Focus on boundaries: what talks to what, what depends on what

## How to Present

When you have enough signal, call present_notebook with your analysis.

Structure your cells as a C4 hierarchy:
- **system**: The top-level project (usually one)
- **container**: Major subsystems (e.g., API layer, database layer)
- **component**: Individual units of functionality within a container
- **code_unit**: Only when specifically relevant

Rules:
- Every cell ID in children/dependencies must exist in the cells array
- Use kebab-case IDs derived from the component name
- Summaries should describe intent and responsibility, not implementation details
- Include file_paths only for concrete components
- Start with one system cell, decompose into containers, then components

You MUST call present_notebook exactly once when done. Do not output the notebook as text.`;

// --- Scanner agent factory ---

export interface ScannerAgentOptions {
  projectHandle: FileSystemDirectoryHandle;
  notebookStore: NotebookStore;
  model: Model<any>;
}

export interface ScannerHandle {
  /** The underlying agent instance. */
  agent: Agent;
  /** Start scanning the project. Returns when the agent finishes. */
  scan(projectName: string): Promise<void>;
  /** Abort an in-progress scan. */
  abort(): void;
  /** Subscribe to agent events (for progress tracking). */
  subscribe(fn: (event: AgentEvent) => void): () => void;
}

export function createScannerAgent(options: ScannerAgentOptions): ScannerHandle {
  const { projectHandle, notebookStore, model } = options;

  // Collect tools: filesystem + present_notebook
  const ctx = { projectHandle };
  const fsTools: AgentTool<any>[] = filesystemPlugin.tools?.(ctx) ?? [];
  const notebookTool = presentNotebookTool(notebookStore);
  const tools = [...fsTools, notebookTool];

  const agent = new Agent({
    initialState: {
      systemPrompt: SCANNER_SYSTEM_PROMPT,
      model,
      thinkingLevel: "off",
      tools,
      messages: [],
    },
    // API key resolution — same mechanism as the chat agent
    getApiKey: async (provider: string) => {
      const key = await getAppStorage().providerKeys.get(provider);
      return key ?? undefined;
    },
    // Proxy support — same mechanism as the chat agent
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

    async scan(projectName: string): Promise<void> {
      await agent.prompt(
        `Analyze the architecture of this project ("${projectName}").`,
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

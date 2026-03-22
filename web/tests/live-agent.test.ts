/**
 * Live agent integration tests — Canopy analyzes its own source tree
 *
 * Phase 2: Agent scans web/src/ and emits a canopy-notebook
 * Phase 3: Agent proposes changes in response to a cell edit
 *
 * Requires ANTHROPIC_API_KEY in the root .env file.
 * Skips gracefully if no key is available.
 *
 * If Node can't reach api.anthropic.com directly (sandboxed
 * environments), a curl-based proxy relays API traffic.
 *
 * Run explicitly:  npm run test:live
 */

import { describe, it, expect, afterAll, beforeAll } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import type { Model } from "@mariozechner/pi-ai";
import { createRegistry, type PluginContext, type PluginRegistry } from "../src/agent/plugins.js";
import { NodeDirectoryHandle } from "./node-fs-handle.js";
import { findLatestNotebook, findLatestChanges } from "../src/notebook/parse.js";
import { NotebookStore } from "../src/notebook/store.js";
import type { Notebook } from "../src/notebook/types.js";
import { startCurlProxy, type CurlProxy } from "./curl-proxy.js";

// --- Load API key from .env into process.env ---

function loadEnv(): string | undefined {
  const envPath = path.resolve(__dirname, "../../.env");
  if (!fs.existsSync(envPath)) return undefined;
  const content = fs.readFileSync(envPath, "utf-8");
  for (const line of content.split("\n")) {
    const match = line.match(/^([A-Z_]+)=(.+)$/);
    if (match) {
      process.env[match[1]] = match[2].trim();
    }
  }
  return process.env.ANTHROPIC_API_KEY;
}

const API_KEY = loadEnv();
const WEB_SRC = path.resolve(__dirname, "../src");

// --- Connectivity check ---

async function canReachApi(): Promise<boolean> {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5_000);
    await fetch("https://api.anthropic.com", { signal: controller.signal });
    clearTimeout(timeout);
    return true;
  } catch {
    return false;
  }
}

// --- Shared agent factory ---

function buildSystemPrompt(registry: PluginRegistry, ctx: PluginContext): string {
  const base = [
    `You are Canopy, an AI assistant that helps developers understand and modify codebases at the architectural level.`,
    ``,
    `When a user points you at a codebase, you analyze its structure and present it as a hierarchy of components with human-readable summaries.`,
    ``,
    `## Architecture Hierarchy`,
    ``,
    `- **System**: The top-level project or service`,
    `- **Container**: Major subsystems`,
    `- **Component**: Individual units of functionality within a container`,
    `- **Code Unit**: Specific functions, classes, or modules (only when the user drills down)`,
  ].join("\n");

  return `${base}\n\n${registry.systemPrompt(ctx)}`;
}

function assertNoAgentError(messages: any[]): void {
  const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant") as any;
  if (lastAssistant?.stopReason === "error") {
    throw new Error(`Agent error: ${lastAssistant.errorMessage ?? "unknown"}`);
  }
}

// --- Tests ---

describe("Live agent: Canopy analyzes itself", () => {
  const run = API_KEY ? it : it.skip;
  let proxy: CurlProxy | null = null;
  let model: Model<any>;
  let registry: PluginRegistry;
  let ctx: PluginContext;

  // Shared state across sequential tests
  let scanNotebook: Notebook;
  let scanMessages: any[];

  beforeAll(async () => {
    registry = createRegistry();
    const handle = new NodeDirectoryHandle(WEB_SRC);
    ctx = { projectHandle: handle as unknown as FileSystemDirectoryHandle };

    model = getModel("anthropic", "claude-sonnet-4-5-20250929");
    const directAccess = await canReachApi();
    if (!directAccess) {
      console.log("[live-agent] Node cannot reach api.anthropic.com — starting curl proxy");
      proxy = await startCurlProxy("api.anthropic.com");
      model.baseUrl = proxy.baseUrl;
      console.log(`[live-agent] Proxy listening at ${proxy.baseUrl}`);
    }
  }, 15_000);

  afterAll(async () => {
    if (proxy) await proxy.stop();
  });

  // =========================================================
  // Phase 2: Architecture scan
  // =========================================================

  run(
    "Phase 2: agent scans web/src/ and emits a valid canopy-notebook",
    async () => {
      const agent = new Agent({
        initialState: {
          systemPrompt: buildSystemPrompt(registry, ctx),
          model,
          thinkingLevel: "off",
          messages: [],
          tools: registry.resolveTools(ctx),
        },
      });

      const scanSkill = registry.skill("scan-architecture", ctx)!;
      await agent.prompt(scanSkill.prompt({ projectName: "canopy-web" }));
      await agent.waitForIdle();

      const messages = agent.state.messages;
      assertNoAgentError(messages);

      // Verify tool usage
      const toolCalls = messages
        .filter((m: any) => m.role === "assistant")
        .flatMap((m: any) => (m.content ?? []).filter((c: any) => c.type === "toolCall"));
      expect(toolCalls.length).toBeGreaterThan(0);

      const listDirCalls = toolCalls.filter((c: any) => c.name === "list_directory");
      expect(listDirCalls.length).toBeGreaterThan(0);

      const readFileCalls = toolCalls.filter((c: any) => c.name === "read_file");
      expect(readFileCalls.length).toBeGreaterThan(0);

      // Extract and validate notebook
      const notebook = findLatestNotebook(messages);
      expect(notebook).not.toBeNull();
      expect(notebook!.cells.size).toBeGreaterThanOrEqual(3);
      expect(notebook!.root_ids.length).toBeGreaterThanOrEqual(1);

      const store = new NotebookStore();
      store.load(notebook!);
      expect(store.empty).toBe(false);

      const root = store.cell(notebook!.root_ids[0])!;
      expect(root.kind).toBe("system");
      expect(root.children.length).toBeGreaterThan(0);

      // Every reference must resolve
      for (const [, cell] of notebook!.cells) {
        for (const childId of cell.children) {
          expect(notebook!.cells.has(childId)).toBe(true);
        }
        for (const depId of cell.dependencies) {
          expect(notebook!.cells.has(depId)).toBe(true);
        }
      }

      // Save for Phase 3
      scanNotebook = notebook!;
      scanMessages = messages;

      // Print summary
      console.log(`\n--- Phase 2: Scanned ${notebook!.cells.size} cells ---`);
      for (const [, cell] of notebook!.cells) {
        const indent = cell.kind === "system" ? "" : cell.kind === "container" ? "  " : "    ";
        console.log(`${indent}[${cell.kind}] ${cell.name}`);
      }
      console.log("--- end ---\n");
    },
    120_000,
  );

  // =========================================================
  // Phase 3: Proposal round-trip
  // =========================================================

  run(
    "Phase 3: agent proposes changes in response to a cell edit",
    async () => {
      // Phase 3 depends on Phase 2's notebook
      expect(scanNotebook).toBeDefined();

      // Pick a component cell with file_paths for a meaningful proposal
      let targetCell = [...scanNotebook.cells.values()].find(
        (c) => c.kind === "component" && c.file_paths.length > 0,
      );
      // Fallback: any component
      if (!targetCell) {
        targetCell = [...scanNotebook.cells.values()].find(
          (c) => c.kind === "component",
        );
      }
      expect(targetCell).toBeDefined();
      console.log(`\n[Phase 3] Editing cell: "${targetCell!.name}" (${targetCell!.id})`);
      console.log(`[Phase 3] Original summary: ${targetCell!.summary}`);

      // Simulate a user edit — change the summary to request a concrete change
      const oldSummary = targetCell!.summary;
      const newSummary = `${oldSummary} Should also expose a health-check endpoint for monitoring.`;

      // Create a fresh agent with the scan conversation as context
      const agent = new Agent({
        initialState: {
          systemPrompt: buildSystemPrompt(registry, ctx),
          model,
          thinkingLevel: "off",
          messages: [...scanMessages],
          tools: registry.resolveTools(ctx),
        },
      });

      // Send the propose-changes skill prompt
      const proposeSkill = registry.skill("propose-changes", ctx)!;
      const prompt = proposeSkill.prompt({
        kind: targetCell!.kind,
        name: targetCell!.name,
        oldSummary,
        newSummary,
        filePaths: targetCell!.file_paths,
      });

      console.log(`[Phase 3] Sending propose-changes prompt...`);
      await agent.prompt(prompt);
      await agent.waitForIdle();

      const messages = agent.state.messages;
      assertNoAgentError(messages);

      // Agent should have emitted a canopy-changes fence
      const changeSet = findLatestChanges(messages);
      expect(changeSet).not.toBeNull();
      expect(changeSet!.proposals.length).toBeGreaterThanOrEqual(1);

      // At least one proposal should target our edited cell
      const targetProposal = changeSet!.proposals.find(
        (p) => p.cell_id === targetCell!.id,
      );
      expect(targetProposal).toBeDefined();
      expect(targetProposal!.summary.length).toBeGreaterThan(0);
      expect(targetProposal!.changes.length).toBeGreaterThan(0);

      // Each change should have a file path and description
      for (const change of targetProposal!.changes) {
        expect(change.file_path.length).toBeGreaterThan(0);
        expect(change.description.length).toBeGreaterThan(0);
      }

      // Load into store — the full lifecycle
      const store = new NotebookStore();
      store.load(scanNotebook);
      store.loadChanges(changeSet!);

      expect(store.hasChanges).toBe(true);
      const storedChange = store.changeFor(targetCell!.id);
      expect(storedChange).toBeDefined();
      expect(storedChange!.summary).toBe(targetProposal!.summary);

      // Dismiss and verify lifecycle
      store.dismissChange(targetCell!.id);
      expect(store.changeFor(targetCell!.id)).toBeUndefined();

      // Print results
      console.log(`[Phase 3] Proposal received for "${targetCell!.name}":`);
      console.log(`  Summary: ${targetProposal!.summary}`);
      for (const change of targetProposal!.changes) {
        console.log(`  File: ${change.file_path} — ${change.description}`);
        if (change.before) console.log(`    - ${change.before.substring(0, 80)}`);
        if (change.after) console.log(`    + ${change.after.substring(0, 80)}`);
      }
      console.log("--- Phase 3 complete ---\n");
    },
    120_000,
  );
});

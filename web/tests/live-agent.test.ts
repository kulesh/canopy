/**
 * Live agent integration test — Canopy scans its own source tree
 *
 * This test creates a real Pi SDK agent with the Anthropic API,
 * points it at Canopy's own web/src/ via NodeDirectoryHandle,
 * and verifies it produces a valid canopy-notebook.
 *
 * Requires ANTHROPIC_API_KEY in the root .env file.
 * Skips gracefully if no key is available.
 *
 * If Node can't reach api.anthropic.com directly (sandboxed
 * environments), a curl-based proxy relays API traffic.
 *
 * Run explicitly:  npm run test:live
 */

import { describe, it, expect, afterAll } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import { createRegistry, type PluginContext } from "../src/agent/plugins.js";
import { NodeDirectoryHandle } from "./node-fs-handle.js";
import { findLatestNotebook } from "../src/notebook/parse.js";
import { NotebookStore } from "../src/notebook/store.js";
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

// --- Test ---

describe("Live agent: Canopy scans itself", () => {
  const run = API_KEY ? it : it.skip;
  let proxy: CurlProxy | null = null;

  afterAll(async () => {
    if (proxy) await proxy.stop();
  });

  run(
    "agent analyzes web/src/ and emits a valid canopy-notebook",
    async () => {
      const registry = createRegistry();
      const handle = new NodeDirectoryHandle(WEB_SRC);
      const ctx: PluginContext = {
        projectHandle: handle as unknown as FileSystemDirectoryHandle,
      };

      // Build the same system prompt the PWA would use
      const basePrompt = [
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

      const systemPrompt = `${basePrompt}\n\n${registry.systemPrompt(ctx)}`;
      const tools = registry.resolveTools(ctx);

      // Configure model — use curl proxy if Node can't reach the API directly
      const model = getModel("anthropic", "claude-sonnet-4-5-20250929");
      const directAccess = await canReachApi();
      if (!directAccess) {
        console.log("[live-agent] Node cannot reach api.anthropic.com — starting curl proxy");
        proxy = await startCurlProxy("api.anthropic.com");
        model.baseUrl = proxy.baseUrl;
        console.log(`[live-agent] Proxy listening at ${proxy.baseUrl}`);
      }

      const agent = new Agent({
        initialState: {
          systemPrompt,
          model,
          thinkingLevel: "off",
          messages: [],
          tools,
        },
      });

      // Send the scan prompt via the skill
      const scanSkill = registry.skill("scan-architecture", ctx)!;
      await agent.prompt(scanSkill.prompt({ projectName: "canopy-web" }));

      // Wait for idle (all tool calls complete, final response ready)
      await agent.waitForIdle();

      const messages = agent.state.messages;

      // Check for API errors (network, auth, etc.)
      const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant") as any;
      if (lastAssistant?.stopReason === "error") {
        throw new Error(`Agent error: ${lastAssistant.errorMessage ?? "unknown"}`);
      }

      // Extract tool call blocks from assistant messages
      const toolCalls = messages
        .filter((m) => m.role === "assistant")
        .flatMap((m: any) =>
          (m.content ?? []).filter((c: any) => c.type === "toolCall"),
        );
      expect(toolCalls.length).toBeGreaterThan(0);

      // Agent should have called list_directory at least once
      const listDirCalls = toolCalls.filter(
        (c: any) => c.name === "list_directory",
      );
      expect(listDirCalls.length).toBeGreaterThan(0);

      // Agent should have called read_file at least once
      const readFileCalls = toolCalls.filter(
        (c: any) => c.name === "read_file",
      );
      expect(readFileCalls.length).toBeGreaterThan(0);

      // Tool results should exist
      const toolResults = messages.filter((m) => m.role === "toolResult");
      expect(toolResults.length).toBeGreaterThan(0);

      // Extract notebook from messages
      const notebook = findLatestNotebook(messages);
      expect(notebook).not.toBeNull();
      expect(notebook!.cells.size).toBeGreaterThanOrEqual(3);
      expect(notebook!.root_ids.length).toBeGreaterThanOrEqual(1);

      // Load into store — must work
      const store = new NotebookStore();
      store.load(notebook!);
      expect(store.empty).toBe(false);

      // Root cell should be a system
      const root = store.cell(notebook!.root_ids[0])!;
      expect(root.kind).toBe("system");
      expect(root.children.length).toBeGreaterThan(0);

      // Every child reference must resolve
      for (const [, cell] of notebook!.cells) {
        for (const childId of cell.children) {
          expect(notebook!.cells.has(childId)).toBe(true);
        }
        for (const depId of cell.dependencies) {
          expect(notebook!.cells.has(depId)).toBe(true);
        }
      }

      // Print summary for human review
      console.log("\n--- Canopy self-scan results ---");
      console.log(`Cells: ${notebook!.cells.size}`);
      console.log(`Root: ${root.name}`);
      for (const [, cell] of notebook!.cells) {
        const indent = cell.kind === "system" ? "" : cell.kind === "container" ? "  " : "    ";
        console.log(`${indent}[${cell.kind}] ${cell.name}: ${cell.summary}`);
      }
      console.log("--- end ---\n");
    },
    120_000, // 2 minute timeout — real API call with multiple tool rounds
  );
});

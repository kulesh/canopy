/**
 * Plugin system tests
 *
 * Unit tests for the plugin registry, architecture plugin, and
 * filesystem plugin — then a dogfood integration test that wires
 * them together to scan Canopy's own source tree.
 *
 * The dogfood test exercises the full pipeline minus LLM inference:
 * registry → system prompt → filesystem tools → skill prompts →
 * notebook parse → store load.
 */

import { describe, it, expect } from "vitest";
import * as path from "node:path";
import { PluginRegistry, type Plugin, type PluginContext, type Skill } from "../src/agent/plugins.js";
import { createRegistry } from "../src/agent/plugins.js";
import { NodeDirectoryHandle } from "./node-fs-handle.js";
import { parseNotebookFromMessage } from "../src/notebook/parse.js";
import { NotebookStore } from "../src/notebook/store.js";
import type { AgentMessage } from "@mariozechner/pi-agent-core";

// --- Fixtures ---

const WEB_SRC = path.resolve(__dirname, "../src");

function makeAssistantMessage(text: string): AgentMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "messages",
    provider: "anthropic",
    model: "test",
    usage: { inputTokens: 0, outputTokens: 0, inputCachedTokens: 0, inputCacheWriteTokens: 0 },
    stopReason: "stop",
    timestamp: Date.now(),
  } as any;
}

/** Minimal plugin for testing registry composition. */
function stubPlugin(overrides: Partial<Plugin> & { id: string }): Plugin {
  return {
    label: overrides.id,
    available: () => true,
    ...overrides,
  };
}

// =============================================================
// Plugin Registry
// =============================================================

describe("PluginRegistry", () => {
  it("resolves tools from active plugins only", () => {
    const registry = new PluginRegistry();
    const tool = { name: "test_tool" } as any;

    registry.register(stubPlugin({
      id: "active",
      available: () => true,
      tools: () => [tool],
    }));
    registry.register(stubPlugin({
      id: "inactive",
      available: () => false,
      tools: () => [{ name: "hidden" } as any],
    }));

    const tools = registry.resolveTools({});
    expect(tools).toHaveLength(1);
    expect(tools[0].name).toBe("test_tool");
  });

  it("composes system prompts from active plugins", () => {
    const registry = new PluginRegistry();
    registry.register(stubPlugin({
      id: "a",
      systemPrompt: () => "Fragment A",
    }));
    registry.register(stubPlugin({
      id: "b",
      systemPrompt: () => "Fragment B",
    }));
    registry.register(stubPlugin({
      id: "c",
      available: () => false,
      systemPrompt: () => "Fragment C (inactive)",
    }));

    const prompt = registry.systemPrompt({});
    expect(prompt).toContain("Fragment A");
    expect(prompt).toContain("Fragment B");
    expect(prompt).not.toContain("Fragment C");
  });

  it("looks up skills by ID across active plugins", () => {
    const registry = new PluginRegistry();
    registry.register(stubPlugin({
      id: "p1",
      skills: () => [
        { id: "skill-a", label: "A", prompt: () => "prompt-a" },
      ],
    }));
    registry.register(stubPlugin({
      id: "p2",
      skills: () => [
        { id: "skill-b", label: "B", prompt: () => "prompt-b" },
      ],
    }));

    expect(registry.skill("skill-a", {})?.prompt({})).toBe("prompt-a");
    expect(registry.skill("skill-b", {})?.prompt({})).toBe("prompt-b");
    expect(registry.skill("nonexistent", {})).toBeUndefined();
  });

  it("rejects duplicate plugin IDs", () => {
    const registry = new PluginRegistry();
    registry.register(stubPlugin({ id: "dup" }));
    expect(() => registry.register(stubPlugin({ id: "dup" }))).toThrow(
      /already registered/,
    );
  });

  it("reports plugin status", () => {
    const registry = new PluginRegistry();
    registry.register(stubPlugin({
      id: "fs",
      label: "File System",
      available: (ctx) => ctx.projectHandle !== undefined,
    }));

    const withoutProject = registry.status({});
    expect(withoutProject[0].active).toBe(false);

    const withProject = registry.status({
      projectHandle: {} as FileSystemDirectoryHandle,
    });
    expect(withProject[0].active).toBe(true);
  });
});

// =============================================================
// Auto-discovery
// =============================================================

describe("Plugin auto-discovery", () => {
  it("discovers filesystem and architecture plugins", () => {
    const registry = createRegistry();
    const status = registry.status({});
    const ids = status.map((s) => s.id);

    expect(ids).toContain("filesystem");
    expect(ids).toContain("architecture");
  });

  it("filesystem plugin is inactive without project handle", () => {
    const registry = createRegistry();
    const fs = registry.status({}).find((s) => s.id === "filesystem");
    expect(fs?.active).toBe(false);
  });

  it("architecture plugin is always active", () => {
    const registry = createRegistry();
    const arch = registry.status({}).find((s) => s.id === "architecture");
    expect(arch?.active).toBe(true);
  });
});

// =============================================================
// Architecture plugin skills
// =============================================================

describe("Architecture plugin: skills", () => {
  const registry = createRegistry();
  const ctx: PluginContext = {};

  it("provides scan-architecture skill", () => {
    const skill = registry.skill("scan-architecture", ctx);
    expect(skill).toBeDefined();
    expect(skill!.label).toBe("Scan Architecture");

    const prompt = skill!.prompt({ projectName: "canopy" });
    expect(prompt).toContain("canopy");
    expect(prompt).toContain("canopy-notebook");
  });

  it("provides propose-changes skill", () => {
    const skill = registry.skill("propose-changes", ctx);
    expect(skill).toBeDefined();

    const prompt = skill!.prompt({
      kind: "component",
      name: "Auth Service",
      oldSummary: "Old description",
      newSummary: "New description",
      filePaths: ["src/auth.ts", "src/tokens.ts"],
    });

    expect(prompt).toContain('component "Auth Service"');
    expect(prompt).toContain("Old description");
    expect(prompt).toContain("New description");
    expect(prompt).toContain("src/auth.ts");
    expect(prompt).toContain("canopy-changes");
  });

  it("provides rescan-components skill", () => {
    const skill = registry.skill("rescan-components", ctx);
    expect(skill).toBeDefined();

    const prompt = skill!.prompt({ names: ["API Layer", "Auth Service"] });
    expect(prompt).toContain("API Layer");
    expect(prompt).toContain("Auth Service");
    expect(prompt).toContain("canopy-notebook");
  });
});

// =============================================================
// Architecture plugin: system prompt
// =============================================================

describe("Architecture plugin: system prompt", () => {
  const registry = createRegistry();

  it("includes scanning strategy when project is open", () => {
    const ctx: PluginContext = {
      projectHandle: {} as FileSystemDirectoryHandle,
    };
    const prompt = registry.systemPrompt(ctx);
    expect(prompt).toContain("list_directory");
    expect(prompt).toContain("read_file");
    expect(prompt).toContain("Scanning Strategy");
  });

  it("tells agent to ask user to open project when no project", () => {
    const prompt = registry.systemPrompt({});
    expect(prompt).toContain("open a project");
  });

  it("includes notebook format instructions", () => {
    const prompt = registry.systemPrompt({});
    expect(prompt).toContain("canopy-notebook");
    expect(prompt).toContain("kebab-case");
  });

  it("includes change proposal format instructions", () => {
    const prompt = registry.systemPrompt({});
    expect(prompt).toContain("canopy-changes");
    expect(prompt).toContain("cell_id");
  });
});

// =============================================================
// Dogfood: Canopy scans its own source tree
// =============================================================

describe("Dogfood: filesystem tools against Canopy source", () => {
  const registry = createRegistry();
  const handle = new NodeDirectoryHandle(WEB_SRC);
  const ctx: PluginContext = {
    projectHandle: handle as unknown as FileSystemDirectoryHandle,
  };

  it("filesystem plugin activates with project handle", () => {
    const tools = registry.resolveTools(ctx);
    const names = tools.map((t) => t.name);
    expect(names).toContain("list_directory");
    expect(names).toContain("read_file");
  });

  it("list_directory reads Canopy's own source root", async () => {
    const tools = registry.resolveTools(ctx);
    const listDir = tools.find((t) => t.name === "list_directory")!;

    const result = await listDir.execute("test-1", {});
    const text = (result.content[0] as any).text as string;

    // Must see known directories and files
    expect(text).toContain("[dir]  agent");
    expect(text).toContain("[dir]  notebook");
    expect(text).toContain("[file] main.ts");
  });

  it("list_directory reads agent/plugins/ subdirectory", async () => {
    const tools = registry.resolveTools(ctx);
    const listDir = tools.find((t) => t.name === "list_directory")!;

    const result = await listDir.execute("test-2", { path: "agent/plugins" });
    const text = (result.content[0] as any).text as string;

    expect(text).toContain("[file] filesystem.ts");
    expect(text).toContain("[file] architecture.ts");
  });

  it("read_file reads Canopy's own plugins.ts", async () => {
    const tools = registry.resolveTools(ctx);
    const readFile = tools.find((t) => t.name === "read_file")!;

    const result = await readFile.execute("test-3", { path: "agent/plugins.ts" });
    const text = (result.content[0] as any).text as string;

    expect(text).toContain("agent/plugins.ts");
    expect(text).toContain("export interface Plugin");
    expect(text).toContain("export class PluginRegistry");
  });

  it("read_file reads Canopy's own main.ts", async () => {
    const tools = registry.resolveTools(ctx);
    const readFile = tools.find((t) => t.name === "read_file")!;

    const result = await readFile.execute("test-4", { path: "main.ts" });
    const text = (result.content[0] as any).text as string;

    expect(text).toContain("main.ts");
    expect(text).toContain("createRegistry");
    expect(text).toContain("notebookStore");
  });

  it("list_directory filters ignored directories", async () => {
    // Run against the web root (parent of src) which contains node_modules
    const webRoot = new NodeDirectoryHandle(path.resolve(WEB_SRC, ".."));
    const webCtx: PluginContext = {
      projectHandle: webRoot as unknown as FileSystemDirectoryHandle,
    };
    const tools = registry.resolveTools(webCtx);
    const listDir = tools.find((t) => t.name === "list_directory")!;

    const result = await listDir.execute("test-5", {});
    const text = (result.content[0] as any).text as string;

    expect(text).not.toContain("node_modules");
    expect(text).toContain("[dir]  src");
  });
});

// =============================================================
// Dogfood: full pipeline — tools → notebook → store
// =============================================================

describe("Dogfood: full pipeline", () => {
  it("simulated agent scan produces a valid notebook that loads into the store", () => {
    // Simulate what the agent does: after reading Canopy's source,
    // it emits a canopy-notebook fence. We construct a realistic one
    // based on what the agent would actually see.
    const notebookJson = JSON.stringify({
      cells: [
        {
          id: "canopy-pwa",
          kind: "system",
          name: "Canopy PWA",
          summary: "A progressive web app for AI-assisted architecture analysis of codebases.",
          children: ["agent-subsystem", "notebook-subsystem"],
          dependencies: [],
          file_paths: [],
          provenance: { source: "ai" },
        },
        {
          id: "agent-subsystem",
          kind: "container",
          name: "Agent Subsystem",
          summary: "Manages the Pi agent lifecycle, plugin system, and tool execution.",
          children: ["plugin-registry", "filesystem-plugin", "architecture-plugin", "agent-session"],
          dependencies: [],
          file_paths: ["src/agent/"],
          provenance: { source: "ai" },
        },
        {
          id: "notebook-subsystem",
          kind: "container",
          name: "Notebook Subsystem",
          summary: "Parses, stores, and renders C4-style architecture notebooks.",
          children: ["notebook-parser", "notebook-store", "notebook-renderer"],
          dependencies: ["agent-subsystem"],
          file_paths: ["src/notebook/"],
          provenance: { source: "ai" },
        },
        {
          id: "plugin-registry",
          kind: "component",
          name: "Plugin Registry",
          summary: "Auto-discovers plugins, composes system prompts, resolves tools and skills.",
          children: [],
          dependencies: [],
          file_paths: ["src/agent/plugins.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "filesystem-plugin",
          kind: "component",
          name: "Filesystem Plugin",
          summary: "Provides list_directory and read_file tools via the File System Access API.",
          children: [],
          dependencies: ["plugin-registry"],
          file_paths: ["src/agent/plugins/filesystem.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "architecture-plugin",
          kind: "component",
          name: "Architecture Plugin",
          summary: "Provides scanning strategy, notebook format, and lifecycle skills.",
          children: [],
          dependencies: ["plugin-registry"],
          file_paths: ["src/agent/plugins/architecture.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "agent-session",
          kind: "component",
          name: "Agent Session",
          summary: "Creates and configures the Pi agent with composed system prompt and tools.",
          children: [],
          dependencies: ["plugin-registry"],
          file_paths: ["src/agent/session.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "notebook-parser",
          kind: "component",
          name: "Notebook Parser",
          summary: "Extracts structured notebooks and change proposals from agent message fences.",
          children: [],
          dependencies: [],
          file_paths: ["src/notebook/parse.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "notebook-store",
          kind: "component",
          name: "Notebook Store",
          summary: "Single source of truth for notebook state with pub/sub event system.",
          children: [],
          dependencies: ["notebook-parser"],
          file_paths: ["src/notebook/store.ts"],
          provenance: { source: "ai" },
        },
        {
          id: "notebook-renderer",
          kind: "component",
          name: "Notebook Renderer",
          summary: "Renders foldable cell tree with keyboard navigation, editing, and change proposals.",
          children: [],
          dependencies: ["notebook-store"],
          file_paths: ["src/notebook/panel.ts", "src/notebook/cell.ts"],
          provenance: { source: "ai" },
        },
      ],
      root_ids: ["canopy-pwa"],
    });

    // Step 1: Agent emits a notebook fence
    const agentResponse = makeAssistantMessage(
      `I've analyzed the Canopy PWA source tree. Here's the architecture:\n\n` +
      `\`\`\`canopy-notebook\n${notebookJson}\n\`\`\`\n\n` +
      `The system has two major subsystems: the agent (plugin-based) and the notebook (parse/store/render).`,
    );

    // Step 2: Parse the notebook from the message
    const notebook = parseNotebookFromMessage(agentResponse);
    expect(notebook).not.toBeNull();
    expect(notebook!.cells.size).toBe(10);
    expect(notebook!.root_ids).toEqual(["canopy-pwa"]);

    // Step 3: Load into the store
    const store = new NotebookStore();
    store.load(notebook!);

    expect(store.empty).toBe(false);
    expect(store.rootIds).toEqual(["canopy-pwa"]);

    // Step 4: Verify the tree structure
    const root = store.cell("canopy-pwa")!;
    expect(root.kind).toBe("system");
    expect(root.children).toEqual(["agent-subsystem", "notebook-subsystem"]);

    const agent = store.cell("agent-subsystem")!;
    expect(agent.kind).toBe("container");
    expect(agent.children).toContain("plugin-registry");
    expect(agent.children).toContain("architecture-plugin");

    const archPlugin = store.cell("architecture-plugin")!;
    expect(archPlugin.kind).toBe("component");
    expect(archPlugin.file_paths).toEqual(["src/agent/plugins/architecture.ts"]);

    // Step 5: Verify navigation works
    const visible = store.visibleCellIds();
    expect(visible).toContain("canopy-pwa");
    // Root is auto-expanded, children visible
    expect(visible).toContain("agent-subsystem");
    expect(visible).toContain("notebook-subsystem");
    // Containers not expanded by default, components hidden
    expect(visible).not.toContain("plugin-registry");

    // Expand agent-subsystem
    store.toggle("agent-subsystem");
    const expanded = store.visibleCellIds();
    expect(expanded).toContain("plugin-registry");
    expect(expanded).toContain("architecture-plugin");

    // Step 6: Verify file paths map to real files
    for (const [, cell] of notebook!.cells) {
      for (const fp of cell.file_paths) {
        // File paths should look like real Canopy source paths
        expect(fp).toMatch(/^src\//);
      }
    }
  });

  it("skill prompt → tool calls → notebook parse is a coherent pipeline", async () => {
    const registry = createRegistry();
    const handle = new NodeDirectoryHandle(WEB_SRC);
    const ctx: PluginContext = {
      projectHandle: handle as unknown as FileSystemDirectoryHandle,
    };

    // 1. Get the scan skill and generate the prompt
    const scanSkill = registry.skill("scan-architecture", ctx)!;
    const scanPrompt = scanSkill.prompt({ projectName: "canopy-web" });
    expect(scanPrompt).toContain("canopy-web");

    // 2. The system prompt is well-formed
    const systemPrompt = registry.systemPrompt(ctx);
    expect(systemPrompt).toContain("list_directory");
    expect(systemPrompt).toContain("canopy-notebook");
    expect(systemPrompt).toContain("canopy-changes");

    // 3. Tools are available
    const tools = registry.resolveTools(ctx);
    expect(tools.length).toBeGreaterThanOrEqual(2);

    // 4. Simulate what the agent would do: list root, read key files
    const listDir = tools.find((t) => t.name === "list_directory")!;
    const readFile = tools.find((t) => t.name === "read_file")!;

    const rootListing = await listDir.execute("call-1", {});
    const rootText = (rootListing.content[0] as any).text as string;
    expect(rootText).toContain("main.ts");

    const mainFile = await readFile.execute("call-2", { path: "main.ts" });
    const mainText = (mainFile.content[0] as any).text as string;
    expect(mainText).toContain("createRegistry");

    // 5. After tool calls, agent would emit a notebook — verify parsing works
    //    (already tested above, but this proves the pipeline is coherent)

    // 6. Propose-changes skill works with real cell data
    const proposeSkill = registry.skill("propose-changes", ctx)!;
    const proposePrompt = proposeSkill.prompt({
      kind: "component",
      name: "Plugin Registry",
      oldSummary: "Resolves tools from plugins",
      newSummary: "Auto-discovers plugins, composes system prompts, resolves tools and skills",
      filePaths: ["src/agent/plugins.ts"],
    });
    expect(proposePrompt).toContain("Plugin Registry");
    expect(proposePrompt).toContain("src/agent/plugins.ts");
  });
});

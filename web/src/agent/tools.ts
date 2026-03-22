/**
 * Tool Plugin System
 *
 * Tools are the agent's hands. A ToolPlugin is a factory that produces
 * AgentTools given a runtime context. Plugins declare what they need
 * (e.g., a project directory handle) and the registry activates them
 * when that context becomes available.
 *
 * The registry is the single place where tools are assembled. When
 * context changes — user opens a project, grants a permission — the
 * registry re-evaluates all plugins and rebuilds the tool set.
 */

import type { AgentTool } from "@mariozechner/pi-agent-core";

/**
 * Runtime context available to tool plugins.
 * Grows as the app gains capabilities — each new field unlocks
 * a new class of plugins without changing existing ones.
 */
export interface ToolContext {
  projectHandle?: FileSystemDirectoryHandle;
}

/**
 * A tool plugin is a named factory that produces AgentTools.
 *
 * - `available()` gates activation: if the plugin's prerequisites
 *   aren't met (e.g., no project directory), it stays dormant.
 * - `createTools()` builds the actual AgentTool instances.
 *   Called only when `available()` returns true.
 */
export interface ToolPlugin {
  /** Unique identifier (e.g., "filesystem", "git"). */
  id: string;
  /** Human-readable label for UI display. */
  label: string;
  /** Can this plugin activate in the current context? */
  available(ctx: ToolContext): boolean;
  /** Produce tools for the current context. */
  createTools(ctx: ToolContext): AgentTool[];
}

/**
 * Central registry for tool plugins.
 *
 * Plugins register once. The registry resolves the active tool set
 * whenever context changes, returning a flat array of AgentTools
 * ready to pass to `agent.setTools()`.
 */
export class ToolRegistry {
  private plugins: ToolPlugin[] = [];

  register(plugin: ToolPlugin): void {
    if (this.plugins.some((p) => p.id === plugin.id)) {
      throw new Error(`Tool plugin "${plugin.id}" already registered`);
    }
    this.plugins.push(plugin);
  }

  /** Resolve all active tools for the given context. */
  resolve(ctx: ToolContext): AgentTool[] {
    return this.plugins
      .filter((p) => p.available(ctx))
      .flatMap((p) => p.createTools(ctx));
  }

  /** List registered plugins with their activation status. */
  status(ctx: ToolContext): Array<{ id: string; label: string; active: boolean }> {
    return this.plugins.map((p) => ({
      id: p.id,
      label: p.label,
      active: p.available(ctx),
    }));
  }
}

/**
 * Create a registry with all plugins auto-discovered from `./plugins/`.
 *
 * Convention: each file in `plugins/` exports a default ToolPlugin.
 * Drop a file in, it's registered. Remove it, it's gone. No wiring code.
 */
export function createRegistry(): ToolRegistry {
  const registry = new ToolRegistry();
  const modules = import.meta.glob<{ default: ToolPlugin }>(
    "./plugins/*.ts",
    { eager: true },
  );

  for (const [path, mod] of Object.entries(modules)) {
    if (!mod.default) {
      console.warn(`Plugin at ${path} has no default export — skipping`);
      continue;
    }
    registry.register(mod.default);
  }

  return registry;
}

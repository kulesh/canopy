/**
 * Plugin System
 *
 * A Plugin is a self-contained unit of agent capability. Plugins can
 * contribute any combination of:
 *
 * - **Tools**: functions the agent can call (e.g., list_directory, read_file)
 * - **System prompt**: instructions appended to the agent's base prompt
 * - **Skills**: named prompt templates triggered by lifecycle events
 *
 * The registry auto-discovers plugins from `./plugins/` and composes
 * their contributions into a coherent agent configuration.
 */

import type { AgentTool } from "@mariozechner/pi-agent-core";

/**
 * Runtime context available to plugins.
 * Grows as the app gains capabilities — each new field unlocks
 * a new class of plugins without changing existing ones.
 */
export interface PluginContext {
  projectHandle?: FileSystemDirectoryHandle;
}

/**
 * A named prompt template, triggered by lifecycle events.
 *
 * Skills are the plugin's way of saying "when X happens, here's
 * what to tell the agent." The registry exposes skills by ID;
 * the orchestrator (main.ts) decides when to fire them.
 */
export interface Skill {
  /** Unique identifier (e.g., "scan-architecture"). */
  id: string;
  /** Human-readable label for UI display. */
  label: string;
  /** Build the prompt string from event-specific parameters. */
  prompt(params: Record<string, unknown>): string;
}

/**
 * A plugin is a named, context-gated factory for agent capabilities.
 *
 * - `available()` gates activation on runtime context
 * - `tools()` provides callable tools for the agent
 * - `systemPrompt()` provides instructions appended to the base prompt
 * - `skills()` provides named prompt templates for lifecycle events
 *
 * All capability methods are optional — a plugin can provide any
 * combination. A filesystem plugin provides only tools. An
 * architecture plugin provides a system prompt and skills.
 */
export interface Plugin {
  /** Unique identifier (e.g., "filesystem", "architecture"). */
  id: string;
  /** Human-readable label for UI display. */
  label: string;
  /** Can this plugin activate in the current context? */
  available(ctx: PluginContext): boolean;
  /** Produce tools for the current context. */
  tools?(ctx: PluginContext): AgentTool[];
  /** System prompt fragment appended to the base prompt. */
  systemPrompt?(ctx: PluginContext): string;
  /** Named prompt templates for lifecycle events. */
  skills?(ctx: PluginContext): Skill[];
}

/**
 * Central registry for plugins.
 *
 * Plugins register once. The registry resolves the active tool set,
 * composes system prompts, and exposes skills — all scoped to the
 * current runtime context.
 */
export class PluginRegistry {
  private plugins: Plugin[] = [];

  register(plugin: Plugin): void {
    if (this.plugins.some((p) => p.id === plugin.id)) {
      throw new Error(`Plugin "${plugin.id}" already registered`);
    }
    this.plugins.push(plugin);
  }

  /** Resolve all active tools for the given context. */
  resolveTools(ctx: PluginContext): AgentTool[] {
    return this.plugins
      .filter((p) => p.available(ctx))
      .flatMap((p) => p.tools?.(ctx) ?? []);
  }

  /** Compose system prompt fragments from all active plugins. */
  systemPrompt(ctx: PluginContext): string {
    return this.plugins
      .filter((p) => p.available(ctx) && p.systemPrompt)
      .map((p) => p.systemPrompt!(ctx))
      .join("\n\n");
  }

  /** Look up a skill by ID across all active plugins. */
  skill(id: string, ctx: PluginContext): Skill | undefined {
    for (const plugin of this.plugins) {
      if (!plugin.available(ctx) || !plugin.skills) continue;
      const found = plugin.skills(ctx).find((s) => s.id === id);
      if (found) return found;
    }
    return undefined;
  }

  /** List registered plugins with their activation status. */
  status(ctx: PluginContext): Array<{ id: string; label: string; active: boolean }> {
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
 * Convention: each file in `plugins/` exports a default Plugin.
 * Drop a file in, it's registered. Remove it, it's gone. No wiring code.
 */
export function createRegistry(): PluginRegistry {
  const registry = new PluginRegistry();
  const modules = import.meta.glob<{ default: Plugin }>(
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

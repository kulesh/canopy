import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  ApiKeyPromptDialog,
  type ChatPanel,
} from "@mariozechner/pi-web-ui";
import type { PluginRegistry, PluginContext } from "./plugins.js";

// --- System prompt ---

/**
 * Base identity and hierarchy rules. Plugin-specific instructions
 * (scanning strategy, notebook format, change proposals) are
 * composed by the registry from active plugins.
 */
function buildSystemPrompt(ctx: PluginContext, registry?: PluginRegistry): string {
  const pluginPrompt = registry ? registry.systemPrompt(ctx) : "";

  const base = `You are Canopy, an AI assistant that helps developers understand and modify codebases at the architectural level.

When a user points you at a codebase, you analyze its structure and present it as a hierarchy of components with human-readable summaries — not as files and lines, but as systems, containers, and components with clear responsibilities and relationships.

## Architecture Hierarchy

- **System**: The top-level project or service
- **Container**: Major subsystems (e.g., API layer, database layer, auth module)
- **Component**: Individual units of functionality within a container
- **Code Unit**: Specific functions, classes, or modules (only when the user drills down)`;

  return pluginPrompt ? `${base}\n\n${pluginPrompt}` : base;
}

// --- Agent factory ---

export interface CreateAgentOptions {
  chatPanel: ChatPanel;
  registry?: PluginRegistry;
  toolContext?: PluginContext;
  initialMessages?: any[];
}

export async function createCanopyAgent(
  options: CreateAgentOptions,
): Promise<Agent> {
  const { chatPanel, registry, toolContext, initialMessages } = options;
  const ctx = toolContext ?? {};
  const tools = registry ? registry.resolveTools(ctx) : [];

  const agent = new Agent({
    initialState: {
      systemPrompt: buildSystemPrompt(ctx, registry),
      model: getModel("anthropic", "claude-sonnet-4-5-20250929"),
      thinkingLevel: "off",
      messages: initialMessages ?? [],
      tools,
    },
  });

  await chatPanel.setAgent(agent, {
    onApiKeyRequired: async (provider: string) => {
      return await ApiKeyPromptDialog.prompt(provider);
    },
  });

  return agent;
}

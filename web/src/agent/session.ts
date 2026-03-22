import { Agent } from "@mariozechner/pi-agent-core";
import type { Model } from "@mariozechner/pi-ai";
import { getModel } from "@mariozechner/pi-ai";
import {
  ApiKeyPromptDialog,
  type ChatPanel,
  type SettingsStore,
} from "@mariozechner/pi-web-ui";
import type { PluginRegistry, PluginContext } from "./plugins.js";

// --- Default model ---

const FALLBACK_PROVIDER = "anthropic";
const FALLBACK_MODEL_ID = "claude-sonnet-4-5-20250929";

interface ModelPreference {
  provider: string;
  modelId: string;
}

/** Persist the user's model choice so new sessions start with it. */
export async function saveModelPreference(
  settings: SettingsStore,
  model: Model<any>,
): Promise<void> {
  await settings.set<ModelPreference>("model.default", {
    provider: model.provider,
    modelId: model.id,
  });
}

/** Resolve the user's preferred model, falling back to the built-in default. */
export async function resolveDefaultModel(
  settings: SettingsStore,
): Promise<Model<any>> {
  const pref = await settings.get<ModelPreference>("model.default");
  if (pref) {
    const model = (getModel as (p: string, id: string) => Model<any> | undefined)(
      pref.provider,
      pref.modelId,
    );
    if (model) return model;
  }
  return getModel(FALLBACK_PROVIDER, FALLBACK_MODEL_ID)!;
}

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
  model: Model<any>;
}

export async function createCanopyAgent(
  options: CreateAgentOptions,
): Promise<Agent> {
  const { chatPanel, registry, toolContext, initialMessages, model } = options;
  const ctx = toolContext ?? {};
  const tools = registry ? registry.resolveTools(ctx) : [];

  const agent = new Agent({
    initialState: {
      systemPrompt: buildSystemPrompt(ctx, registry),
      model,
      thinkingLevel: "off",
      messages: initialMessages ?? [],
    },
  });

  await chatPanel.setAgent(agent, {
    onApiKeyRequired: async (provider: string) => {
      return await ApiKeyPromptDialog.prompt(provider);
    },
    toolsFactory: (_agent, _agentInterface, _artifactsPanel, _runtimeProvidersFactory) => tools,
  });

  return agent;
}

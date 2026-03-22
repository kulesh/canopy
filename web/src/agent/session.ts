import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  ApiKeyPromptDialog,
  type ChatPanel,
} from "@mariozechner/pi-web-ui";

const SYSTEM_PROMPT = `You are Canopy, an AI assistant that helps developers understand and modify codebases at the architectural level.

When a user points you at a codebase, you analyze its structure and present it as a hierarchy of components with human-readable summaries — not as files and lines, but as systems, containers, and components with clear responsibilities and relationships.

You have access to tools for reading files, writing files, editing code, and running commands. Use them to explore and understand codebases, then communicate your understanding in clear, structured human language.

When presenting architecture, use this hierarchy:
- **System**: The top-level project or service
- **Container**: Major subsystems (e.g., API layer, database layer, auth module)
- **Component**: Individual units of functionality within a container
- **Code Unit**: Specific functions, classes, or modules (only when the user drills down)

Always lead with the human-readable summary. Code is a detail the user can drill into, not the primary view.`;

export async function createCanopyAgent(
  chatPanel: ChatPanel,
  initialMessages?: any[],
): Promise<Agent> {
  const agent = new Agent({
    initialState: {
      systemPrompt: SYSTEM_PROMPT,
      model: getModel("anthropic", "claude-sonnet-4-5-20250929"),
      thinkingLevel: "off",
      messages: initialMessages ?? [],
      tools: [],
    },
  });

  await chatPanel.setAgent(agent, {
    onApiKeyRequired: async (provider: string) => {
      return await ApiKeyPromptDialog.prompt(provider);
    },
  });

  return agent;
}

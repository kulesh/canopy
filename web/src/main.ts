import "@mariozechner/mini-lit/dist/ThemeToggle.js";
import {
  AppStorage,
  ChatPanel,
  CustomProvidersStore,
  IndexedDBStorageBackend,
  ProviderKeysStore,
  SessionsStore,
  SettingsDialog,
  SettingsStore,
  SessionListDialog,
  ProvidersModelsTab,
  ProxyTab,
  setAppStorage,
} from "@mariozechner/pi-web-ui";
import type { Agent, AgentMessage } from "@mariozechner/pi-agent-core";
import { html, render } from "lit";
import { icon } from "@mariozechner/mini-lit";
import { Button } from "@mariozechner/mini-lit/dist/Button.js";
import { History, Plus, Settings } from "lucide";
import { createCanopyAgent } from "./agent/session.js";
import "./app.css";

// --- Storage setup ---

const settings = new SettingsStore();
const providerKeys = new ProviderKeysStore();
const sessions = new SessionsStore();
const customProviders = new CustomProvidersStore();

const backend = new IndexedDBStorageBackend({
  dbName: "canopy",
  version: 1,
  stores: [
    settings.getConfig(),
    SessionsStore.getMetadataConfig(),
    providerKeys.getConfig(),
    customProviders.getConfig(),
    sessions.getConfig(),
  ],
});

settings.setBackend(backend);
providerKeys.setBackend(backend);
customProviders.setBackend(backend);
sessions.setBackend(backend);

const storage = new AppStorage(settings, providerKeys, sessions, customProviders, backend);
setAppStorage(storage);

// --- App state ---

let agent: Agent;
let chatPanel: ChatPanel;
let currentSessionId: string | undefined;
let currentTitle = "";
let agentUnsubscribe: (() => void) | undefined;

// --- Session helpers ---

function titleFromMessages(messages: AgentMessage[]): string {
  const first = messages.find(
    (m) => m.role === "user" || m.role === "user-with-attachments",
  );
  if (!first || (first.role !== "user" && first.role !== "user-with-attachments"))
    return "";

  const content = first.content;
  const text =
    typeof content === "string"
      ? content
      : (content as any[])
          .filter((c: any) => c.type === "text")
          .map((c: any) => c.text || "")
          .join(" ");

  const trimmed = text.trim();
  if (!trimmed) return "";
  return trimmed.length <= 60 ? trimmed : `${trimmed.substring(0, 57)}...`;
}

function hasConversation(messages: AgentMessage[]): boolean {
  return (
    messages.some((m: any) => m.role === "user" || m.role === "user-with-attachments") &&
    messages.some((m: any) => m.role === "assistant")
  );
}

async function saveSession() {
  if (!storage.sessions || !currentSessionId || !agent || !currentTitle) return;
  const state = agent.state;
  if (!hasConversation(state.messages)) return;

  await storage.sessions.save(
    {
      id: currentSessionId,
      title: currentTitle,
      model: state.model!,
      thinkingLevel: state.thinkingLevel,
      messages: state.messages,
      createdAt: new Date().toISOString(),
      lastModified: new Date().toISOString(),
    },
    {
      id: currentSessionId,
      title: currentTitle,
      createdAt: new Date().toISOString(),
      lastModified: new Date().toISOString(),
      messageCount: state.messages.length,
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
      },
      thinkingLevel: state.thinkingLevel,
      preview: titleFromMessages(state.messages),
    },
  );
}

// --- Agent lifecycle ---

async function initAgent(initialMessages?: AgentMessage[]) {
  if (agentUnsubscribe) agentUnsubscribe();

  agent = await createCanopyAgent(chatPanel, initialMessages);

  agentUnsubscribe = agent.subscribe((event: any) => {
    if (event.type !== "state-update") return;
    const messages = event.state.messages;

    if (!currentTitle && hasConversation(messages)) {
      currentTitle = titleFromMessages(messages);
    }
    if (!currentSessionId && hasConversation(messages)) {
      currentSessionId = crypto.randomUUID();
      const url = new URL(window.location.href);
      url.searchParams.set("session", currentSessionId);
      window.history.replaceState({}, "", url);
    }
    if (currentSessionId) saveSession();
    renderApp();
  });
}

async function loadSession(sessionId: string): Promise<boolean> {
  if (!storage.sessions) return false;
  const data = await storage.sessions.get(sessionId);
  if (!data) return false;

  currentSessionId = sessionId;
  const metadata = await storage.sessions.getMetadata(sessionId);
  currentTitle = metadata?.title || "";

  await initAgent(data.messages);
  renderApp();
  return true;
}

function newSession() {
  const url = new URL(window.location.href);
  url.search = "";
  window.location.href = url.toString();
}

// --- Render ---

function renderApp() {
  const app = document.getElementById("app");
  if (!app) return;

  render(
    html`
      <div class="w-full h-screen flex flex-col bg-background text-foreground overflow-hidden">
        <div class="flex items-center justify-between border-b border-border shrink-0">
          <div class="flex items-center gap-2 px-4 py-1">
            ${Button({
              variant: "ghost",
              size: "sm",
              children: icon(History, "sm"),
              onClick: () =>
                SessionListDialog.open(
                  async (id) => await loadSession(id),
                  (deletedId) => {
                    if (deletedId === currentSessionId) newSession();
                  },
                ),
              title: "Sessions",
            })}
            ${Button({
              variant: "ghost",
              size: "sm",
              children: icon(Plus, "sm"),
              onClick: newSession,
              title: "New Session",
            })}
            <span class="text-sm font-medium text-foreground truncate max-w-xs">
              ${currentTitle || "Canopy"}
            </span>
          </div>
          <div class="flex items-center gap-1 px-2">
            <theme-toggle></theme-toggle>
            ${Button({
              variant: "ghost",
              size: "sm",
              children: icon(Settings, "sm"),
              onClick: () =>
                SettingsDialog.open([new ProvidersModelsTab(), new ProxyTab()]),
              title: "Settings",
            })}
          </div>
        </div>
        ${chatPanel}
      </div>
    `,
    app,
  );
}

// --- Init ---

async function init() {
  const app = document.getElementById("app");
  if (!app) throw new Error("App container not found");

  render(
    html`
      <div class="w-full h-screen flex items-center justify-center bg-background text-foreground">
        <div class="text-muted-foreground">Loading...</div>
      </div>
    `,
    app,
  );

  chatPanel = new ChatPanel();

  const sessionId = new URLSearchParams(window.location.search).get("session");
  if (sessionId) {
    const loaded = await loadSession(sessionId);
    if (!loaded) {
      newSession();
      return;
    }
  } else {
    await initAgent();
  }

  renderApp();
}

init();

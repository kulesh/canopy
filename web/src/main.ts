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
import { History, Plus, Settings, PanelLeft, PanelLeftClose } from "lucide";
import { createCanopyAgent } from "./agent/session.js";
import { NotebookStore, type CellEdit } from "./notebook/store.js";
import { findLatestNotebook, findLatestChanges } from "./notebook/parse.js";
import { renderNotebookPanel } from "./notebook/panel.js";
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
let notebookVisible = true;

const notebookStore = new NotebookStore();

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

// --- Notebook extraction from agent messages ---

function syncNotebookFromMessages(messages: AgentMessage[]): void {
  const notebook = findLatestNotebook(messages);
  if (notebook && notebook.cells.size > 0) {
    notebookStore.load(notebook);
    if (!notebookVisible) {
      notebookVisible = true;
    }
  }

  // Sync change proposals (Phase 3c)
  const changeSet = findLatestChanges(messages);
  if (changeSet && changeSet.proposals.length > 0) {
    notebookStore.loadChanges(changeSet);
  }
}

// --- Cell edit → agent proposal (Phase 3b) ---

function requestCellChangeProposal(edit: CellEdit): void {
  const cell = notebookStore.cell(edit.cellId);
  if (!cell || !agent) return;

  const filePaths =
    cell.file_paths.length > 0
      ? `\nRelevant files: ${cell.file_paths.join(", ")}`
      : "";

  const prompt = [
    `The user edited the ${cell.kind} "${cell.name}".`,
    ``,
    `Previous description:`,
    `> ${edit.oldSummary}`,
    ``,
    `New description:`,
    `> ${edit.newSummary}`,
    `${filePaths}`,
    ``,
    `Review the relevant code and propose changes to align the implementation with the updated intent. Explain what you would change and why before making edits.`,
  ].join("\n");

  agent.prompt(prompt);
}

// --- Re-scan after changes (Phase 3d) ---

function requestRescan(cellIds: string[]): void {
  if (!agent || cellIds.length === 0) return;

  const names = cellIds
    .map((id) => notebookStore.cell(id)?.name)
    .filter(Boolean);

  if (names.length === 0) return;

  const prompt = [
    `The following components were recently modified: ${names.join(", ")}.`,
    ``,
    `Please re-analyze these components and provide an updated architecture notebook that reflects the current state. Include all cells from the previous notebook, updating the summaries of changed components. Emit a full canopy-notebook fence.`,
  ].join("\n");

  agent.prompt(prompt);
}

// --- Agent lifecycle ---

async function initAgent(initialMessages?: AgentMessage[]) {
  if (agentUnsubscribe) agentUnsubscribe();

  agent = await createCanopyAgent(chatPanel, initialMessages);

  // If restoring a session, check for existing notebook data
  if (initialMessages) {
    syncNotebookFromMessages(initialMessages);
  }

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

    // Check for notebook data in the latest messages
    syncNotebookFromMessages(messages);

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

  const hasNotebook = !notebookStore.empty;

  render(
    html`
      <div class="w-full h-screen flex flex-col bg-background text-foreground overflow-hidden">
        <!-- Header -->
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
            ${hasNotebook
              ? Button({
                  variant: "ghost",
                  size: "sm",
                  children: icon(notebookVisible ? PanelLeftClose : PanelLeft, "sm"),
                  onClick: () => {
                    notebookVisible = !notebookVisible;
                    renderApp();
                  },
                  title: notebookVisible ? "Hide Notebook" : "Show Notebook",
                })
              : ""}
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

        <!-- Main content: notebook + chat side by side -->
        <div class="flex-1 flex overflow-hidden">
          <!-- Notebook panel -->
          ${notebookVisible
            ? html`
                <div class="flex flex-col border-r border-border ${hasNotebook ? 'w-1/2' : 'w-2/5'} shrink-0">
                  <div class="px-3 py-1.5 border-b border-border/50 shrink-0">
                    <span class="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                      Architecture
                    </span>
                  </div>
                  ${renderNotebookPanel(notebookStore, renderApp)}
                </div>
              `
            : ""}

          <!-- Chat panel -->
          <div class="flex-1 min-w-0">
            ${chatPanel}
          </div>
        </div>
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

  // Subscribe to notebook store for re-renders, edits, and change lifecycle
  const dismissedCells: string[] = [];

  notebookStore.subscribe((event) => {
    if (event.type === "cell-edited") {
      requestCellChangeProposal(event.edit);
    }
    if (event.type === "changes-dismissed") {
      dismissedCells.push(event.cellId);
      // When all changes have been reviewed, trigger re-scan
      if (!notebookStore.hasChanges && dismissedCells.length > 0) {
        requestRescan([...dismissedCells]);
        dismissedCells.length = 0;
      }
    }
    if (event.type === "changes-loaded") {
      dismissedCells.length = 0;
    }
    renderApp();
  });

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

// Dev-mode test bridge — allows Playwright to inject notebook data
if (import.meta.env.DEV) {
  (window as any).__canopy__ = {
    store: notebookStore,
    renderApp,
    get agent() { return agent; },
  };
}

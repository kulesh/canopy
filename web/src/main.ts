/**
 * Canopy PWA — Entry Point
 *
 * Wires together storage, plugin registry, orchestrator, and layout.
 * No business logic lives here — just initialization and event wiring.
 */

import "@mariozechner/mini-lit/dist/ThemeToggle.js";
import { ChatPanel } from "@mariozechner/pi-web-ui";
import type { Agent } from "@mariozechner/pi-agent-core";
import { createRegistry } from "./agent/plugins.js";
import { NotebookStore } from "./notebook/store.js";
import { Orchestrator } from "./orchestrator.js";
import { createStorageLayer, ensureProxySettings } from "./storage.js";
import { renderLayout, renderLoading } from "./layout.js";
import "./app.css";

// --- Storage ---

const { settings, providerKeys, storage } = createStorageLayer();

// --- Plugin registry ---

const registry = createRegistry();

// --- Notebook store ---

const notebookStore = new NotebookStore();

// --- Orchestrator ---

let orchestrator: Orchestrator;

function renderApp() {
  renderLayout({
    chatPanel: orchestrator.state.chatPanel,
    notebookStore,
    notebookVisible: orchestrator.state.notebookVisible,
    scanning: orchestrator.state.scanning,
    projectName: orchestrator.state.projectName,
    currentTitle: orchestrator.state.currentTitle,
    agentMessages: () => orchestrator.state.agent?.state?.messages ?? [],
    onOpenProject: () => orchestrator.openProject(),
    onLoadSession: (id) => orchestrator.loadSession(id),
    onDeleteSession: (deletedId) => {
      if (deletedId === orchestrator.state.currentSessionId) newSession();
    },
    onNewSession: newSession,
    setNotebookVisible: (v) => {
      orchestrator.state.notebookVisible = v;
    },
    renderApp,
  });
}

function newSession() {
  const url = new URL(window.location.href);
  url.search = "";
  window.location.href = url.toString();
}

// --- Init ---

async function init() {
  renderLoading();

  const chatPanel = new ChatPanel();

  ensureProxySettings(settings);

  orchestrator = new Orchestrator(
    {
      agent: undefined as unknown as Agent,
      chatPanel,
      currentSessionId: undefined,
      currentTitle: "",
      scanning: false,
      projectName: undefined,
      toolContext: {},
      notebookVisible: false,
    },
    {
      registry,
      notebookStore,
      settings,
      storage,
      renderApp,
    },
  );

  // Subscribe to notebook store for re-renders, edits, and change lifecycle
  const dismissedCells: string[] = [];

  notebookStore.subscribe((event) => {
    if (event.type === "cell-edited") {
      orchestrator.requestCellChangeProposal(event.edit);
    }
    if (event.type === "changes-dismissed") {
      dismissedCells.push(event.cellId);
      // When all changes have been reviewed, trigger re-scan
      if (!notebookStore.hasChanges && dismissedCells.length > 0) {
        orchestrator.requestRescan([...dismissedCells]);
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
    const loaded = await orchestrator.loadSession(sessionId);
    if (!loaded) {
      newSession();
      return;
    }
  } else {
    await orchestrator.initAgent();
  }

  renderApp();
}

init();

// Dev-mode test bridge — allows Playwright to inject notebook data
if (import.meta.env.DEV) {
  (window as any).__canopy__ = {
    store: notebookStore,
    registry,
    get orchestrator() { return orchestrator; },
    get toolContext() { return orchestrator.state.toolContext; },
    renderApp,
    get agent() { return orchestrator.state.agent; },
    get scanner() { return orchestrator.scanner; },
    get isScanning() { return orchestrator.state.scanning; },
    showNotebook() { orchestrator.state.notebookVisible = true; renderApp(); },
    hideNotebook() { orchestrator.state.notebookVisible = false; renderApp(); },
    setProject(handle: FileSystemDirectoryHandle) {
      orchestrator.state.projectName = handle.name;
      orchestrator.state.toolContext = { projectHandle: handle };
      renderApp();
    },
    async setApiKey(provider: string, key: string) {
      await providerKeys.set(provider, key);
    },
  };
}

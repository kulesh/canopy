/**
 * Orchestrator
 *
 * Manages the lifecycle of scanning, editing, and proposal workflows.
 * Coordinates between the scanner agent, chat agent, plugin registry,
 * and notebook store — the glue between agents and UI state.
 *
 * Responsibilities:
 * - Scanner agent lifecycle (start, abort, completion)
 * - Proposer agent lifecycle (cell edit → structured proposals)
 * - Notebook sync from legacy fence-based messages (session restore)
 * - Change dismiss → rescan-components skill dispatch
 * - Session persistence
 */

import type { Agent, AgentMessage } from "@mariozechner/pi-agent-core";
import type { SettingsStore } from "@mariozechner/pi-web-ui";
import type { ChatPanel, AppStorage } from "@mariozechner/pi-web-ui";
import { createCanopyAgent, saveModelPreference, resolveDefaultModel } from "./agent/session.js";
import { createScannerAgent, type ScannerHandle } from "./agent/scanner.js";
import { createProposerAgent, type ProposerHandle } from "./agent/proposer.js";
import type { PluginRegistry, PluginContext } from "./agent/plugins.js";
import type { NotebookStore, CellEdit } from "./notebook/store.js";
import { findLatestNotebook } from "./notebook/parse.js";
import { titleFromMessages, hasConversation } from "./messages.js";

export { titleFromMessages, hasConversation };

// --- Orchestrator ---

export interface OrchestratorState {
  agent: Agent;
  chatPanel: ChatPanel;
  currentSessionId: string | undefined;
  currentTitle: string;
  scanning: boolean;
  projectName: string | undefined;
  toolContext: PluginContext;
  notebookVisible: boolean;
}

export interface OrchestratorDeps {
  registry: PluginRegistry;
  notebookStore: NotebookStore;
  settings: SettingsStore;
  storage: AppStorage;
  renderApp: () => void;
}

export class Orchestrator {
  state: OrchestratorState;
  private deps: OrchestratorDeps;
  private agentUnsubscribe: (() => void) | undefined;
  private scannerHandle: ScannerHandle | undefined;
  private proposerHandle: ProposerHandle | undefined;

  constructor(state: OrchestratorState, deps: OrchestratorDeps) {
    this.state = state;
    this.deps = deps;
  }

  // --- Session persistence ---

  async saveSession(): Promise<void> {
    const { storage } = this.deps;
    const { agent, currentSessionId, currentTitle } = this.state;

    if (!storage.sessions || !currentSessionId || !agent || !currentTitle) {
      if (agent && hasConversation(agent.state.messages)) {
        console.warn("[canopy] saveSession skipped — missing:",
          !storage.sessions && "storage.sessions",
          !currentSessionId && "sessionId",
          !agent && "agent",
          !currentTitle && "title",
        );
      }
      return;
    }
    const agentState = agent.state;
    if (!hasConversation(agentState.messages)) return;

    try {
      await storage.sessions.save(
        {
          id: currentSessionId,
          title: currentTitle,
          model: agentState.model!,
          thinkingLevel: agentState.thinkingLevel,
          messages: agentState.messages,
          createdAt: new Date().toISOString(),
          lastModified: new Date().toISOString(),
        },
        {
          id: currentSessionId,
          title: currentTitle,
          createdAt: new Date().toISOString(),
          lastModified: new Date().toISOString(),
          messageCount: agentState.messages.length,
          usage: {
            input: 0,
            output: 0,
            cacheRead: 0,
            cacheWrite: 0,
            totalTokens: 0,
            cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
          },
          thinkingLevel: agentState.thinkingLevel,
          preview: titleFromMessages(agentState.messages),
        },
      );
    } catch (e) {
      console.error("[canopy] Failed to save session:", e);
    }
  }

  // --- Notebook sync from agent messages ---
  // Used only for session restoration — legacy sessions may contain
  // notebook data in markdown fences. New sessions use the scanner
  // agent's present_notebook tool and the proposer agent's
  // propose_changes tool instead.

  syncNotebookFromMessages(messages: AgentMessage[]): void {
    const { notebookStore } = this.deps;

    if (notebookStore.empty) {
      const notebook = findLatestNotebook(messages);
      if (notebook && notebook.cells.size > 0) {
        notebookStore.load(notebook);
        if (!this.state.notebookVisible) {
          this.state.notebookVisible = true;
        }
      }
    }
  }

  // --- Cell edit → proposer agent ---

  requestCellChangeProposal(edit: CellEdit): void {
    const { notebookStore, settings, renderApp } = this.deps;
    const { toolContext } = this.state;
    const cell = notebookStore.cell(edit.cellId);
    if (!cell || !toolContext.projectHandle) return;

    // Abort any in-progress proposal
    if (this.proposerHandle) {
      this.proposerHandle.abort();
    }

    resolveDefaultModel(settings).then((model) => {
      this.proposerHandle = createProposerAgent({
        projectHandle: toolContext.projectHandle!,
        notebookStore,
        model,
      });

      // The propose_changes tool loads proposals directly into the store.
      // Subscribe for completion to trigger re-render.
      const unsubscribe = this.proposerHandle.subscribe((event) => {
        if (event.type === "tool_execution_end") {
          renderApp();
        }
      });

      this.proposerHandle.propose(cell, edit).then(() => {
        unsubscribe();
        renderApp();
      }).catch((e) => {
        console.error("[canopy] Proposer failed:", e);
        unsubscribe();
        renderApp();
      });
    });
  }

  // --- Re-scan after all changes dismissed ---

  requestRescan(cellIds: string[]): void {
    const { registry, notebookStore } = this.deps;
    const { agent, toolContext } = this.state;
    if (!agent || cellIds.length === 0) return;

    const names = cellIds
      .map((id) => notebookStore.cell(id)?.name)
      .filter(Boolean) as string[];

    if (names.length === 0) return;

    const skill = registry.skill("rescan-components", toolContext);
    if (!skill) return;

    agent.prompt(skill.prompt({ names }));
  }

  // --- Scanner lifecycle ---

  async startScan(handle: FileSystemDirectoryHandle): Promise<void> {
    const { notebookStore, settings, renderApp } = this.deps;

    // Abort any in-progress scan
    if (this.scannerHandle) {
      this.scannerHandle.abort();
    }

    const model = await resolveDefaultModel(settings);
    this.scannerHandle = createScannerAgent({
      projectHandle: handle,
      notebookStore,
      model,
    });

    this.state.scanning = true;
    renderApp();

    // Show notebook panel when scanner delivers data
    const unsubscribe = this.scannerHandle.subscribe((event) => {
      if (event.type === "tool_execution_end") {
        if (!notebookStore.empty && !this.state.notebookVisible) {
          this.state.notebookVisible = true;
          renderApp();
        }
      }
    });

    // Fire and forget, clean up on completion
    this.scannerHandle.scan(handle.name).then(() => {
      this.state.scanning = false;
      unsubscribe();
      renderApp();
    }).catch((e) => {
      console.error("[canopy] Scanner failed:", e);
      this.state.scanning = false;
      unsubscribe();
      renderApp();
    });
  }

  // --- Agent lifecycle ---

  async initAgent(initialMessages?: AgentMessage[]): Promise<void> {
    if (this.agentUnsubscribe) this.agentUnsubscribe();

    const { registry, settings, renderApp } = this.deps;
    const model = await resolveDefaultModel(settings);

    this.state.agent = await createCanopyAgent({
      chatPanel: this.state.chatPanel,
      registry,
      toolContext: this.state.toolContext,
      initialMessages,
      model,
    });

    // Restore notebook from legacy session messages
    if (initialMessages) {
      this.syncNotebookFromMessages(initialMessages);
    }

    let lastModelId = this.state.agent.state.model?.id;

    this.agentUnsubscribe = this.state.agent.subscribe((event: any) => {
      if (event.type !== "message_end" && event.type !== "turn_end") return;
      const messages = this.state.agent.state.messages;

      // Persist model preference when user changes it
      const currentModelId = this.state.agent.state.model?.id;
      if (currentModelId && currentModelId !== lastModelId) {
        lastModelId = currentModelId;
        saveModelPreference(settings, this.state.agent.state.model!);
      }

      if (!this.state.currentTitle && hasConversation(messages)) {
        this.state.currentTitle = titleFromMessages(messages);
      }
      if (!this.state.currentSessionId && hasConversation(messages)) {
        this.state.currentSessionId = crypto.randomUUID();
        const url = new URL(window.location.href);
        url.searchParams.set("session", this.state.currentSessionId);
        window.history.replaceState({}, "", url);
      }
      if (this.state.currentSessionId) this.saveSession();

      renderApp();
    });
  }

  async loadSession(sessionId: string): Promise<boolean> {
    const { storage } = this.deps;
    if (!storage.sessions) return false;
    const data = await storage.sessions.get(sessionId);
    if (!data) return false;

    this.state.currentSessionId = sessionId;
    const metadata = await storage.sessions.getMetadata(sessionId);
    this.state.currentTitle = metadata?.title || "";

    await this.initAgent(data.messages);
    this.deps.renderApp();
    return true;
  }

  // --- Project ---

  async openProject(): Promise<void> {
    const { selectProjectDirectory } = await import("./agent/project.js");
    try {
      const handle = await selectProjectDirectory();
      this.state.projectName = handle.name;
      this.state.toolContext = { projectHandle: handle };

      // Reinitialize chat agent with file system tools now available
      await this.initAgent();

      // Launch scanner agent — separate instance, headless
      await this.startScan(handle);

      this.deps.renderApp();
    } catch (e) {
      // User cancelled the picker — do nothing
      if ((e as Error).name === "AbortError") return;
      console.error("Failed to open project:", e);
    }
  }

  // --- Accessors for test bridge ---

  get scanner(): ScannerHandle | undefined {
    return this.scannerHandle;
  }
}

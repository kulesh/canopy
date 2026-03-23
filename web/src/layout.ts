/**
 * Layout
 *
 * Renders the application shell: header, notebook panel, chat panel,
 * and split-pane management. Pure rendering — no business logic.
 */

import { html, render, type TemplateResult } from "lit";
import { icon } from "@mariozechner/mini-lit";
import { Button } from "@mariozechner/mini-lit/dist/Button.js";
import {
  History,
  Plus,
  Settings,
  PanelLeft,
  PanelLeftClose,
  PanelRight,
  PanelRightClose,
  FolderOpen,
  LoaderCircle,
} from "lucide";
import {
  SettingsDialog,
  SessionListDialog,
  ProvidersModelsTab,
  ProxyTab,
} from "@mariozechner/pi-web-ui";
import type { ChatPanel } from "@mariozechner/pi-web-ui";
import type { NotebookStore } from "./notebook/store.js";
import { renderNotebookPanel } from "./notebook/panel.js";
import { isFileSystemAccessSupported } from "./agent/project.js";
import { hasConversation } from "./messages.js";

// --- Split-pane state ---

const SPLIT_MIN = 20;
const SPLIT_MAX = 80;

let splitPercent = 50;
let mobilePanel: "chat" | "notebook" = "chat";
let chatVisible = true;

function isMobile(): boolean {
  return window.innerWidth < 768;
}

function onSplitterPointerDown(e: PointerEvent, renderApp: () => void): void {
  e.preventDefault();
  const container = (e.target as HTMLElement).parentElement!;
  const rect = container.getBoundingClientRect();
  const target = e.target as HTMLElement;
  target.setPointerCapture(e.pointerId);

  const onMove = (ev: PointerEvent) => {
    const pct = ((ev.clientX - rect.left) / rect.width) * 100;
    splitPercent = Math.max(SPLIT_MIN, Math.min(SPLIT_MAX, pct));
    renderApp();
  };
  const onUp = () => {
    target.removeEventListener("pointermove", onMove);
    target.removeEventListener("pointerup", onUp);
  };
  target.addEventListener("pointermove", onMove);
  target.addEventListener("pointerup", onUp);
}

// --- Render ---

export interface LayoutProps {
  chatPanel: ChatPanel;
  notebookStore: NotebookStore;
  notebookVisible: boolean;
  scanning: boolean;
  projectName: string | undefined;
  currentTitle: string;
  agentMessages: () => any[];
  onOpenProject: () => void;
  onLoadSession: (id: string) => Promise<boolean>;
  onDeleteSession: (id: string) => void;
  onNewSession: () => void;
  setNotebookVisible: (v: boolean) => void;
  renderApp: () => void;
}

export function renderLayout(props: LayoutProps): void {
  const app = document.getElementById("app");
  if (!app) return;

  const {
    chatPanel,
    notebookStore,
    notebookVisible,
    scanning,
    projectName,
    currentTitle,
    agentMessages,
    onOpenProject,
    onLoadSession,
    onDeleteSession,
    onNewSession,
    setNotebookVisible,
    renderApp,
  } = props;

  const hasNotebook = !notebookStore.empty || scanning;
  const mobile = isMobile();
  const notebookHasContent = !notebookStore.empty;
  const showNotebook = (notebookVisible || scanning) && notebookHasContent && (!mobile || mobilePanel === "notebook");
  const showChat = chatVisible && (!mobile || mobilePanel === "chat");
  const showBothPanels = showNotebook && showChat && !mobile;
  const fsSupported = isFileSystemAccessSupported();
  const displayTitle = projectName
    ? `${currentTitle || "Canopy"} — ${projectName}`
    : (currentTitle || "Canopy");

  // Scan overlay: shown in chat area when scanning + no conversation yet
  const messages = agentMessages();
  const chatEmpty = !messages || !hasConversation(messages);
  const showScanOverlay = scanning && chatEmpty && showChat;

  render(
    html`
      <div class="w-full h-screen flex flex-col bg-background text-foreground overflow-hidden">
        <!-- Header -->
        ${renderHeader({
          hasNotebook,
          mobile,
          notebookVisible,
          scanning,
          displayTitle,
          fsSupported,
          showNotebook,
          onOpenProject,
          onLoadSession,
          onDeleteSession,
          onNewSession,
          setNotebookVisible,
          renderApp,
        })}

        <!-- Main content: notebook + chat side by side (desktop) or one at a time (mobile) -->
        <div class="flex-1 flex overflow-hidden">
          <!-- Notebook panel -->
          ${showNotebook
            ? html`
                <div class="flex flex-col shrink-0"
                     style="${showBothPanels ? `width: ${splitPercent}%` : 'width: 100%'}">
                  <div class="px-3 py-1.5 border-b border-border/50 shrink-0 flex items-center gap-2">
                    <span class="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                      Architecture
                    </span>
                    ${scanning
                      ? html`<span class="text-xs text-muted-foreground animate-pulse">Scanning...</span>`
                      : ""}
                  </div>
                  ${renderNotebookPanel(notebookStore, renderApp)}
                </div>
              `
            : ""}

          <!-- Drag handle -->
          ${showBothPanels
            ? html`<div class="splitter" @pointerdown=${(e: PointerEvent) => onSplitterPointerDown(e, renderApp)}></div>`
            : ""}

          <!-- Chat panel -->
          ${showChat
            ? html`
                <div class="flex-1 min-w-0 relative">
                  ${chatPanel}
                  ${showScanOverlay
                    ? html`
                        <div class="scan-overlay">
                          <div class="scan-overlay-content">
                            <span class="scan-overlay-icon">${icon(LoaderCircle, "md")}</span>
                            <span class="text-sm font-medium">Analyzing codebase architecture</span>
                            <span class="text-xs text-muted-foreground">The notebook panel will appear when analysis is ready</span>
                          </div>
                        </div>
                      `
                    : ""}
                </div>
              `
            : ""}
        </div>
      </div>
    `,
    app,
  );
}

// --- Header ---

interface HeaderProps {
  hasNotebook: boolean;
  mobile: boolean;
  notebookVisible: boolean;
  scanning: boolean;
  displayTitle: string;
  fsSupported: boolean;
  showNotebook: boolean;
  onOpenProject: () => void;
  onLoadSession: (id: string) => Promise<boolean>;
  onDeleteSession: (id: string) => void;
  onNewSession: () => void;
  setNotebookVisible: (v: boolean) => void;
  renderApp: () => void;
}

function renderHeader(p: HeaderProps): TemplateResult {
  return html`
    <div class="flex items-center justify-between border-b border-border shrink-0">
      <div class="flex items-center gap-2 px-4 py-1">
        ${Button({
          variant: "ghost",
          size: "sm",
          children: icon(History, "sm"),
          onClick: () =>
            SessionListDialog.open(
              async (id) => await p.onLoadSession(id),
              (deletedId) => p.onDeleteSession(deletedId),
            ),
          title: "Sessions",
        })}
        ${Button({
          variant: "ghost",
          size: "sm",
          children: icon(Plus, "sm"),
          onClick: p.onNewSession,
          title: "New Session",
        })}
        ${p.fsSupported
          ? Button({
              variant: "ghost",
              size: "sm",
              children: icon(FolderOpen, "sm"),
              onClick: p.onOpenProject,
              title: "Open Project",
            })
          : ""}
        <span class="text-sm font-medium text-foreground truncate max-w-xs">
          ${p.displayTitle}
        </span>
        ${p.scanning
          ? html`<span class="scan-chip">${icon(LoaderCircle, "xs")} Scanning</span>`
          : ""}
      </div>
      <div class="flex items-center gap-1 px-2">
        ${p.hasNotebook && !p.mobile
          ? Button({
              variant: "ghost",
              size: "sm",
              children: icon(p.notebookVisible || p.scanning ? PanelLeftClose : PanelLeft, "sm"),
              onClick: () => {
                p.setNotebookVisible(!(p.notebookVisible || p.scanning));
                if (p.notebookVisible && !chatVisible) chatVisible = true;
                p.renderApp();
              },
              title: p.notebookVisible || p.scanning ? "Hide Notebook" : "Show Notebook",
            })
          : ""}
        ${p.hasNotebook && p.mobile
          ? Button({
              variant: "ghost",
              size: "sm",
              children: icon(mobilePanel === "notebook" ? PanelLeftClose : PanelLeft, "sm"),
              onClick: () => {
                mobilePanel = mobilePanel === "chat" ? "notebook" : "chat";
                p.renderApp();
              },
              title: mobilePanel === "chat" ? "Show Notebook" : "Show Chat",
            })
          : ""}
        ${p.showNotebook && !p.mobile
          ? Button({
              variant: "ghost",
              size: "sm",
              children: icon(chatVisible ? PanelRightClose : PanelRight, "sm"),
              onClick: () => {
                chatVisible = !chatVisible;
                if (!chatVisible && !p.notebookVisible && !p.scanning) p.setNotebookVisible(true);
                p.renderApp();
              },
              title: chatVisible ? "Hide Chat" : "Show Chat",
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
  `;
}

/** Render a loading spinner while the app initializes. */
export function renderLoading(): void {
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
}

/**
 * Notebook Panel
 *
 * Renders the full notebook as a tree of foldable cells.
 * Handles keyboard navigation (j/k/Enter/Esc).
 * Composed into the main layout alongside the chat panel.
 */

import { html, type TemplateResult } from "lit";
import type { NotebookStore } from "./store.js";
import { renderCell } from "./cell.js";

export function renderNotebookPanel(
  store: NotebookStore,
  onRender: () => void,
): TemplateResult {
  if (store.empty) {
    return html`
      <div class="flex-1 flex items-center justify-center p-8">
        <div class="text-center space-y-3 max-w-sm">
          <div class="text-3xl text-muted-foreground/30">🌳</div>
          <p class="text-sm text-muted-foreground">
            Ask the agent to analyze a codebase architecture.
          </p>
          <p class="text-xs text-muted-foreground/60 font-mono">
            "Show me the architecture of this project"
          </p>
        </div>
      </div>
    `;
  }

  return html`
    <div
      class="flex-1 overflow-y-auto p-3 space-y-1 focus:outline-none"
      tabindex="0"
      data-notebook-panel
      @keydown=${(e: KeyboardEvent) => handleKeyboard(e, store, onRender)}
    >
      ${store.rootIds.map((rootId) => {
        const cell = store.cell(rootId);
        if (!cell) return "";
        return renderCell(cell, store, 0, onRender);
      })}
    </div>
  `;
}

function handleKeyboard(
  e: KeyboardEvent,
  store: NotebookStore,
  onRender: () => void,
): void {
  // Never intercept keys from form elements — let them handle their own input.
  // This guards against both active edits and race conditions where cancelEdit
  // clears the editing flag before the event bubbles here.
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "TEXTAREA" || tag === "INPUT") return;

  // When editing, let the textarea handle all keys
  if (store.isEditing) return;

  const visible = store.visibleCellIds();
  if (visible.length === 0) return;

  const currentIdx = store.focusedId
    ? visible.indexOf(store.focusedId)
    : -1;

  switch (e.key) {
    case "e": {
      // Enter edit mode on the focused, expanded cell
      e.preventDefault();
      if (store.focusedId && store.isExpanded(store.focusedId)) {
        store.startEdit(store.focusedId);
        onRender();
        // Auto-focus textarea
        requestAnimationFrame(() => {
          const el = document.querySelector(
            `[data-cell-id="${store.focusedId}"] textarea`,
          );
          if (el instanceof HTMLTextAreaElement) {
            el.focus();
            el.setSelectionRange(el.value.length, el.value.length);
          }
        });
      }
      break;
    }
    case "j":
    case "ArrowDown": {
      e.preventDefault();
      const next = Math.min(currentIdx + 1, visible.length - 1);
      store.focus(visible[next]);
      onRender();
      scrollToFocused(visible[next]);
      break;
    }
    case "k":
    case "ArrowUp": {
      e.preventDefault();
      const prev = Math.max(currentIdx - 1, 0);
      store.focus(visible[prev]);
      onRender();
      scrollToFocused(visible[prev]);
      break;
    }
    case "Enter":
    case "l":
    case "ArrowRight": {
      e.preventDefault();
      if (store.focusedId) {
        const cell = store.cell(store.focusedId);
        if (cell && !store.isExpanded(store.focusedId)) {
          store.toggle(store.focusedId);
          onRender();
        }
      }
      break;
    }
    case "Escape":
    case "h":
    case "ArrowLeft": {
      e.preventDefault();
      if (store.focusedId) {
        if (store.isExpanded(store.focusedId)) {
          store.toggle(store.focusedId);
          onRender();
        } else {
          // Navigate to parent
          const parentId = findParent(store, store.focusedId);
          if (parentId) {
            store.focus(parentId);
            onRender();
            scrollToFocused(parentId);
          }
        }
      }
      break;
    }
  }
}

function findParent(store: NotebookStore, childId: string): string | null {
  // Walk the tree to find who has childId in their children array
  const search = (ids: string[]): string | null => {
    for (const id of ids) {
      const cell = store.cell(id);
      if (!cell) continue;
      if (cell.children.includes(childId)) return id;
      const found = search(cell.children);
      if (found) return found;
    }
    return null;
  };
  return search(store.rootIds);
}

function scrollToFocused(cellId: string): void {
  requestAnimationFrame(() => {
    document
      .querySelector(`[data-cell-id="${cellId}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  });
}

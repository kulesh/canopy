/**
 * Notebook Cell Renderer
 *
 * Renders a single semantic cell as a foldable card. Each cell shows:
 * - Header: fold indicator, name, kind badge, dependency count
 * - Body (when expanded): summary, dependencies, file paths, children
 *
 * Uses Lit html templates — no custom element registration.
 * Composed by the notebook panel.
 */

import { html, type TemplateResult } from "lit";
import type { NotebookCell, CellKind, ChangeProposal } from "./types.js";
import type { NotebookStore } from "./store.js";

const KIND_COLORS: Record<CellKind, string> = {
  system: "bg-purple-500/20 text-purple-300 border-purple-500/30",
  container: "bg-blue-500/20 text-blue-300 border-blue-500/30",
  component: "bg-green-500/20 text-green-300 border-green-500/30",
  code_unit: "bg-amber-500/20 text-amber-300 border-amber-500/30",
};

const DEPTH_BORDERS: Record<CellKind, string> = {
  system: "border-l-purple-500",
  container: "border-l-blue-500",
  component: "border-l-green-500",
  code_unit: "border-l-amber-500",
};

function kindBadge(kind: CellKind): TemplateResult {
  const label = kind === "code_unit" ? "unit" : kind;
  return html`
    <span
      class="text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded border ${KIND_COLORS[kind]}"
    >
      ${label}
    </span>
  `;
}

function provenanceBadge(source: "ai" | "human"): TemplateResult {
  const cls =
    source === "ai"
      ? "bg-sky-500/20 text-sky-300 border-sky-500/30"
      : "bg-emerald-500/20 text-emerald-300 border-emerald-500/30";
  return html`
    <span class="text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded border ${cls}">
      ${source}
    </span>
  `;
}

function renderSummary(
  cell: NotebookCell,
  store: NotebookStore,
  onRender: () => void,
): TemplateResult {
  const editing = store.editingCellId === cell.id;

  if (editing) {
    return html`
      <div class="space-y-1.5 pt-2">
        <textarea
          class="w-full bg-secondary/50 text-sm text-foreground rounded border border-primary/40 px-2 py-1.5 leading-relaxed resize-y focus:outline-none focus:ring-1 focus:ring-primary/50"
          rows="3"
          .value=${store.editDraft}
          @input=${(e: Event) => {
            store.updateDraft((e.target as HTMLTextAreaElement).value);
          }}
          @keydown=${(e: KeyboardEvent) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              e.stopPropagation();
              store.commitEdit();
              onRender();
            } else if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              store.cancelEdit();
              onRender();
            }
          }}
          @click=${(e: Event) => e.stopPropagation()}
        ></textarea>
        <div class="flex items-center gap-2 text-[10px] text-muted-foreground/60">
          <span class="font-mono">Ctrl+Enter</span> save
          <span class="mx-1">·</span>
          <span class="font-mono">Esc</span> cancel
        </div>
      </div>
    `;
  }

  return html`
    <p
      class="text-sm text-muted-foreground leading-relaxed pt-2 cursor-text rounded px-1 -mx-1 hover:bg-secondary/30 transition-colors"
      @dblclick=${(e: Event) => {
        e.stopPropagation();
        store.startEdit(cell.id);
        onRender();
        // Auto-focus the textarea after render
        requestAnimationFrame(() => {
          const el = document.querySelector(`[data-cell-id="${cell.id}"] textarea`);
          if (el instanceof HTMLTextAreaElement) {
            el.focus();
            el.setSelectionRange(el.value.length, el.value.length);
          }
        });
      }}
    >
      ${cell.summary}
    </p>
  `;
}

function renderChangeProposal(
  proposal: ChangeProposal,
  store: NotebookStore,
  onRender: () => void,
): TemplateResult {
  return html`
    <div class="rounded border border-amber-500/30 bg-amber-500/5 p-2 space-y-2">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-amber-300">
          Proposed changes
        </span>
        <button
          class="text-[10px] px-1.5 py-0.5 rounded bg-secondary hover:bg-secondary/80 text-muted-foreground cursor-pointer"
          @click=${(e: Event) => {
            e.stopPropagation();
            store.dismissChange(proposal.cell_id);
            onRender();
          }}
        >
          dismiss
        </button>
      </div>
      <p class="text-xs text-muted-foreground leading-relaxed">
        ${proposal.summary}
      </p>
      ${proposal.changes.length > 0
        ? html`
            <div class="space-y-1">
              ${proposal.changes.map(
                (change) => html`
                  <div class="text-xs font-mono rounded bg-secondary/40 p-1.5 space-y-1">
                    <div class="text-muted-foreground/80">
                      ${change.file_path}
                    </div>
                    <div class="text-muted-foreground">
                      ${change.description}
                    </div>
                    ${change.before
                      ? html`<pre class="text-red-400/70 bg-red-500/5 rounded px-1.5 py-1 overflow-x-auto whitespace-pre-wrap">- ${change.before}</pre>`
                      : ""}
                    ${change.after
                      ? html`<pre class="text-green-400/70 bg-green-500/5 rounded px-1.5 py-1 overflow-x-auto whitespace-pre-wrap">+ ${change.after}</pre>`
                      : ""}
                  </div>
                `,
              )}
            </div>
          `
        : ""}
    </div>
  `;
}

export function renderCell(
  cell: NotebookCell,
  store: NotebookStore,
  depth: number,
  onRender: () => void,
): TemplateResult {
  const expanded = store.isExpanded(cell.id);
  const focused = store.focusedId === cell.id;
  const hasChildren = cell.children.length > 0;
  const chevron = hasChildren ? (expanded ? "▾" : "▸") : " ";
  const depCount = cell.dependencies.length;
  const change = store.changeFor(cell.id);

  const borderClass = DEPTH_BORDERS[cell.kind];
  const focusRing = focused ? "ring-1 ring-primary/50" : "";
  const indent = depth * 16;

  return html`
    <div
      class="border-l-2 ${borderClass} ${focusRing} rounded-r transition-all duration-150"
      style="margin-left: ${indent}px"
      data-cell-id=${cell.id}
    >
      <!-- Header -->
      <button
        class="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-secondary/50 transition-colors cursor-pointer"
        @click=${() => {
          store.focus(cell.id);
          if (hasChildren) store.toggle(cell.id);
          onRender();
        }}
        @focus=${() => {
          store.focus(cell.id);
          onRender();
        }}
      >
        <span class="text-muted-foreground text-sm w-4 shrink-0 font-mono">
          ${chevron}
        </span>
        <span class="font-medium text-sm text-foreground truncate">
          ${cell.name}
        </span>
        <span class="flex items-center gap-1.5 ml-auto shrink-0">
          ${change
            ? html`<span class="text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded border bg-amber-500/20 text-amber-300 border-amber-500/30">
                changes
              </span>`
            : ""}
          ${provenanceBadge(cell.provenance.source)}
          ${kindBadge(cell.kind)}
          ${depCount > 0
            ? html`<span
                class="text-[10px] text-muted-foreground px-1.5 py-0.5 rounded border border-border"
              >
                ${depCount}→
              </span>`
            : ""}
        </span>
      </button>

      <!-- Body (expanded) -->
      ${expanded
        ? html`
            <div class="px-3 pb-3 space-y-2 border-t border-border/50">
              <!-- Summary -->
              ${renderSummary(cell, store, onRender)}

              <!-- Change proposal -->
              ${change ? renderChangeProposal(change, store, onRender) : ""}

              <!-- Dependencies -->
              ${cell.dependencies.length > 0
                ? html`
                    <div class="flex flex-wrap gap-1">
                      <span class="text-xs text-muted-foreground/70 mr-1">deps:</span>
                      ${cell.dependencies.map(
                        (dep) => html`
                          <span
                            class="text-xs px-1.5 py-0.5 rounded bg-secondary text-secondary-foreground cursor-pointer hover:bg-secondary/80"
                            @click=${(e: Event) => {
                              e.stopPropagation();
                              store.focus(dep);
                              onRender();
                              // Scroll to dep
                              document
                                .querySelector(`[data-cell-id="${dep}"]`)
                                ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
                            }}
                          >
                            ${store.cell(dep)?.name ?? dep}
                          </span>
                        `,
                      )}
                    </div>
                  `
                : ""}

              <!-- File paths -->
              ${cell.file_paths.length > 0
                ? html`
                    <div class="flex flex-wrap gap-1">
                      <span class="text-xs text-muted-foreground/70 mr-1">files:</span>
                      ${cell.file_paths.map(
                        (path) => html`
                          <span class="text-xs font-mono px-1.5 py-0.5 rounded bg-secondary/50 text-muted-foreground">
                            ${path}
                          </span>
                        `,
                      )}
                    </div>
                  `
                : ""}

              <!-- Children -->
              ${hasChildren && expanded
                ? html`
                    <div class="pt-1 space-y-1">
                      ${cell.children.map((childId) => {
                        const child = store.cell(childId);
                        if (!child) return "";
                        return renderCell(child, store, depth + 1, onRender);
                      })}
                    </div>
                  `
                : ""}
            </div>
          `
        : ""}
    </div>
  `;
}

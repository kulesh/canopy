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
import type { NotebookCell, CellKind } from "./types.js";
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
              <p class="text-sm text-muted-foreground leading-relaxed pt-2">
                ${cell.summary}
              </p>

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

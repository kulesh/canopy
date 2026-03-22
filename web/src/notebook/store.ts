/**
 * Notebook Store
 *
 * Holds the current notebook state and notifies listeners on changes.
 * Single source of truth for the cell tree.
 */

import type { Notebook, NotebookCell } from "./types.js";

export type NotebookEvent =
  | { type: "loaded"; notebook: Notebook }
  | { type: "cell-toggled"; cellId: string; expanded: boolean }
  | { type: "focus-changed"; cellId: string | null };

export type NotebookListener = (event: NotebookEvent) => void;

export class NotebookStore {
  private notebook: Notebook = { cells: new Map(), root_ids: [] };
  private expanded = new Set<string>();
  private focusedCellId: string | null = null;
  private listeners: NotebookListener[] = [];

  get empty(): boolean {
    return this.notebook.cells.size === 0;
  }

  get rootIds(): string[] {
    return this.notebook.root_ids;
  }

  get focusedId(): string | null {
    return this.focusedCellId;
  }

  cell(id: string): NotebookCell | undefined {
    return this.notebook.cells.get(id);
  }

  isExpanded(id: string): boolean {
    return this.expanded.has(id);
  }

  load(notebook: Notebook): void {
    this.notebook = notebook;
    this.expanded.clear();
    this.focusedCellId = null;

    // Auto-expand root cells
    for (const id of notebook.root_ids) {
      this.expanded.add(id);
    }

    this.emit({ type: "loaded", notebook });
  }

  toggle(cellId: string): void {
    const wasExpanded = this.expanded.has(cellId);
    if (wasExpanded) {
      this.expanded.delete(cellId);
    } else {
      this.expanded.add(cellId);
    }
    this.emit({ type: "cell-toggled", cellId, expanded: !wasExpanded });
  }

  focus(cellId: string | null): void {
    if (this.focusedCellId === cellId) return;
    this.focusedCellId = cellId;
    this.emit({ type: "focus-changed", cellId });
  }

  /**
   * Returns cell IDs in visual order (depth-first, respecting expanded state).
   * Used for keyboard navigation.
   */
  visibleCellIds(): string[] {
    const result: string[] = [];
    const walk = (ids: string[]) => {
      for (const id of ids) {
        result.push(id);
        const cell = this.notebook.cells.get(id);
        if (cell && this.expanded.has(id) && cell.children.length > 0) {
          walk(cell.children);
        }
      }
    };
    walk(this.notebook.root_ids);
    return result;
  }

  subscribe(listener: NotebookListener): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  }

  private emit(event: NotebookEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }
}

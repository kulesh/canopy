/**
 * Canopy Notebook Domain Model
 *
 * The unit of interaction is a semantic cell — a human-language description
 * of a software component at a specific level of abstraction. Cells form
 * a tree: systems contain containers, containers contain components,
 * components contain code units.
 *
 * Code is a detail within a cell, not the primary view.
 */

export type CellKind = "system" | "container" | "component" | "code_unit";

export type Provenance = {
  source: "ai" | "human";
  edited_at?: string;
};

export interface NotebookCell {
  id: string;
  kind: CellKind;
  name: string;
  summary: string;
  children: string[];
  dependencies: string[];
  file_paths: string[];
  provenance: Provenance;
}

export interface Notebook {
  cells: Map<string, NotebookCell>;
  root_ids: string[];
}

/**
 * The wire format returned by the agent — uses plain arrays
 * since JSON doesn't have Map.
 */
export interface NotebookWire {
  cells: NotebookCellWire[];
  root_ids: string[];
}

// --- Change proposals (Phase 3c) ---

export interface FileChange {
  file_path: string;
  description: string;
  before?: string;
  after?: string;
}

export interface ChangeProposal {
  cell_id: string;
  summary: string;
  changes: FileChange[];
}

export interface ChangeSet {
  proposals: ChangeProposal[];
}

export type ChangeSetWire = ChangeSet;

// --- Wire format ---

export type NotebookCellWire = Omit<NotebookCell, never>;

export function notebookFromWire(wire: NotebookWire): Notebook {
  const cells = new Map<string, NotebookCell>();
  for (const cell of wire.cells) {
    cells.set(cell.id, cell);
  }
  return { cells, root_ids: wire.root_ids };
}

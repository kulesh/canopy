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

// --- Validation ---

const VALID_KINDS: ReadonlySet<string> = new Set<CellKind>([
  "system",
  "container",
  "component",
  "code_unit",
]);

export function isValidCell(obj: unknown): boolean {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof (obj as any).id === "string" &&
    typeof (obj as any).name === "string" &&
    typeof (obj as any).summary === "string" &&
    VALID_KINDS.has((obj as any).kind) &&
    Array.isArray((obj as any).children) &&
    Array.isArray((obj as any).dependencies) &&
    Array.isArray((obj as any).file_paths)
  );
}

export function isValidNotebookWire(obj: unknown): obj is NotebookWire {
  return (
    typeof obj === "object" &&
    obj !== null &&
    Array.isArray((obj as any).cells) &&
    Array.isArray((obj as any).root_ids) &&
    (obj as any).cells.length > 0 &&
    (obj as any).cells.every(isValidCell)
  );
}

export function isValidFileChange(obj: unknown): obj is FileChange {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof (obj as any).file_path === "string" &&
    typeof (obj as any).description === "string"
  );
}

export function isValidChangeProposal(obj: unknown): obj is ChangeProposal {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof (obj as any).cell_id === "string" &&
    typeof (obj as any).summary === "string" &&
    Array.isArray((obj as any).changes) &&
    (obj as any).changes.every(isValidFileChange)
  );
}

export function isValidChangeSet(obj: unknown): obj is ChangeSet {
  return (
    typeof obj === "object" &&
    obj !== null &&
    Array.isArray((obj as any).proposals) &&
    (obj as any).proposals.every(isValidChangeProposal)
  );
}

/** Normalize a raw cell from wire/fence input, filling in default provenance. */
export function normalizeCell(raw: any): NotebookCellWire {
  return {
    id: raw.id,
    kind: raw.kind as CellKind,
    name: raw.name,
    summary: raw.summary,
    children: raw.children,
    dependencies: raw.dependencies,
    file_paths: raw.file_paths,
    provenance: raw.provenance ?? { source: "ai" },
  };
}

/**
 * Notebook Parser
 *
 * Extracts structured notebook JSON from agent messages.
 *
 * Preferred format — explicit `canopy-notebook` fence:
 *   ```canopy-notebook
 *   { "cells": [...], "root_ids": [...] }
 *   ```
 *
 * Fallback — any code fence (`json`, unmarked, etc.) containing
 * valid notebook JSON. The `isValidNotebookWire` validator is strict
 * enough (cells array with specific required fields + root_ids) that
 * false positives are effectively impossible.
 */

import type { AgentMessage } from "@mariozechner/pi-agent-core";
import {
  type CellKind,
  type NotebookWire,
  type Notebook,
  type ChangeSet,
  type ChangeProposal,
  type FileChange,
  notebookFromWire,
} from "./types.js";

const FENCE_RE = /```canopy-notebook\s*\n([\s\S]*?)```/;
const ANY_FENCE_RE = /```(?:\w*)\s*\n([\s\S]*?)```/g;
const CHANGES_FENCE_RE = /```canopy-changes\s*\n([\s\S]*?)```/;

const VALID_KINDS: Set<string> = new Set([
  "system",
  "container",
  "component",
  "code_unit",
]);

function isValidCell(obj: any): boolean {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof obj.id === "string" &&
    typeof obj.name === "string" &&
    typeof obj.summary === "string" &&
    VALID_KINDS.has(obj.kind) &&
    Array.isArray(obj.children) &&
    Array.isArray(obj.dependencies) &&
    Array.isArray(obj.file_paths)
  );
}

function isValidNotebookWire(obj: any): obj is NotebookWire {
  return (
    typeof obj === "object" &&
    obj !== null &&
    Array.isArray(obj.cells) &&
    Array.isArray(obj.root_ids) &&
    obj.cells.every(isValidCell)
  );
}

function normalizeCell(raw: any): NotebookWire["cells"][number] {
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

/** Extract text from an agent message's content. */
function extractText(message: AgentMessage): string | null {
  const content = message.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return (content as any[])
      .filter((b: any) => b.type === "text")
      .map((b: any) => b.text ?? "")
      .join("\n") || null;
  }
  return null;
}

/** Try to parse a JSON string as a valid notebook. */
function tryParseNotebook(json: string): Notebook | null {
  try {
    const parsed = JSON.parse(json);
    if (!isValidNotebookWire(parsed)) return null;

    const wire: NotebookWire = {
      cells: parsed.cells.map(normalizeCell),
      root_ids: parsed.root_ids,
    };

    return notebookFromWire(wire);
  } catch {
    return null;
  }
}

/**
 * Try to extract a Notebook from an assistant message's text content.
 *
 * Strategy (first match wins):
 * 1. Explicit `canopy-notebook` fence
 * 2. Any code fence containing valid notebook JSON
 */
export function parseNotebookFromMessage(
  message: AgentMessage,
): Notebook | null {
  if (message.role !== "assistant") return null;

  const text = extractText(message);
  if (!text) return null;

  // Preferred: explicit canopy-notebook fence
  const exact = FENCE_RE.exec(text);
  if (exact) {
    const nb = tryParseNotebook(exact[1]);
    if (nb) return nb;
  }

  // Fallback: any code fence with valid notebook JSON
  const fallback = new RegExp(ANY_FENCE_RE.source, ANY_FENCE_RE.flags);
  let match: RegExpExecArray | null;
  while ((match = fallback.exec(text)) !== null) {
    const nb = tryParseNotebook(match[1]);
    if (nb) return nb;
  }

  return null;
}

/**
 * Scan all messages (newest first) for a notebook.
 * Returns the most recent notebook found, or null.
 */
export function findLatestNotebook(
  messages: AgentMessage[],
): Notebook | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const nb = parseNotebookFromMessage(messages[i]);
    if (nb) return nb;
  }
  return null;
}

// --- Change proposal parsing (Phase 3c) ---

function isValidFileChange(obj: any): obj is FileChange {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof obj.file_path === "string" &&
    typeof obj.description === "string"
  );
}

function isValidChangeProposal(obj: any): obj is ChangeProposal {
  return (
    typeof obj === "object" &&
    obj !== null &&
    typeof obj.cell_id === "string" &&
    typeof obj.summary === "string" &&
    Array.isArray(obj.changes) &&
    obj.changes.every(isValidFileChange)
  );
}

function isValidChangeSet(obj: any): obj is ChangeSet {
  return (
    typeof obj === "object" &&
    obj !== null &&
    Array.isArray(obj.proposals) &&
    obj.proposals.every(isValidChangeProposal)
  );
}

/**
 * Extract a ChangeSet from an assistant message's text content.
 * Returns null if no valid canopy-changes fence is found.
 */
export function parseChangesFromMessage(
  message: AgentMessage,
): ChangeSet | null {
  if (message.role !== "assistant") return null;

  const text = extractText(message);
  if (!text) return null;

  const match = CHANGES_FENCE_RE.exec(text);
  if (!match) return null;

  try {
    const parsed = JSON.parse(match[1]);
    if (!isValidChangeSet(parsed)) return null;
    return parsed;
  } catch {
    return null;
  }
}

/**
 * Scan messages (newest first) for change proposals.
 * Returns the most recent ChangeSet, or null.
 */
export function findLatestChanges(
  messages: AgentMessage[],
): ChangeSet | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const cs = parseChangesFromMessage(messages[i]);
    if (cs) return cs;
  }
  return null;
}

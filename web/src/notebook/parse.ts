/**
 * Notebook Parser
 *
 * Extracts structured notebook JSON from agent messages.
 * The agent returns architecture scans as JSON wrapped in a markdown
 * code fence with language `canopy-notebook`. This is an explicit signal
 * — no heuristic guessing.
 *
 * Format:
 *   ```canopy-notebook
 *   { "cells": [...], "root_ids": [...] }
 *   ```
 */

import type { AgentMessage } from "@mariozechner/pi-agent-core";
import {
  type CellKind,
  type NotebookWire,
  type Notebook,
  notebookFromWire,
} from "./types.js";

const FENCE_RE = /```canopy-notebook\s*\n([\s\S]*?)```/;

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

/**
 * Try to extract a Notebook from an assistant message's text content.
 * Returns null if no valid notebook JSON is found.
 */
export function parseNotebookFromMessage(
  message: AgentMessage,
): Notebook | null {
  if (message.role !== "assistant") return null;

  const content = message.content;
  const text =
    typeof content === "string"
      ? content
      : Array.isArray(content)
        ? (content as any[])
            .filter((b: any) => b.type === "text")
            .map((b: any) => b.text ?? "")
            .join("\n")
        : null;

  if (!text) return null;

  const match = FENCE_RE.exec(text);
  if (!match) return null;

  try {
    const parsed = JSON.parse(match[1]);
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

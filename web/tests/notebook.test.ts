/**
 * Notebook integration tests
 *
 * Exercises the data flow through all phases without a browser:
 * - Phase 2: Parse notebook from agent messages, store operations
 * - Phase 3a: Cell editing (start/commit/cancel)
 * - Phase 3b: Edit events emitted for agent proposal
 * - Phase 3c: Parse change proposals, store tracking, dismiss
 * - Phase 3d: Re-scan trigger after all changes dismissed
 */

import { describe, it, expect, vi } from "vitest";
import { NotebookStore } from "../src/notebook/store.js";
import {
  parseNotebookFromMessage,
  findLatestNotebook,
  parseChangesFromMessage,
  findLatestChanges,
} from "../src/notebook/parse.js";
import type { Notebook, ChangeSet } from "../src/notebook/types.js";
import type { AgentMessage } from "@mariozechner/pi-agent-core";

// --- Fixtures ---

function makeAssistantMessage(text: string): AgentMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "messages",
    provider: "anthropic",
    model: "test",
    usage: { inputTokens: 0, outputTokens: 0, inputCachedTokens: 0, inputCacheWriteTokens: 0 },
    stopReason: "stop",
    timestamp: Date.now(),
  } as any;
}

const SAMPLE_NOTEBOOK_JSON = JSON.stringify({
  cells: [
    {
      id: "web-app",
      kind: "system",
      name: "Web Application",
      summary: "A full-stack web application",
      children: ["api-layer", "ui-layer"],
      dependencies: [],
      file_paths: [],
      provenance: { source: "ai" },
    },
    {
      id: "api-layer",
      kind: "container",
      name: "API Layer",
      summary: "REST API backend handling requests",
      children: ["auth-service"],
      dependencies: [],
      file_paths: ["src/api/"],
      provenance: { source: "ai" },
    },
    {
      id: "ui-layer",
      kind: "container",
      name: "UI Layer",
      summary: "React frontend for user interaction",
      children: [],
      dependencies: ["api-layer"],
      file_paths: ["src/ui/"],
      provenance: { source: "ai" },
    },
    {
      id: "auth-service",
      kind: "component",
      name: "Auth Service",
      summary: "Handles JWT authentication and session management",
      children: [],
      dependencies: [],
      file_paths: ["src/api/auth.ts"],
      provenance: { source: "ai" },
    },
  ],
  root_ids: ["web-app"],
});

const SAMPLE_CHANGES_JSON = JSON.stringify({
  proposals: [
    {
      cell_id: "auth-service",
      summary: "Add OAuth2 token refresh to the auth flow",
      changes: [
        {
          file_path: "src/api/auth.ts",
          description: "Add refresh token rotation",
          before: "function authenticate(token: string) {",
          after: "function authenticate(token: string, refreshToken?: string) {",
        },
      ],
    },
  ],
});

// --- Phase 2: Notebook parsing and store ---

describe("Phase 2: Notebook parsing", () => {
  it("parses a canopy-notebook fence from an assistant message", () => {
    const msg = makeAssistantMessage(
      `Here's the architecture:\n\n\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\`\n\nLet me know if you want details.`,
    );
    const notebook = parseNotebookFromMessage(msg);
    expect(notebook).not.toBeNull();
    expect(notebook!.cells.size).toBe(4);
    expect(notebook!.root_ids).toEqual(["web-app"]);
  });

  it("returns null for user messages", () => {
    const msg: AgentMessage = {
      role: "user",
      content: "show architecture",
      timestamp: Date.now(),
    } as any;
    expect(parseNotebookFromMessage(msg)).toBeNull();
  });

  it("parses notebook from a json code fence (fallback)", () => {
    const msg = makeAssistantMessage(
      `Here's the analysis:\n\n\`\`\`json\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\`\n\nLet me know.`,
    );
    const notebook = parseNotebookFromMessage(msg);
    expect(notebook).not.toBeNull();
    expect(notebook!.cells.size).toBe(4);
  });

  it("parses notebook from an unmarked code fence (fallback)", () => {
    const msg = makeAssistantMessage(
      `Architecture:\n\n\`\`\`\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    const notebook = parseNotebookFromMessage(msg);
    expect(notebook).not.toBeNull();
    expect(notebook!.root_ids).toEqual(["web-app"]);
  });

  it("prefers canopy-notebook fence over json fence", () => {
    const v2 = JSON.stringify({
      cells: [
        {
          id: "v2",
          kind: "system",
          name: "V2",
          summary: "From canopy-notebook fence",
          children: [],
          dependencies: [],
          file_paths: [],
          provenance: { source: "ai" },
        },
      ],
      root_ids: ["v2"],
    });
    const msg = makeAssistantMessage(
      `\`\`\`json\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\`\n\n\`\`\`canopy-notebook\n${v2}\n\`\`\``,
    );
    const notebook = parseNotebookFromMessage(msg);
    expect(notebook).not.toBeNull();
    expect(notebook!.root_ids).toEqual(["v2"]);
  });

  it("returns null when no fence is present", () => {
    const msg = makeAssistantMessage("Just some text, no notebook here.");
    expect(parseNotebookFromMessage(msg)).toBeNull();
  });

  it("returns null for malformed JSON in fence", () => {
    const msg = makeAssistantMessage("```canopy-notebook\n{broken json}\n```");
    expect(parseNotebookFromMessage(msg)).toBeNull();
  });

  it("returns null for valid JSON but invalid notebook shape", () => {
    const msg = makeAssistantMessage(
      '```canopy-notebook\n{"cells": [{"id": "x"}], "root_ids": ["x"]}\n```',
    );
    expect(parseNotebookFromMessage(msg)).toBeNull();
  });

  it("ignores non-notebook JSON in code fences", () => {
    const msg = makeAssistantMessage(
      '```json\n{"name": "foo", "version": "1.0"}\n```',
    );
    expect(parseNotebookFromMessage(msg)).toBeNull();
  });

  it("findLatestNotebook returns the newest notebook", () => {
    const older = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    const newer = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${JSON.stringify({
        cells: [
          {
            id: "v2",
            kind: "system",
            name: "V2",
            summary: "Updated",
            children: [],
            dependencies: [],
            file_paths: [],
            provenance: { source: "ai" },
          },
        ],
        root_ids: ["v2"],
      })}\n\`\`\``,
    );
    const result = findLatestNotebook([older, newer]);
    expect(result).not.toBeNull();
    expect(result!.root_ids).toEqual(["v2"]);
  });
});

describe("Phase 2: NotebookStore", () => {
  function loadedStore(): NotebookStore {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    const notebook = parseNotebookFromMessage(msg)!;
    store.load(notebook);
    return store;
  }

  it("loads notebook and exposes cells", () => {
    const store = loadedStore();
    expect(store.empty).toBe(false);
    expect(store.rootIds).toEqual(["web-app"]);
    expect(store.cell("api-layer")?.name).toBe("API Layer");
  });

  it("toggle expands and collapses cells", () => {
    const store = loadedStore();
    // Root cells are auto-expanded on load
    expect(store.isExpanded("web-app")).toBe(true);
    store.toggle("web-app");
    expect(store.isExpanded("web-app")).toBe(false);
    store.toggle("web-app");
    expect(store.isExpanded("web-app")).toBe(true);
  });

  it("focus tracks the focused cell", () => {
    const store = loadedStore();
    expect(store.focusedId).toBeNull();
    store.focus("api-layer");
    expect(store.focusedId).toBe("api-layer");
  });

  it("visibleCellIds respects expanded state", () => {
    const store = loadedStore();
    // Root is auto-expanded, so children are visible
    const visible = store.visibleCellIds();
    expect(visible).toContain("web-app");
    expect(visible).toContain("api-layer");
    expect(visible).toContain("ui-layer");
    // auth-service is child of api-layer which is not expanded
    expect(visible).not.toContain("auth-service");

    // Collapse root — only root visible
    store.toggle("web-app");
    expect(store.visibleCellIds()).toEqual(["web-app"]);
  });
});

// --- Phase 3a: Cell editing ---

describe("Phase 3a: Cell editing", () => {
  function loadedStore(): NotebookStore {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);
    return store;
  }

  it("startEdit enters editing mode with current summary as draft", () => {
    const store = loadedStore();
    store.startEdit("auth-service");
    expect(store.isEditing).toBe(true);
    expect(store.editingCellId).toBe("auth-service");
    expect(store.editDraft).toBe(
      "Handles JWT authentication and session management",
    );
  });

  it("cancelEdit exits editing mode without changing summary", () => {
    const store = loadedStore();
    store.startEdit("auth-service");
    store.updateDraft("Something different");
    store.cancelEdit();
    expect(store.isEditing).toBe(false);
    expect(store.cell("auth-service")?.summary).toBe(
      "Handles JWT authentication and session management",
    );
  });

  it("commitEdit updates the summary and provenance", () => {
    const store = loadedStore();
    store.startEdit("auth-service");
    store.updateDraft("Handles OAuth2 authentication with refresh tokens");
    store.commitEdit();

    expect(store.isEditing).toBe(false);
    const cell = store.cell("auth-service")!;
    expect(cell.summary).toBe(
      "Handles OAuth2 authentication with refresh tokens",
    );
    expect(cell.provenance.source).toBe("human");
    expect(cell.provenance.edited_at).toBeDefined();
  });
});

// --- Phase 3b: Edit events ---

describe("Phase 3b: Edit events", () => {
  it("emits cell-edited event on commitEdit", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    const listener = vi.fn();
    store.subscribe(listener);

    store.startEdit("auth-service");
    store.updateDraft("New description");
    store.commitEdit();

    const editEvent = listener.mock.calls.find(
      ([e]: any) => e.type === "cell-edited",
    );
    expect(editEvent).toBeDefined();
    expect(editEvent![0].edit.cellId).toBe("auth-service");
    expect(editEvent![0].edit.oldSummary).toBe(
      "Handles JWT authentication and session management",
    );
    expect(editEvent![0].edit.newSummary).toBe("New description");
  });

  it("does not emit cell-edited when summary unchanged", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    const listener = vi.fn();
    store.subscribe(listener);

    store.startEdit("auth-service");
    // Don't change the draft — commit with same summary
    store.commitEdit();

    const editEvent = listener.mock.calls.find(
      ([e]: any) => e.type === "cell-edited",
    );
    expect(editEvent).toBeUndefined();
  });
});

// --- Phase 3c: Change proposal parsing ---

describe("Phase 3c: Change proposal parsing", () => {
  it("parses a canopy-changes fence from an assistant message", () => {
    const msg = makeAssistantMessage(
      `Here's what I'd change:\n\n\`\`\`canopy-changes\n${SAMPLE_CHANGES_JSON}\n\`\`\`\n\nShall I proceed?`,
    );
    const cs = parseChangesFromMessage(msg);
    expect(cs).not.toBeNull();
    expect(cs!.proposals).toHaveLength(1);
    expect(cs!.proposals[0].cell_id).toBe("auth-service");
    expect(cs!.proposals[0].changes).toHaveLength(1);
    expect(cs!.proposals[0].changes[0].file_path).toBe("src/api/auth.ts");
  });

  it("returns null for messages without canopy-changes fence", () => {
    const msg = makeAssistantMessage("No changes here.");
    expect(parseChangesFromMessage(msg)).toBeNull();
  });

  it("returns null for malformed JSON", () => {
    const msg = makeAssistantMessage("```canopy-changes\n{bad}\n```");
    expect(parseChangesFromMessage(msg)).toBeNull();
  });

  it("returns null for user messages", () => {
    const msg: AgentMessage = {
      role: "user",
      content: `\`\`\`canopy-changes\n${SAMPLE_CHANGES_JSON}\n\`\`\``,
      timestamp: Date.now(),
    } as any;
    expect(parseChangesFromMessage(msg)).toBeNull();
  });

  it("findLatestChanges returns the newest change set", () => {
    const msg1 = makeAssistantMessage(
      `\`\`\`canopy-changes\n${SAMPLE_CHANGES_JSON}\n\`\`\``,
    );
    const msg2 = makeAssistantMessage(
      `\`\`\`canopy-changes\n${JSON.stringify({
        proposals: [
          {
            cell_id: "ui-layer",
            summary: "Update UI",
            changes: [],
          },
        ],
      })}\n\`\`\``,
    );
    const result = findLatestChanges([msg1, msg2]);
    expect(result).not.toBeNull();
    expect(result!.proposals[0].cell_id).toBe("ui-layer");
  });
});

// --- Phase 3c: Change tracking in store ---

describe("Phase 3c: Store change tracking", () => {
  function storeWithChanges() {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    const cs: ChangeSet = JSON.parse(SAMPLE_CHANGES_JSON);
    store.loadChanges(cs);
    return store;
  }

  it("tracks change proposals for existing cells", () => {
    const store = storeWithChanges();
    expect(store.hasChanges).toBe(true);
    const change = store.changeFor("auth-service");
    expect(change).toBeDefined();
    expect(change!.summary).toBe("Add OAuth2 token refresh to the auth flow");
  });

  it("ignores proposals for non-existent cells", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    store.loadChanges({
      proposals: [
        {
          cell_id: "nonexistent",
          summary: "ghost",
          changes: [],
        },
      ],
    });
    expect(store.hasChanges).toBe(false);
  });

  it("dismissChange removes a single proposal", () => {
    const store = storeWithChanges();
    store.dismissChange("auth-service");
    expect(store.changeFor("auth-service")).toBeUndefined();
    expect(store.hasChanges).toBe(false);
  });

  it("clearChanges removes all proposals", () => {
    const store = storeWithChanges();
    store.clearChanges();
    expect(store.hasChanges).toBe(false);
  });
});

// --- Phase 3d: Re-scan trigger ---

describe("Phase 3d: Re-scan lifecycle", () => {
  it("emits changes-dismissed event when proposal dismissed", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);
    store.loadChanges(JSON.parse(SAMPLE_CHANGES_JSON));

    const listener = vi.fn();
    store.subscribe(listener);

    store.dismissChange("auth-service");

    const dismissEvent = listener.mock.calls.find(
      ([e]: any) => e.type === "changes-dismissed",
    );
    expect(dismissEvent).toBeDefined();
    expect(dismissEvent![0].cellId).toBe("auth-service");
  });

  it("hasChanges becomes false after all proposals dismissed", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    // Load changes for two cells
    store.loadChanges({
      proposals: [
        { cell_id: "auth-service", summary: "Change 1", changes: [] },
        { cell_id: "api-layer", summary: "Change 2", changes: [] },
      ],
    });

    expect(store.hasChanges).toBe(true);
    store.dismissChange("auth-service");
    expect(store.hasChanges).toBe(true);
    store.dismissChange("api-layer");
    expect(store.hasChanges).toBe(false);
  });

  it("loadChanges emits changes-loaded event", () => {
    const store = new NotebookStore();
    const msg = makeAssistantMessage(
      `\`\`\`canopy-notebook\n${SAMPLE_NOTEBOOK_JSON}\n\`\`\``,
    );
    store.load(parseNotebookFromMessage(msg)!);

    const listener = vi.fn();
    store.subscribe(listener);

    const cs: ChangeSet = JSON.parse(SAMPLE_CHANGES_JSON);
    store.loadChanges(cs);

    const loadEvent = listener.mock.calls.find(
      ([e]: any) => e.type === "changes-loaded",
    );
    expect(loadEvent).toBeDefined();
  });
});

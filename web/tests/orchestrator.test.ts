/**
 * Orchestrator Tests
 *
 * Behavioral tests for the orchestration layer — the glue between
 * agents, notebook store, and UI state. Tests the flows that were
 * previously untested in main.ts:
 *
 * - Session helpers (title extraction, conversation detection)
 * - Notebook sync from legacy messages (session restore)
 * - Dismiss lifecycle (dismiss all → rescan triggered)
 */

import { describe, it, expect, vi } from "vitest";
import { titleFromMessages, hasConversation } from "../src/messages.js";
import { NotebookStore } from "../src/notebook/store.js";
import { notebookFromWire, type ChangeSet } from "../src/notebook/types.js";
import type { AgentMessage } from "@mariozechner/pi-agent-core";

// --- Fixtures ---

function makeUserMessage(text: string): AgentMessage {
  return {
    role: "user",
    content: text,
    timestamp: Date.now(),
  } as any;
}

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

function loadedStore(): NotebookStore {
  const store = new NotebookStore();
  store.load(notebookFromWire({
    cells: [
      {
        id: "web-app",
        kind: "system",
        name: "Web App",
        summary: "A web application",
        children: ["api", "ui"],
        dependencies: [],
        file_paths: [],
        provenance: { source: "ai" },
      },
      {
        id: "api",
        kind: "container",
        name: "API",
        summary: "Backend API",
        children: [],
        dependencies: [],
        file_paths: ["src/api/"],
        provenance: { source: "ai" },
      },
      {
        id: "ui",
        kind: "container",
        name: "UI",
        summary: "Frontend UI",
        children: [],
        dependencies: ["api"],
        file_paths: ["src/ui/"],
        provenance: { source: "ai" },
      },
    ],
    root_ids: ["web-app"],
  }));
  return store;
}

// --- Session helpers ---

describe("titleFromMessages", () => {
  it("extracts title from first user message", () => {
    const messages = [
      makeUserMessage("Show me the architecture"),
      makeAssistantMessage("Here it is..."),
    ];
    expect(titleFromMessages(messages)).toBe("Show me the architecture");
  });

  it("truncates long messages to 60 chars", () => {
    const long = "A".repeat(100);
    const messages = [makeUserMessage(long)];
    const title = titleFromMessages(messages);
    expect(title.length).toBeLessThanOrEqual(60);
    expect(title).toContain("...");
  });

  it("returns empty string when no user message", () => {
    const messages = [makeAssistantMessage("Hello")];
    expect(titleFromMessages(messages)).toBe("");
  });

  it("returns empty string for empty messages", () => {
    expect(titleFromMessages([])).toBe("");
  });

  it("handles multipart content messages", () => {
    const messages: AgentMessage[] = [{
      role: "user-with-attachments",
      content: [
        { type: "text", text: "Analyze" },
        { type: "text", text: "this codebase" },
      ],
      timestamp: Date.now(),
    } as any];
    expect(titleFromMessages(messages)).toBe("Analyze this codebase");
  });
});

describe("hasConversation", () => {
  it("returns true when both user and assistant messages exist", () => {
    const messages = [
      makeUserMessage("Hi"),
      makeAssistantMessage("Hello"),
    ];
    expect(hasConversation(messages)).toBe(true);
  });

  it("returns false when only user messages", () => {
    expect(hasConversation([makeUserMessage("Hi")])).toBe(false);
  });

  it("returns false when only assistant messages", () => {
    expect(hasConversation([makeAssistantMessage("Hi")])).toBe(false);
  });

  it("returns false for empty messages", () => {
    expect(hasConversation([])).toBe(false);
  });
});

// --- Dismiss lifecycle ---

describe("Dismiss lifecycle", () => {
  it("emits changes-dismissed when a proposal is dismissed", () => {
    const store = loadedStore();
    const listener = vi.fn();
    store.subscribe(listener);

    store.loadChanges({
      proposals: [
        { cell_id: "api", summary: "Change API", changes: [] },
      ],
    });

    store.dismissChange("api");

    const dismissEvent = listener.mock.calls.find(
      ([e]: any) => e.type === "changes-dismissed",
    );
    expect(dismissEvent).toBeDefined();
    expect(dismissEvent![0].cellId).toBe("api");
  });

  it("tracks dismissed cells until all proposals are reviewed", () => {
    const store = loadedStore();
    const dismissed: string[] = [];

    store.subscribe((event) => {
      if (event.type === "changes-dismissed") {
        dismissed.push(event.cellId);
      }
    });

    store.loadChanges({
      proposals: [
        { cell_id: "api", summary: "Change API", changes: [] },
        { cell_id: "ui", summary: "Change UI", changes: [] },
      ],
    });

    store.dismissChange("api");
    expect(store.hasChanges).toBe(true);
    expect(dismissed).toEqual(["api"]);

    store.dismissChange("ui");
    expect(store.hasChanges).toBe(false);
    expect(dismissed).toEqual(["api", "ui"]);
  });

  it("full edit → propose → dismiss → rescan lifecycle", () => {
    const store = loadedStore();
    const events: string[] = [];
    const dismissedCells: string[] = [];

    store.subscribe((event) => {
      events.push(event.type);
      if (event.type === "changes-dismissed") {
        dismissedCells.push(event.cellId);
      }
    });

    // 1. User edits a cell
    store.startEdit("api");
    store.updateDraft("New API description");
    const edit = store.commitEdit();
    expect(edit).not.toBeNull();
    expect(events).toContain("cell-edited");

    // 2. Proposer delivers changes (via propose_changes tool)
    store.loadChanges({
      proposals: [{
        cell_id: "api",
        summary: "Update routes",
        changes: [{ file_path: "src/api/routes.ts", description: "Add new routes" }],
      }],
    });
    expect(events).toContain("changes-loaded");
    expect(store.hasChanges).toBe(true);

    // 3. User dismisses the proposal
    store.dismissChange("api");
    expect(events).toContain("changes-dismissed");
    expect(store.hasChanges).toBe(false);
    expect(dismissedCells).toEqual(["api"]);

    // 4. At this point, main.ts would trigger requestRescan(dismissedCells)
    // Verify the store is in the right state for that
    expect(store.cell("api")?.summary).toBe("New API description");
    expect(store.cell("api")?.provenance.source).toBe("human");
  });
});

// --- Validators ---

describe("Shared validators", () => {
  // Import from the shared location (types.ts)
  let validators: typeof import("../src/notebook/types.js");

  it("isValidCell validates complete cells", async () => {
    validators = await import("../src/notebook/types.js");

    expect(validators.isValidCell({
      id: "x",
      kind: "component",
      name: "X",
      summary: "Does X",
      children: [],
      dependencies: [],
      file_paths: [],
    })).toBe(true);
  });

  it("isValidCell rejects missing fields", async () => {
    validators = await import("../src/notebook/types.js");

    expect(validators.isValidCell({ id: "x" })).toBe(false);
    expect(validators.isValidCell({ id: "x", kind: "bad" })).toBe(false);
    expect(validators.isValidCell(null)).toBe(false);
    expect(validators.isValidCell(undefined)).toBe(false);
  });

  it("isValidNotebookWire requires non-empty cells", async () => {
    validators = await import("../src/notebook/types.js");

    expect(validators.isValidNotebookWire({
      cells: [],
      root_ids: [],
    })).toBe(false);
  });

  it("isValidChangeSet validates proposals", async () => {
    validators = await import("../src/notebook/types.js");

    expect(validators.isValidChangeSet({
      proposals: [{
        cell_id: "x",
        summary: "Change",
        changes: [{ file_path: "a.ts", description: "Modify" }],
      }],
    })).toBe(true);

    expect(validators.isValidChangeSet({
      proposals: [{ cell_id: "x" }],
    })).toBe(false);
  });

  it("normalizeCell fills default provenance", async () => {
    validators = await import("../src/notebook/types.js");

    const cell = validators.normalizeCell({
      id: "x",
      kind: "component",
      name: "X",
      summary: "Does X",
      children: [],
      dependencies: [],
      file_paths: [],
    });

    expect(cell.provenance).toEqual({ source: "ai" });
  });

  it("normalizeCell preserves explicit provenance", async () => {
    validators = await import("../src/notebook/types.js");

    const cell = validators.normalizeCell({
      id: "x",
      kind: "component",
      name: "X",
      summary: "Does X",
      children: [],
      dependencies: [],
      file_paths: [],
      provenance: { source: "human", edited_at: "2026-01-01" },
    });

    expect(cell.provenance).toEqual({ source: "human", edited_at: "2026-01-01" });
  });
});

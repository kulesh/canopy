/**
 * Proposer Agent Tests
 *
 * Tests the propose_changes tool — the proposer agent's structured
 * output contract. The tool validates input and loads change proposals
 * directly into the NotebookStore, replacing fence scraping.
 *
 * Same testing pattern as scanner.test.ts.
 */

import { describe, it, expect } from "vitest";
import { NotebookStore } from "../src/notebook/store.js";
import { proposeChangesTool } from "../src/agent/tools/propose-changes.js";
import { notebookFromWire } from "../src/notebook/types.js";

// --- Helpers ---

function resultText(result: { content: { type: string; text: string }[] }): string {
  return result.content[0].text;
}

function storeWithNotebook(): NotebookStore {
  const store = new NotebookStore();
  store.load(notebookFromWire({
    cells: [
      {
        id: "web-app",
        kind: "system",
        name: "Web Application",
        summary: "A full-stack web application",
        children: ["api-layer"],
        dependencies: [],
        file_paths: [],
        provenance: { source: "ai" },
      },
      {
        id: "api-layer",
        kind: "container",
        name: "API Layer",
        summary: "REST API backend",
        children: ["auth-service"],
        dependencies: [],
        file_paths: ["src/api/"],
        provenance: { source: "ai" },
      },
      {
        id: "auth-service",
        kind: "component",
        name: "Auth Service",
        summary: "Handles JWT authentication",
        children: [],
        dependencies: [],
        file_paths: ["src/api/auth.ts"],
        provenance: { source: "ai" },
      },
    ],
    root_ids: ["web-app"],
  }));
  return store;
}

// --- Tests ---

describe("propose_changes tool", () => {
  it("loads valid proposals into store", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    const result = await tool.execute("call-1", {
      proposals: [
        {
          cell_id: "auth-service",
          summary: "Add OAuth2 token refresh",
          changes: [
            {
              file_path: "src/api/auth.ts",
              description: "Add refresh token parameter",
              before: "function authenticate(token: string) {",
              after: "function authenticate(token: string, refreshToken?: string) {",
            },
          ],
        },
      ],
    });

    expect(store.hasChanges).toBe(true);
    const change = store.changeFor("auth-service");
    expect(change).toBeDefined();
    expect(change!.summary).toBe("Add OAuth2 token refresh");
    expect(change!.changes).toHaveLength(1);
    expect(change!.changes[0].file_path).toBe("src/api/auth.ts");
    expect(resultText(result)).toContain("1 cell(s)");
    expect(resultText(result)).toContain("1 file change(s)");
  });

  it("loads multiple proposals at once", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    const result = await tool.execute("call-1", {
      proposals: [
        {
          cell_id: "auth-service",
          summary: "Change auth",
          changes: [
            { file_path: "src/api/auth.ts", description: "Modify auth" },
          ],
        },
        {
          cell_id: "api-layer",
          summary: "Change API",
          changes: [
            { file_path: "src/api/index.ts", description: "Modify routes" },
            { file_path: "src/api/middleware.ts", description: "Add middleware" },
          ],
        },
      ],
    });

    expect(store.hasChanges).toBe(true);
    expect(store.changeFor("auth-service")).toBeDefined();
    expect(store.changeFor("api-layer")).toBeDefined();
    expect(resultText(result)).toContain("2 cell(s)");
    expect(resultText(result)).toContain("3 file change(s)");
  });

  it("rejects proposals with missing required fields", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    const result = await tool.execute("call-1", {
      proposals: [
        { cell_id: "auth-service" }, // missing summary and changes
      ],
    });

    expect(store.hasChanges).toBe(false);
    expect(resultText(result)).toContain("Error");
  });

  it("rejects proposals with missing file change fields", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    const result = await tool.execute("call-1", {
      proposals: [
        {
          cell_id: "auth-service",
          summary: "Change",
          changes: [
            { file_path: "src/auth.ts" }, // missing description
          ],
        },
      ],
    });

    expect(store.hasChanges).toBe(false);
    expect(resultText(result)).toContain("Error");
  });

  it("rejects empty proposals array", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    // Empty proposals is technically valid per isValidChangeSet,
    // but the tool should still load it (empty proposals = no changes)
    const result = await tool.execute("call-1", {
      proposals: [],
    });

    // Valid payload — just no proposals
    expect(store.hasChanges).toBe(false);
    expect(resultText(result)).toContain("0 cell(s)");
  });

  it("rejects completely invalid input", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    const result = await tool.execute("call-1", {
      not_proposals: "wrong shape",
    } as any);

    expect(store.hasChanges).toBe(false);
    expect(resultText(result)).toContain("Error");
  });

  it("replaces previous proposals on second call", async () => {
    const store = storeWithNotebook();
    const tool = proposeChangesTool(store);

    await tool.execute("call-1", {
      proposals: [{
        cell_id: "auth-service",
        summary: "First proposal",
        changes: [{ file_path: "a.ts", description: "First" }],
      }],
    });
    expect(store.changeFor("auth-service")!.summary).toBe("First proposal");

    await tool.execute("call-2", {
      proposals: [{
        cell_id: "api-layer",
        summary: "Second proposal",
        changes: [{ file_path: "b.ts", description: "Second" }],
      }],
    });

    // First proposal replaced
    expect(store.changeFor("auth-service")).toBeUndefined();
    expect(store.changeFor("api-layer")!.summary).toBe("Second proposal");
  });
});

/**
 * Scanner Agent Tests
 *
 * Tests the present_notebook tool — the scanner agent's structured
 * output contract. The tool validates input and loads notebooks
 * directly into the NotebookStore, bypassing fence scraping.
 */

import { describe, it, expect } from "vitest";
import { NotebookStore } from "../src/notebook/store.js";
import { presentNotebookTool } from "../src/agent/tools/present-notebook.js";

// --- Helpers ---

function resultText(result: { content: { type: string; text: string }[] }): string {
  return result.content[0].text;
}

// --- Valid notebook payload ---

const VALID_CELLS = [
  {
    id: "web-app",
    kind: "system",
    name: "Web Application",
    summary: "A full-stack web application",
    children: ["api-layer", "ui-layer"],
    dependencies: [],
    file_paths: [],
  },
  {
    id: "api-layer",
    kind: "container",
    name: "API Layer",
    summary: "REST API backend",
    children: [],
    dependencies: [],
    file_paths: ["src/api/"],
  },
  {
    id: "ui-layer",
    kind: "container",
    name: "UI Layer",
    summary: "React frontend",
    children: [],
    dependencies: ["api-layer"],
    file_paths: ["src/ui/"],
  },
];

// --- Tests ---

describe("present_notebook tool", () => {
  it("loads valid notebook into store", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    const result = await tool.execute("call-1", {
      cells: VALID_CELLS,
      root_ids: ["web-app"],
    });

    expect(store.empty).toBe(false);
    expect(store.rootIds).toEqual(["web-app"]);
    expect(store.cell("api-layer")?.name).toBe("API Layer");
    expect(store.cell("ui-layer")?.dependencies).toEqual(["api-layer"]);
    expect(resultText(result)).toContain("3 cells");
  });

  it("sets provenance to ai by default", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    await tool.execute("call-1", {
      cells: VALID_CELLS,
      root_ids: ["web-app"],
    });

    expect(store.cell("web-app")?.provenance).toEqual({ source: "ai" });
  });

  it("rejects cells with missing required fields", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    const result = await tool.execute("call-1", {
      cells: [{ id: "x", name: "X" }], // missing kind, summary, children, etc.
      root_ids: ["x"],
    });

    expect(store.empty).toBe(true);
    expect(resultText(result)).toContain("Error");
  });

  it("rejects empty cells array", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    const result = await tool.execute("call-1", {
      cells: [],
      root_ids: [],
    });

    expect(store.empty).toBe(true);
    expect(resultText(result)).toContain("Error");
  });

  it("rejects invalid cell kind", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    const result = await tool.execute("call-1", {
      cells: [{
        id: "x",
        kind: "invalid-kind",
        name: "X",
        summary: "Test",
        children: [],
        dependencies: [],
        file_paths: [],
      }],
      root_ids: ["x"],
    });

    expect(store.empty).toBe(true);
    expect(resultText(result)).toContain("Error");
  });

  it("replaces existing notebook on second call", async () => {
    const store = new NotebookStore();
    const tool = presentNotebookTool(store);

    await tool.execute("call-1", {
      cells: VALID_CELLS,
      root_ids: ["web-app"],
    });
    expect(store.cell("web-app")).toBeDefined();

    await tool.execute("call-2", {
      cells: [{
        id: "v2",
        kind: "system",
        name: "V2 System",
        summary: "Replaced",
        children: [],
        dependencies: [],
        file_paths: [],
      }],
      root_ids: ["v2"],
    });

    expect(store.cell("web-app")).toBeUndefined();
    expect(store.cell("v2")?.name).toBe("V2 System");
  });
});

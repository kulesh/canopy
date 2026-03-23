# Composable Agents — Implementation Plan

Reference: [ADR-014](../adrs/ADR-014-composable-single-responsibility-agents.md)

## Overview

Replace the monolithic chat-agent-does-everything design with three
composable agents, each with a single responsibility and a structured
output contract via tool calls.

## Phase 1: Scanner Agent (the critical path)

The scanner is where all the current pain is. Fix this first.

### 1a. Create `present_notebook` tool

A tool the scanner agent calls to deliver its output. The tool handler
validates the `NotebookWire` schema and loads it into `NotebookStore`.

```
src/agent/tools/present-notebook.ts

present_notebook({cells, root_ids})
  → validate via isValidNotebookWire()
  → notebookStore.load(notebookFromWire(wire))
  → return "Notebook loaded: {n} cells"
```

The tool's schema IS the output contract. No fence scraping.

### 1b. Create scanner agent factory

```
src/agent/scanner.ts

createScannerAgent(projectHandle, notebookStore) → Agent
  - System prompt: scanning strategy only (20 lines, not 100)
  - Tools: list_directory, read_file, present_notebook
  - No chat panel — runs headless
  - Returns the Agent instance
```

### 1c. Wire scanner into main.ts

```typescript
// In openProject():
scannerAgent = createScannerAgent(handle, notebookStore);
scannerAgent.prompt(`Analyze the architecture of "${handle.name}".`);

// Subscribe to scanner for completion / progress
scannerAgent.subscribe((event) => {
  if (event.type === "agent_end") {
    // Scanner done — notebook is already loaded via tool call
    renderApp();
  }
});
```

The chat agent no longer receives the scan prompt. The chat panel shows
nothing during the scan (or a small status indicator).

### 1d. Remove scan concerns from chat agent

- Remove `NOTEBOOK_FORMAT` and `scanningStrategy` from architecture
  plugin's system prompt contribution to the chat agent
- Remove `syncNotebookFromMessages` (no longer needed — store is
  updated directly by the tool)
- Remove fence parsing fallback from `parse.ts` (optional — keep as
  defense-in-depth if desired)
- Remove `scan-architecture` skill

### Tests

- Unit: `present_notebook` tool validates and rejects bad input
- Unit: Scanner agent factory produces agent with correct tools
- Integration: Scanner agent + mock filesystem → NotebookStore loaded

---

## Phase 2: Proposer Agent

Same pattern as the scanner, but for change proposals.

### 2a. Create `propose_changes` tool

```
src/agent/tools/propose-changes.ts

propose_changes({proposals})
  → validate via isValidChangeSet()
  → notebookStore.loadChanges(changeSet)
  → return "Proposed changes for {n} cells"
```

### 2b. Create proposer agent factory

```
src/agent/proposer.ts

createProposerAgent(projectHandle, notebookStore, edit) → Agent
  - System prompt: code analysis instructions + cell context
  - Tools: read_file, propose_changes
  - Receives the cell edit as initial context
```

### 2c. Wire into main.ts

Replace `requestCellChangeProposal` — instead of prompting the chat
agent, spin up a proposer agent. Replace `requestRescan` similarly.

### Tests

- Unit: `propose_changes` tool validates and rejects bad input
- Integration: Proposer agent + cell edit → ChangeSet in store

---

## Phase 3: Clean up chat agent

With scanning and proposals handled by dedicated agents, the chat agent
becomes purely conversational.

### 3a. Give chat agent notebook-aware tools

- `query_notebook()` — read current notebook state (cell names,
  summaries, structure) so the chat agent can answer questions
- `trigger_scan()` — request a new scan (creates a scanner agent)

### 3b. Simplify chat agent system prompt

Remove all fence formatting instructions. The chat agent's system
prompt becomes: "You are Canopy. You help developers understand
codebases. Use query_notebook to see the current architecture."

### 3c. Fix session persistence

With scan messages out of the chat, sessions become lightweight (just
user messages + assistant text). The serialization issue likely
disappears.

---

## Execution Order

```
Phase 1a → 1b → 1c → 1d → test
Phase 2a → 2b → 2c → test
Phase 3a → 3b → 3c → test
```

Each phase ends with a working, testable system. Phase 1 alone fixes
the primary bug (notebook panel not appearing). Phases 2 and 3 are
refinements.

## Files Changed

| File | Change |
|------|--------|
| `src/agent/tools/present-notebook.ts` | **New** — scanner output tool |
| `src/agent/tools/propose-changes.ts` | **New** — proposer output tool |
| `src/agent/scanner.ts` | **New** — scanner agent factory |
| `src/agent/proposer.ts` | **New** — proposer agent factory |
| `src/agent/plugins/architecture.ts` | Gutted — no longer contributes system prompt to chat |
| `src/agent/session.ts` | Simplified — chat agent only |
| `src/main.ts` | Rewired — openProject uses scanner, edits use proposer |
| `src/notebook/parse.ts` | Kept as fallback or removed |
| `tests/scanner.test.ts` | **New** |
| `tests/proposer.test.ts` | **New** |

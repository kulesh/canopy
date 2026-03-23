# Plan: Composable Single-Responsibility Agents

## Problem

The monolithic chat agent handles scanning, conversation, and change
proposals in one message stream. The scan dumps raw JSON into the chat,
relies on the LLM to use a specific markdown fence (it doesn't), and
pollutes the context window.

## Solution

Three composable agents, each with one job and a structured tool-call
output contract. See [ADR-014](docs/adrs/ADR-014-composable-single-responsibility-agents.md)
and [implementation plan](docs/implementation/plan-composable-agents.md).

## Execution — Phase 1 (Scanner Agent)

This is the critical path that fixes the notebook panel.

### Step 1: `present_notebook` tool
- New file: `src/agent/tools/present-notebook.ts`
- Tool schema matches `NotebookWire` — the agent calls it to deliver
  its structured output
- Tool handler validates via `isValidNotebookWire()`, loads into
  `NotebookStore`, returns confirmation
- Reuses existing validation from `parse.ts`

### Step 2: Scanner agent factory
- New file: `src/agent/scanner.ts`
- `createScannerAgent(handle, notebookStore)` returns a headless Agent
- System prompt: scanning strategy only (~20 lines)
- Tools: `list_directory`, `read_file`, `present_notebook`
- No ChatPanel — runs in background

### Step 3: Wire into main.ts
- `openProject()` creates a scanner agent instead of prompting the chat
- Scanner subscribes to state updates for progress/completion
- Chat panel shows a brief status ("Scanning...") not raw JSON
- Chat agent no longer receives scan prompt or fence instructions

### Step 4: Remove scan concerns from chat agent
- Strip `NOTEBOOK_FORMAT`, `scanningStrategy`, `CHANGE_PROPOSAL_FORMAT`
  from architecture plugin's chat system prompt
- Remove `syncNotebookFromMessages` from main.ts
- Remove `scan-architecture` skill
- Keep `parse.ts` fence fallback as defense-in-depth

### Step 5: Tests
- Unit: `present_notebook` tool validates/rejects input
- Unit: Scanner factory produces agent with correct tool set
- Integration: existing notebook.test.ts still passes

## Phases 2-3 (deferred)
- Phase 2: Proposer agent for change proposals (same pattern)
- Phase 3: Chat agent cleanup + session persistence fix

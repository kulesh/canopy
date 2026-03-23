# ADR-014: Composable Single-Responsibility Agents

## Status
Proposed

## Context

The current architecture uses a single Pi Agent instance for everything:
chat, architecture scanning, change proposals, and rescans. The scan is
triggered by injecting a prompt into the chat (`agent.prompt("Analyze
the architecture...")`), and the notebook data is scraped from the
agent's free-form text response by looking for a `canopy-notebook`
markdown fence.

This creates cascading problems:

1. **Fragile output contract** — The entire notebook pipeline depends on
   the LLM choosing to emit JSON in a specific markdown fence format.
   Models routinely use ```` ```json ```` or dump raw JSON instead of
   ```` ```canopy-notebook ````. No amount of prompt engineering makes
   this reliable.

2. **Chat pollution** — The scan dumps hundreds of lines of raw JSON
   into the conversation. The user never asked for this; it's a
   background computation wearing a conversational disguise.

3. **Context bloat** — Notebook JSON and tool call history from the scan
   consume the chat agent's context window, leaving less room for
   actual conversation.

4. **Session serialization** — Persisting sessions requires serializing
   the entire scan payload (tool calls, tool results, notebook JSON)
   alongside normal chat messages.

5. **Single point of coupling** — One agent, one system prompt, one
   message stream handles three distinct concerns with different
   lifecycles, inputs, and outputs.

The root cause is a violation of the
[Single Responsibility Principle](https://en.wikipedia.org/wiki/Single-responsibility_principle).
The chat agent has one reason to change (conversation UX), the scanner
has another (exploration strategy and notebook schema), and the change
proposer has yet another (code analysis and diff generation). Combining
them into one agent means every concern interferes with every other.

## Decision

Decompose the monolithic agent into **composable single-responsibility
agents**, each with exactly one job and a structured input/output
contract.

### Architecture

```
┌──────────────────────────────────────────────────────┐
│                    Canopy PWA                         │
│                                                      │
│  ┌──────────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Scanner Agent │  │Chat Agent│  │Proposer Agent │  │
│  │              │  │          │  │               │  │
│  │ list_dir     │  │ notebook │  │ read_file     │  │
│  │ read_file    │  │  query   │  │ propose_      │  │
│  │ present_     │  │ trigger_ │  │  changes      │  │
│  │  notebook    │  │  scan    │  │               │  │
│  └──────┬───────┘  └────┬─────┘  └───────┬───────┘  │
│         │               │                │           │
│         └───────┬───────┴────────┬───────┘           │
│                 ▼                ▼                    │
│          ┌─────────────┐  ┌──────────┐               │
│          │NotebookStore│  │ ChatPanel│               │
│          └─────────────┘  └──────────┘               │
└──────────────────────────────────────────────────────┘
```

### Agent Responsibilities

| Agent | Responsibility | Input | Output | Tools |
|-------|---------------|-------|--------|-------|
| **Scanner** | Explore filesystem, build architecture model | Project directory handle | `NotebookWire` via tool call | `list_directory`, `read_file`, `present_notebook` |
| **Chat** | Converse with the user about the codebase | User messages | Conversational text | `query_notebook`, `trigger_scan` |
| **Proposer** | Analyze code changes for edited cells | Cell edit (old/new summary) + file paths | `ChangeSet` via tool call | `read_file`, `propose_changes` |

### Key Design Principles

1. **Tools as output contracts** — Instead of scraping structured data
   from free-form LLM text, each agent delivers its output by calling a
   tool. The scanner calls `present_notebook({cells, root_ids})`; the
   proposer calls `propose_changes({proposals})`. The tool handler
   validates the schema and routes data to `NotebookStore`. The LLM
   cannot "forget" the fence because the tool call IS the output.

2. **Separate agent instances** — Each agent is a distinct `Agent`
   instance with its own message history, system prompt, and tool set.
   The scanner's exploration trace never enters the chat transcript.
   The proposer's code analysis never bloats the chat context.

3. **Store as coordination point** — `NotebookStore` is the shared
   state that all agents read from and write to. The scanner writes
   notebooks. The proposer writes change sets. The chat agent reads
   notebook state to answer questions. No agent talks directly to
   another — they communicate through the store.

4. **Composable, not orchestrated** — Each agent runs independently
   when triggered. `main.ts` wires triggers (project opened → scanner,
   cell edited → proposer, user types → chat) but doesn't orchestrate
   multi-step workflows. Agents are bicycles, not Rube Goldberg
   machines.

### What Changes

| Before | After |
|--------|-------|
| One Agent instance for everything | Three Agent instances, each single-purpose |
| Notebook data scraped from chat text | Notebook data delivered via structured tool call |
| Scan output visible in chat | Scan runs silently; chat shows summary |
| 100-line system prompt for fence formatting | Small focused system prompts per agent |
| Session saves include scan messages | Session saves only chat messages |
| `architecture.ts` plugin contributes system prompt + skills | `architecture.ts` becomes scanner agent factory |

## Consequences

### What We Gain
- **Reliability** — Tool calls have schemas; fences don't
- **Clean chat** — Users see conversation, not JSON dumps
- **Smaller contexts** — Each agent carries only relevant history
- **Testability** — Each agent can be tested in isolation
- **Simpler sessions** — Only chat messages need persistence

### What We Accept
- Three agent instances consume three sets of API calls
- Coordination between agents goes through NotebookStore, adding
  indirection
- Scanner and proposer agents don't benefit from chat context (by
  design — they shouldn't need it)

### What Stays Open
- Whether the chat agent should be able to trigger rescans directly or
  only through the store
- Whether scanner progress should show in the UI (progress bar, status
  line)
- Whether to cache scanner results to avoid re-scanning on page reload

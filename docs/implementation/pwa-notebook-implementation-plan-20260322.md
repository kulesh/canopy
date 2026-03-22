# Implementation Plan: Canopy PWA Notebook Interface

**Date:** 2026-03-22
**ADR:** ADR-013-pwa-notebook-interface

---

## Overview

Build a Progressive Web App that serves as a semantic notebook for agentic software engineering. Pi's coding agent SDK provides all backend capabilities — reading code, generating architectural understanding, mediating edits. The notebook renders that understanding as foldable, editable cells.

Pure TypeScript. No Rust dependency. Pi is the brain; the notebook is the face.

---

## Phase 1: Skeleton — Pi SDK + chat in a PWA shell

**Goal:** A working PWA that embeds Pi's agent SDK. User can chat with the agent about a codebase. Proves the integration works.

### 1a. Scaffold `web/`

```
web/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── index.html
├── public/
│   └── manifest.json         # PWA manifest
├── src/
│   ├── main.ts               # entry point
│   ├── app.ts                # app shell — layout, routing
│   ├── agent/
│   │   └── session.ts        # Pi SDK session setup
│   ├── chat/
│   │   └── chat-panel.ts     # chat interface (pi-web-ui or custom)
│   └── style.css
```

**Tech stack:** Vite + TypeScript. Pi SDK (`@mariozechner/pi-coding-agent`). Pi web-ui components (`@mariozechner/pi-web-ui`) for the chat panel.

### 1b. Pi SDK integration

```typescript
import { createAgentSession, SessionManager, ModelRegistry }
  from "@mariozechner/pi-coding-agent";

const { session } = await createAgentSession({
  sessionManager: SessionManager.inMemory(),
  modelRegistry: new ModelRegistry(authStorage),
});

await session.prompt("Describe the architecture of this codebase.");
```

### 1c. Chat panel

Embed pi-web-ui's `AgentInterface` or build a minimal chat UI:
- User types a message → sent to Pi session
- Agent response streams back → rendered in chat
- Tool calls (file reads, edits) shown as collapsible blocks

**Validation:** Open PWA → chat with Pi about a codebase → agent reads files and responds. PWA installable.

---

## Phase 2: Notebook — render agent understanding as foldable cells

**Goal:** When the user asks "show me the architecture," the agent's response is rendered not as chat text but as a structured notebook of foldable semantic cells.

### 2a. Domain model

```typescript
interface NotebookCell {
  id: string;
  kind: "system" | "container" | "component" | "code_unit";
  name: string;
  summary: string;
  children: string[];
  dependencies: string[];
  file_paths: string[];
  provenance: { source: "ai" | "human"; edited_at?: string };
  expanded: boolean;
}

interface Notebook {
  cells: Map<string, NotebookCell>;
  root_ids: string[];
}
```

### 2b. Architecture scanning prompt

Design a prompt/skill that instructs Pi to:
1. Read the project structure (file tree, key files)
2. Identify architectural layers (systems, containers, components)
3. Return a structured JSON response matching the `Notebook` schema
4. Include human-readable summaries at each level

This is a Pi skill (`SKILL.md` + procedural steps) that the notebook invokes.

### 2c. Notebook renderer

```
┌─────────────────────────────────────────────┐
│ ▸ AuthService                    [AI] [3→2] │  ← collapsed
└─────────────────────────────────────────────┘

┌─────────────────────────────────────────────┐
│ ▾ AuthService                    [AI] [3→2] │  ← expanded
├─────────────────────────────────────────────┤
│ Handles user authentication and session     │
│ management. Validates JWT tokens, manages   │
│ OAuth2 flows, and maintains session state.  │
├─────────────────────────────────────────────┤
│ Dependencies: UserStore, TokenValidator     │
│ Files: src/auth/service.rs (247 lines)      │
├─────────────────────────────────────────────┤
│ ▸ JwtValidator                              │
│ ▸ OAuthFlow                                 │
│ ▸ SessionManager                            │
└─────────────────────────────────────────────┘
```

- Click/tap header → toggle fold/unfold
- Keyboard: `j`/`k` navigate, `Enter` expand, `Esc` collapse
- Depth shown via indentation + left border color
- Provenance badge per cell
- Dependency badges with counts

### 2d. Chat ↔ Notebook integration

- Chat and notebook are side-by-side (or togglable)
- Agent responses that match the notebook schema render as cells
- Conversational responses render as chat messages
- User can ask about specific cells: "tell me more about AuthService"

**Validation:** Ask Pi to analyze a repo → see structured notebook → fold/unfold works → ask follow-up questions about specific components.

---

## Phase 3: Editing — modify the system through the notebook

**Goal:** Users edit cells (intent) and the agent proposes code changes. Users approve or reject.

### 3a. Cell editing

- Double-click or `e` on a cell → summary becomes editable
- User modifies the summary text
- On save, the notebook sends to Pi:

> "The user changed the description of component AuthService from '{old}' to '{new}'. Propose code changes to align the implementation with this new intent."

### 3b. Semantic diff rendering

Agent responds with tool calls (edit, write). The notebook:
1. Maps affected files to cells
2. Shows per-cell diff summary: "Modified: added OAuth2 token refresh"
3. Expandable to see actual code diff
4. Approve/reject per cell or in bulk

### 3c. Agent-initiated changes

When the agent finds bugs or suggests improvements:
1. Agent presents finding in human language (chat or cell annotation)
2. User reviews at the summary level
3. Drills into code only if needed
4. Approves → agent executes the tool calls

### 3d. Re-scan after changes

After approved edits:
1. Trigger incremental re-scan of affected components
2. Update notebook cells with new summaries
3. Highlight what changed since last scan

**Validation:** Edit a cell → agent proposes changes → approve → code changes → notebook refreshes.

---

## Phase 4: Overlays — production context on cells

**Goal:** Attach real-world signals to notebook cells.

### 4a. Overlay model

```typescript
interface CellOverlay {
  source: string;          // "git", "sentry", "coverage"
  kind: string;            // "activity", "errors", "coverage"
  label: string;
  value: string | number;
  updated_at: string;
}
```

### 4b. Git activity overlay (first overlay)

Pi can run git commands via bash tool:
- Recent authors per component
- Change frequency
- Last modified

Rendered as badges on cells.

### 4c. Extensible overlay system

Each overlay is a Pi skill that:
1. Gathers data (git, API calls, file reads)
2. Returns structured overlay data
3. Notebook attaches to matching cells

**Validation:** See git activity badges on cells. Toggle overlays on/off.

---

## Phasing Summary

| Phase | Delivers | Complexity |
|-------|----------|-----------|
| **1: Skeleton** | PWA shell with Pi agent chat | Small |
| **2: Notebook** | Foldable semantic cells from agent analysis | Medium |
| **3: Editing** | Intent editing → agent-mediated code changes | Medium |
| **4: Overlays** | Production context badges on cells | Small-Medium |

**Phase 1 is one session of work.** Get Pi running in a PWA, prove the integration. Phase 2 is the real product — where the notebook thesis is tested. Phases 3-4 layer on interactivity.

---

## Open Design Questions

1. **How does Pi scan architecture?** The quality of the notebook depends on the scanning prompt/skill. This needs careful prompt engineering — probably the hardest part of Phase 2.

2. **Streaming vs batch rendering?** Should cells appear one-by-one as Pi analyzes, or all at once? Streaming feels more responsive but complicates layout. Start with batch; add streaming if the wait feels too long.

3. **Session persistence?** Pi SDK supports session management. Should the notebook auto-save and restore? Yes — but Phase 1 can be ephemeral.

4. **Mobile UX?** The fold/unfold metaphor works well on touch. But the editing flow (Phase 3) needs thought for mobile keyboards. Defer to Phase 3.

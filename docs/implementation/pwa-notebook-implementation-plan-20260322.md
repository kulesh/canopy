# Implementation Plan: Canopy PWA Notebook Interface

**Date:** 2026-03-22
**ADR:** ADR-013-pwa-notebook-interface

---

## Overview

Build a Progressive Web App that renders Canopy's architectural graph as a foldable semantic notebook, with Pi's coding agent SDK providing the agent backend. Four phases, each producing testable, functioning software.

The PWA is pure TypeScript. It consumes the architectural graph as JSON from a Canopy server process. No WASM. Clean separation.

---

## Phase 1: Static Notebook — render an architecture graph as foldable cells

**Goal:** Given an `ArchitectureGraph` JSON file, render it as a navigable, foldable notebook in the browser. No server. No agent. Pure client-side rendering of a static fixture.

### 1a. Scaffold `web/`

```
web/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── index.html
├── public/
│   ├── manifest.json        # PWA manifest
│   └── sw.js                # service worker stub
├── src/
│   ├── main.ts              # entry point
│   ├── domain/
│   │   └── types.ts         # TS interfaces mirroring Canopy's domain models
│   ├── notebook/
│   │   ├── notebook.ts      # notebook container — manages cell tree
│   │   ├── cell.ts          # semantic cell — fold/unfold, render summary
│   │   └── cell.css         # cell styling
│   ├── fixtures/
│   │   └── sample-graph.json # real ArchitectureGraph export for dev
│   └── style.css            # base styles
```

**Tech stack:**
- **Vite** — dev server + build
- **Vanilla TypeScript** — no framework. Web components if useful, plain DOM otherwise.
- **No Pi SDK yet** — Phase 1 is read-only

**Dependencies:** `vite`, `typescript`. That's it.

### 1b. Domain types in TypeScript

Mirror the essential Canopy types. Derived from the JSON output, not from Rust source:

```typescript
interface ArchitectureGraph {
  nodes: Record<string, ArchitectureNode>;
  root_ids: string[];
}

interface ArchitectureNode {
  id: string;
  kind: "system" | "container" | "component" | "code_unit";
  name: string;
  summary: string | null;
  parent_id: string | null;
  children: string[];
  dependencies: string[];
  file_paths: string[];
  provenance: Provenance;
}

interface Provenance {
  source: "ai" | "human";
  author: string | null;
  reason: string | null;
  edited_at: string | null;
}
```

~30 lines. Updated when the JSON schema changes.

### 1c. Notebook renderer

Core UX:

```
┌─────────────────────────────────────────────┐
│ ▸ AuthService                    [AI] [3→2] │  ← collapsed cell
└─────────────────────────────────────────────┘

┌─────────────────────────────────────────────┐
│ ▾ AuthService                    [AI] [3→2] │  ← expanded header
├─────────────────────────────────────────────┤
│ Handles user authentication and session     │  ← summary
│ management. Validates JWT tokens, manages   │
│ OAuth2 flows, and maintains session state.  │
├─────────────────────────────────────────────┤
│ Dependencies: UserStore, TokenValidator     │  ← metadata
│ Files: src/auth/service.rs (247 lines)      │
├─────────────────────────────────────────────┤
│ ▸ JwtValidator                              │  ← nested children
│ ▸ OAuthFlow                                 │
│ ▸ SessionManager                            │
└─────────────────────────────────────────────┘
```

**Behaviors:**
- Click/tap cell header → toggle fold/unfold
- Keyboard: `j`/`k` navigate cells, `Enter` unfold, `Esc` fold, `h`/`l` navigate depth
- Root cells (systems) shown collapsed by default
- Unfolding reveals: summary, metadata, then child cells
- Provenance badge: `[AI]` or `[Human]` with author if present
- Dependency count badge: `[3→2]` = 3 dependents, 2 dependencies
- Indent hierarchy visually (left border + padding per depth level)

**Validation:**
1. Export a real ArchitectureGraph JSON from Canopy TUI (or hand-craft from dogfooding data)
2. Load it in the PWA
3. Fold/unfold works at all levels
4. Keyboard nav works
5. PWA installs on Chrome/Safari

**Deliverable:** A PWA that turns any Canopy architecture graph JSON into a readable, navigable notebook. Immediately useful for sharing architecture views.

---

## Phase 2: Canopy Server + Live Graph

**Goal:** The notebook renders the live architecture of a running Canopy instance, updating as code changes.

### 2a. Server mode for Canopy

Add to `canopy-bin` (or new binary):

```
canopy serve [path]           # starts HTTP server on localhost:3100
canopy serve --port 3100      # configurable port
```

Endpoints:
- `GET /api/graph` — full ArchitectureGraph JSON
- `GET /api/node/:id` — single node with children
- `POST /api/refresh` — trigger re-inference for stale nodes
- `WS /api/events` — stream of `{ type: "node_updated", node: {...} }` events

Uses `axum` or `warp` (lightweight, async, Rust-native).

### 2b. PWA connects to server

- On load: fetch `/api/graph`, render notebook
- WebSocket connection: receive node updates, re-render affected cells
- Reconnect with backoff on disconnect
- Fallback: periodic polling if WebSocket unavailable

### 2c. Change detection pipeline

1. Server watches filesystem (or polls git status)
2. Changed files → map to affected nodes via C4 policy
3. Re-infer summaries for affected nodes only
4. Push updated nodes over WebSocket

**Validation:** `canopy serve ./some-repo` → open PWA → see architecture → change a file → cell updates within seconds.

---

## Phase 3: Agent Integration — Pi SDK + conversational editing

**Goal:** Users can converse with the agent and edit the system through the notebook.

### 3a. Pi SDK integration

```typescript
import { createAgentSession, SessionManager, ModelRegistry }
  from "@mariozechner/pi-coding-agent";

const session = await createAgentSession({
  sessionManager: SessionManager.inMemory(),
  modelRegistry: new ModelRegistry(authStorage),
});
```

- Chat panel (pi-web-ui `AgentInterface` or custom) appears as a slide-over or split pane
- User messages flow through Pi's agent loop
- Agent has access to Pi's built-in tools (read, write, edit, bash)

### 3b. Semantic diff rendering

When the agent proposes code changes:
1. Intercept tool calls (edit, write) from agent responses
2. Map affected files to notebook cells using the graph's file_paths
3. Show cell-level diff: "Modified `AuthService`: added OAuth2 token refresh"
4. Expandable to see actual code diff
5. Approve/reject per cell

### 3c. Intent editing

User edits a cell's summary → triggers agent prompt:

> "The user changed the description of component AuthService from '{old}' to '{new}'. Propose code changes to make the implementation match this intent."

Agent responds → semantic diff rendering → approve/reject loop.

### 3d. Context injection

Before each agent prompt, inject relevant notebook context:
- Current node's summary, dependencies, file paths
- Parent and sibling summaries for architectural context
- Recent edit history from provenance log

This gives the agent architectural awareness without the user having to explain the system.

**Validation:** Chat with agent → see proposed changes as cell-level diffs → approve → code changes → notebook auto-updates via server WebSocket.

---

## Phase 4: Overlays — Production context on semantic blocks

**Goal:** Attach real-world signals to notebook cells.

### 4a. Overlay model

```typescript
interface NodeOverlay {
  source: string;        // "sentry", "github", "datadog"
  kind: "metric" | "alert" | "coverage" | "blame" | "deploy";
  label: string;
  value: string | number;
  updated_at: string;
}
```

Server endpoint: `GET /api/overlays/:node_id` — returns overlays for a node.
Overlay sources are server-side plugins (fetch from external APIs, cache locally).

### 4b. Overlay rendering

Cells gain badges: `[98% cov]` `[3 err/hr]` `[deployed 2h ago]`
Toggle overlay categories in toolbar.
Sparklines for time-series data (error rate trend, deploy frequency).

### 4c. First overlay: git activity

- Recent authors per component
- Change frequency (commits/week)
- Last modified date

Uses Canopy's existing git integration. Zero external dependencies.

**Validation:** See git activity badges on cells. Toggle overlay on/off.

---

## Phasing Summary

| Phase | Delivers | Complexity |
|-------|----------|-----------|
| **1: Static Notebook** | Foldable architecture viewer PWA | Small — pure frontend, no backend |
| **2: Live Graph** | Real-time notebook connected to Canopy server | Medium — new server mode + WebSocket |
| **3: Agent** | Conversational editing via Pi SDK | Medium — Pi integration + diff rendering |
| **4: Overlays** | Production context on cells | Medium — plugin system + external APIs |

**Phase 1 is the MVP.** A static notebook that renders architecture graphs is immediately useful — share architecture views as a link, review system structure on mobile, onboard new contributors. Everything after layers on interactivity.

---

## Open Design Questions

1. **Mobile-first or desktop-first?** The notebook metaphor works on both. PWA gives us both. But the initial layout decisions matter. Recommend: responsive, but optimize for laptop-width first.

2. **Dark theme only?** Canopy TUI is dark-themed. PWA should match. Light theme can come later. Start with one theme done well.

3. **Pi SDK vs Pi RPC for Phase 3?** SDK is cleaner if we're in the same Node.js process. But if Canopy server is Rust and the PWA is static-hosted, we might need Pi running as a separate process. Revisit when we get to Phase 3.

4. **Offline editing?** Phase 1 works offline (static JSON). Phase 2+ requires the server. Decide later how much offline capability matters.

# ADR-013: PWA Notebook Interface with Pi Agent Backend

## Status
Proposed

## Context
Canopy's TUI renders architectural views via Ratatui in the terminal. The thesis we're pursuing: developers doing agentic coding rarely need to see raw code — they need precise high-level details at the right granularity in human language, with the ability to zoom in/out (fold/unfold), edit intent rather than syntax, and have the agent mediate between human language and code changes.

This interaction model maps naturally to a **notebook metaphor** — cells of human-language intent, foldable to reveal implementation, editable to drive agent-mediated code changes. The terminal constrains this vision; a web-based PWA unlocks richer fold/unfold UX, overlays, and accessibility.

Pi (pi.dev) is an open-source coding agent with an SDK mode that provides agent runtime (tool calling, LLM routing, session management) embeddable in custom applications. Pi's web-ui library provides reusable chat components.

## Decision
Build a **pure TypeScript PWA** in `web/` that serves as Canopy's notebook interface, using Pi's SDK for agent capabilities. The PWA consumes Canopy's architectural graph as JSON over HTTP/WebSocket from a Canopy server process. No WASM. No tight coupling to Rust internals.

### Architecture

```
┌─────────────────────────────────┐     ┌──────────────────┐
│        Canopy PWA (TS)          │     │  Canopy Server   │
│  ┌───────────────────────────┐  │     │  (Rust binary)   │
│  │  Semantic Notebook View   │  │◄───►│                  │
│  │  (fold/unfold, editing)   │  │JSON │  GET /api/graph  │
│  ├───────────────────────────┤  │ WS  │  WS /api/events  │
│  │  Pi Agent (SDK)           │  │     │  POST /api/refresh│
│  │  chat, tool calls, models │  │     └──────────────────┘
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

Two clean, decoupled processes that speak JSON:
1. **Canopy server** — Rust. Git, LLM inference, C4 mapping, persistence. Serves the graph.
2. **Canopy PWA** — TypeScript. Rich interactive notebook UI, Pi agent integration.

### Key Decisions

1. **Pi SDK embedding over Pi extension** — The notebook IS the interface, not a panel within Pi. We need full control over the interaction model. Pi SDK gives us agent capabilities without UI constraints.

2. **Same repo** — `web/` sits alongside `lib/` and `bin/`. Two faces of the same product.

3. **Pure TypeScript, no WASM** — Canopy's domain models are serde/JSON structs. TypeScript interfaces mirror them trivially. Graph traversal is a map lookup. The WASM boundary adds build complexity for zero material benefit. The server validates; the client renders.

4. **JSON contract over HTTP/WebSocket** — The PWA and server are decoupled by a JSON API. The PWA doesn't know the server is Rust. The server doesn't know the PWA exists. This means the PWA works with any backend that serves the same graph schema — including a mock server for development and testing.

5. **Intent-first cells** — The unit of interaction is a semantic block described in human language, not a code cell. Code is an expandable detail within each cell. This inverts the Jupyter model.

6. **Provenance-aware overlays** — The data model supports annotations (metrics, coverage, blame, error rates) attached to semantic blocks from day one, using Canopy's existing provenance metadata on graph nodes.

### Alternatives Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **Pi Extension** | Ships as `pi install`, leverages Pi's chrome | Constrained by extension API, notebook is second-class | Rejected |
| **Pi SDK embedding** | Full UX control, Pi handles agent loop | More to build for shell/chrome | **Chosen** |
| **Pi RPC** | Language-agnostic, maximum decoupling | Latency, lose TypeScript API access | Rejected |
| **WASM-coupled PWA** | Single source of truth for types | Build complexity, wasm-pack pipeline, no material benefit for JSON rendering | Rejected |
| **No Pi, custom agent** | Total control | Reimplements tool calling, session mgmt, model routing | Rejected |

## Consequences

### Structural Changes Required
- New `web/` directory with TypeScript/Vite toolchain
- `.mise.toml` gains Node.js runtime for web development
- Canopy server mode added to `bin/` (HTTP/WebSocket API serving the graph)
- JSON schema documented as the contract between server and PWA

### What We Gain
- Rich fold/unfold UX unconstrained by terminal
- PWA installability (offline-capable, native-feel)
- Pi's battle-tested agent runtime (tools, sessions, models)
- Clean decoupling — PWA and server evolve independently
- Foundation for overlays (production metrics, collaboration)
- Simple build: `npm run dev` for the PWA, `cargo run --serve` for the server

### What We Accept
- Two rendering targets to maintain (TUI + PWA)
- TypeScript + Rust polyglot codebase (but cleanly separated)
- Pi SDK as a runtime dependency (open-source, MIT licensed)
- Domain model types duplicated in TypeScript (trivial, ~50 lines)

### Risk Mitigations
- TUI remains the primary interface during PWA development; no regression
- JSON contract means either side can be replaced or mocked independently
- Pi SDK is embeddable and replaceable; the notebook UX is ours
- TypeScript types can be auto-generated from Rust structs via `ts-rs` if duplication becomes burdensome later

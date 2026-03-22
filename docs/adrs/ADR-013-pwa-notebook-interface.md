# ADR-013: PWA Notebook Interface with Pi Agent Backend

## Status
Proposed

## Context
Developers doing agentic coding rarely need to see raw code — they need precise high-level details at the right granularity in human language, with the ability to zoom in/out (fold/unfold), edit intent rather than syntax, and have the agent mediate between human language and code changes.

This interaction model maps naturally to a **notebook metaphor** — cells of human-language intent, foldable to reveal implementation, editable to drive agent-mediated code changes. A web-based PWA is the natural home for this UX.

Pi (pi.dev) is an open-source coding agent with an SDK mode that provides agent runtime (tool calling, LLM routing, session management) embeddable in custom applications. Pi already reads code, generates understanding, and mediates changes — it is the inference engine. We don't need a separate backend.

## Decision
Build a **pure TypeScript PWA** in `web/` powered entirely by Pi's SDK. Pi reads the codebase, generates architectural understanding, and mediates code changes. The notebook renders that understanding as foldable semantic cells. No Rust dependency. No separate server process.

### Architecture

```
┌─────────────────────────────────┐
│        Canopy PWA (TS)          │
│  ┌───────────────────────────┐  │
│  │  Semantic Notebook View   │  │
│  │  fold/unfold intent tree  │  │
│  │  inline editing           │  │
│  │  diff preview             │  │
│  ├───────────────────────────┤  │
│  │  Pi Agent (SDK)           │  │
│  │  reads code               │  │
│  │  generates summaries      │  │
│  │  mediates code changes    │  │
│  │  session management       │  │
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

Single process. The agent IS the backend.

### Key Decisions

1. **Pi SDK as the entire backend** — Pi already has tools to read files, write files, edit code, and run commands. It can generate architectural summaries on demand. No need for a separate Rust server to do what Pi already does.

2. **Completely decoupled from Canopy Rust** — The PWA is a self-contained TypeScript project. It lives in `web/` for co-location but has zero dependency on the Rust workspace. The Rust TUI and the PWA may converge later, but for now they're independent explorations of the same thesis.

3. **Intent-first cells** — The unit of interaction is a semantic block described in human language, not a code cell. Code is an expandable detail within each cell. This inverts the Jupyter model.

4. **Agent-generated architecture** — On first load, Pi scans the codebase and produces an architectural summary structured as a hierarchy of semantic blocks. This is the notebook's content. The user refines, explores, and edits through the notebook; Pi mediates all code interaction.

5. **Provenance-aware from day one** — Each cell tracks whether its content was AI-generated or human-edited, enabling trust calibration and overlay annotations later.

### Alternatives Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **PWA + Canopy Rust server** | Leverages existing C4 pipeline | Cross-language coupling, extra process, premature integration | Rejected for now |
| **PWA + WASM bridge** | Single source of truth for types | Build complexity for zero benefit | Rejected |
| **Pi Extension** | Ships as `pi install` | Constrained by Pi's chrome, notebook is second-class | Rejected |
| **Pure TypeScript PWA + Pi SDK** | Simplest possible architecture, one language, one process | Must build architectural scanning in prompts rather than reuse Rust pipeline | **Chosen** |

## Consequences

### What We Gain
- Simplest possible architecture — one language, one process
- Fast iteration — change TypeScript, see results immediately
- Pi handles all the hard parts (LLM routing, tool calling, session management)
- Clean starting point unconstrained by existing Rust patterns
- Can integrate with Canopy Rust later if/when it makes sense

### What We Accept
- Architectural scanning is prompt-driven (Pi + LLM) rather than code-driven (Canopy's heuristics + mapping policy)
- No offline-first architecture scanning (requires LLM access)
- Domain model defined fresh in TypeScript, not derived from Rust

### What Stays Open
- Whether the Rust TUI and PWA converge or remain parallel interfaces
- Whether Canopy's C4 mapping pipeline feeds into the PWA later
- Production overlay sources (Phase 4 concern)

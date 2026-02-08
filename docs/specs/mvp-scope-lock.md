# MVP Scope Lock (Status Quo Reconciliation)

Date: 2026-02-08  
Status: Active

This document reconciles the product specification with the implemented behavior and locks
the remediation targets for the current cycle.

## Feature Status

| Feature | Spec Intent | Current State | Status |
|---|---|---|---|
| F1 Zero-config load | Open repo directly and infer architecture | Implemented with BYOK and local fallback | Implemented |
| F1.4 Progressive loading | UI interactive while deeper analysis continues | Startup blocks until mapping + semantic inference complete | Partial |
| F2 C4 hierarchy | System -> Container -> Component -> CodeUnit | Implemented with tree navigation and collapse/expand | Implemented |
| F2.5 Breadcrumb | Visible current location breadcrumb | Not explicitly rendered | Partial |
| F3 Semantic summaries | AI summaries + confidence + timestamps | Implemented (provider/cache/local fallback) | Implemented |
| F4 Vim navigation | Keyboard-first flow | Implemented with pane focus and help modal | Implemented |
| F4.3 Back/Esc up behavior | Backspace/Esc goes up hierarchy | Backspace goes up; Esc cancels/quit depending mode | Partial |
| F5 Query | Query mode with mentions and references | Implemented mention resolution and reference jump | Implemented |
| F5.5 Query history recall | Up arrow recalls prior queries | Not implemented | Partial |
| F6 Edit log | Append-only JSONL with export | Implemented | Implemented |
| F7 Summary edit/regenerate | Human edits + regenerate AI summary | Human edits implemented; regenerate currently local-only | Partial |
| Phase 2 Workspace | Multi-repo workspace support | Already implemented behind `--workspace` | Implemented (ahead of MVP) |
| Phase 2 Coverage | Coverage ingestion | Implemented for LCOV if available | Implemented (ahead of MVP) |

## Locked Remediation Targets

1. Replace hardcoded primary C4 mapper with LLM-guided mapping policy + deterministic executor.
2. Make progressive semantic loading truly interactive after TUI launch.
3. Close UX parity gaps: breadcrumb, Esc semantics, query history recall, provider-backed regenerate.
4. Align test and CI workflow with documented strategy (`cargo-nextest` included).
5. Enforce repository hygiene and retrieval index updates.

## Deferred Items

1. Full graph visualization beyond current ASCII dependency view.
2. Local model execution support.
3. Additional provider integrations beyond Anthropic/OpenAI.

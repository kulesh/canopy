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
| F2.5 Breadcrumb | Visible current location breadcrumb | Implemented in Semantic panel metadata | Implemented |
| F3 Semantic summaries | AI summaries + confidence + timestamps | Implemented (provider/cache/local fallback) | Implemented |
| F4 Vim navigation | Keyboard-first flow | Implemented with pane focus and help modal | Implemented |
| F4.3 Back/Esc up behavior | Backspace/Esc goes up hierarchy | Backspace goes up; Esc cancels/quit depending mode | Partial |
| F5 Query | Query mode with mentions and references | Implemented mention resolution and reference jump | Implemented |
| F5.5 Query history recall | Up arrow recalls prior queries | Implemented (`Up/Down` in query mode) | Implemented |
| F6 Edit log | Append-only JSONL with export | Implemented | Implemented |
| F7 Summary edit/regenerate | Human edits + regenerate AI summary | Implemented with provider-backed regenerate and local fallback | Implemented |
| Phase 2 Workspace | Multi-repo workspace support | Already implemented behind `--workspace` | Implemented (ahead of MVP) |
| Phase 2 Coverage | Coverage ingestion | Implemented for LCOV if available | Implemented (ahead of MVP) |

## Active Remediation Targets

1. Make startup fully interactive while policy construction runs.
2. Refine Esc semantics for hierarchy-up behavior parity.
3. Continue accuracy work on model-driven C4 mapping quality against golden-set repos.

## Completed Remediation Targets (This Cycle)

1. Replaced hardcoded primary C4 mapping with policy-driven mapping + deterministic executor.
2. Added bounded validation + repair loop and verifier pass for policy generation.
3. Added query-history recall, breadcrumb rendering, and provider-backed regenerate flow.
4. Added acceptance tests, TUI snapshot tests, and CLI startup e2e checks.
5. Added persistent diagnostics logging and improved source discovery breadth.

## Deferred Items

1. Full graph visualization beyond current ASCII dependency view.
2. Local model execution support.
3. Additional provider integrations beyond Anthropic/OpenAI.

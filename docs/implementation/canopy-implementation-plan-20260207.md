# Canopy Implementation Plan (MVP -> Phase 2)

Status: Proposed  
Date: February 7, 2026  
Inputs: `docs/specs/idea-brief-20260206-033006.md`, `docs/specs/product-spec-20260206-033853.md`

## 1. Purpose

Deliver Canopy as an AI-native TUI that lets developers navigate codebases as C4 architecture, not files, with zero-config startup and provenance-tracked human corrections.

This plan is optimized for:
- Fast delivery of a usable MVP (F1-F7)
- Strong architectural foundations for Phase 2 (F8-F12)
- Measurable quality, performance, and reliability gates

## 2. Scope and Constraints

### In Scope (MVP)
- F1 Zero-config repo loading (BYOK from env)
- F2 C4 hierarchical view (System -> Container -> Component -> Code)
- F3 AI semantic summaries with confidence and timestamps
- F4 Vim-style keyboard navigation
- F5 Natural language query UI with `@component` mentions
- F6 Append-only JSONL edit log in `.canopy/`
- F7 Summary editing with provenance marking and regeneration

### Out of Scope (MVP)
- Multi-repo workspace model
- Coverage ingestion
- Full graph visualization beyond basic relationships
- Offline local model execution

### Non-Negotiable Constraints
- TUI-first UX with keyboard-first operation
- Zero-config startup promise
- Data locality: persistence inside `.canopy/`
- Provider abstraction from day one (Anthropic/OpenAI at minimum)

## 3. Ubiquitous Language (Domain Model)

Core terms to use consistently in code, tests, and docs:
- `Repository`: root path and metadata for a codebase
- `ArchitectureNode`: one C4 node (system/container/component/code-unit)
- `ArchitectureGraph`: complete inferred structure and relationships
- `SemanticSummary`: responsibility/dependency summary with confidence
- `InferenceSnapshot`: timestamped AI analysis output for a node set
- `HumanEdit`: user-authored change to semantic or structural metadata
- `Provenance`: origin marker (`ai` or `human`) plus attribution metadata
- `QuerySession`: natural language interaction history for one app session

## 4. Delivery Strategy

Use vertical slices, not horizontal subsystem completion. Each milestone must produce a testable end-to-end user capability.

### Milestone Sequence
1. M0 Foundation and skeleton app
2. M1 Navigate inferred architecture without LLM
3. M2 AI semantic layer and confidence scoring
4. M3 Query + edit + provenance complete (MVP)
5. M4 Hardening and release readiness
6. M5 Phase 2 extensions

## 5. Technical Architecture Plan

## 5.1 Workspace and Module Boundaries

Keep current workspace (`lib`, `bin`) initially. Build explicit modules in `lib` first; split to extra crates only when boundaries stabilize.

Planned module layout in `lib/src`:
- `domain/`
- `application/`
- `inference/`
- `infrastructure/`
- `tui/`

### Module Responsibilities
- `domain`: C4 node types, invariants, graph traversal, provenance model
- `application`: use-cases, state machine, navigation commands, orchestrators
- `inference`: provider-neutral inference pipeline, prompts, scoring
- `infrastructure`: filesystem/git adapters, persistence, LLM adapters, cache
- `tui`: ratatui widgets/layout/event mapping/render orchestration

## 5.2 Persistence Contract (`.canopy/`)

Initial files:
- `.canopy/c4_model.json`: current architecture graph
- `.canopy/edit_log.jsonl`: append-only edits
- `.canopy/query_history.jsonl`: optional query persistence
- `.canopy/cache.db`: inference and lookup cache (SQLite)

### `edit_log.jsonl` Record Shape (v1)
```json
{
  "timestamp": "2026-02-07T08:15:30Z",
  "author": "kulesh",
  "component_path": "system.auth/container.api/component.jwt_service",
  "field": "summary.responsibility",
  "before": "Handles authentication.",
  "after": "Issues and validates JWT access tokens for API clients.",
  "reason": "Clarifies token lifecycle",
  "provenance": "human"
}
```

## 5.3 Provider Abstraction

Define `LlmProvider` trait and keep all provider specifics in `infrastructure/llm`.

MVP providers:
- Anthropic via `ANTHROPIC_API_KEY`
- OpenAI via `OPENAI_API_KEY`

Required behavior:
- Structured retries with backoff
- Rate limit-aware errors
- Token usage accounting
- Configurable model name via env/CLI flags

## 6. Milestones and Work Breakdown

## M0: Foundation (1 week)

Goals:
- Replace scaffold code with real module skeleton
- Establish coding standards and quality gates
- Define first ADRs

Tasks:
- Create module tree and domain types
- Add app-wide error model and typed results
- Add CLI bootstrap (`clap`) and runtime wiring (`tokio`)
- Add tracing/logging baseline
- Add docs scaffolding: `docs/adrs/`, `docs/testing/`

Acceptance:
- `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt -- --check` all pass
- Empty TUI frame launches and exits with `q`

## M1: Architecture Navigation without LLM (1.5 weeks)

Goals:
- User can open repo and navigate inferred C4 hierarchy immediately

Tasks:
- Repo discovery and `.git` boundary detection
- Ignore handling (`.gitignore`) and file walker
- Heuristic architecture mapper (directory + dependency hints)
- Progressive loader: top-level first, deeper nodes background
- TUI hierarchy pane, breadcrumb, drill-down/up
- Vim movement: `h/j/k/l`, `Enter`, `Esc/Backspace`, `gg`, `G`, `/`, `?`, `q`

Acceptance:
- `canopy <repo_path>` renders system/container/component tree
- Navigation latency <100ms on cached data
- Works on at least 3 language repos before M2

## M2: AI Semantic Layer (2 weeks)

Goals:
- Each visible node gets summary/dependencies/confidence/timestamp

Tasks:
- Inference pipeline passes: structure -> dependencies -> synthesis
- Prompt templates for repository-level and node-level inference
- Confidence scoring model
- Caching: key by repo hash + node fingerprint + model id
- Summary panel in TUI
- Regeneration command plumbing (`r`) with confirmation flow design

Acceptance:
- Summaries visible for selected nodes
- Confidence + last analyzed shown
- Caching prevents repeated calls for unchanged nodes
- Basic cost/usage stats available in status area

## M3: Query, Edit, Provenance (2 weeks)

Goals:
- Complete MVP feature set F5/F6/F7

Tasks:
- Query command mode (`:`), input editor, history recall
- `@component` mention resolver and jump-to-node
- Query response pane and response-to-node references
- Inline summary editor (`e`) with optional reason prompt
- Append-only edit log writer/reader + history viewer
- Provenance display states (`AI`, `Human-Edited`)

Acceptance:
- User can ask question, navigate referenced components, edit summaries
- Every edit appends valid JSONL record
- Human edits survive restart and override regenerated defaults unless confirmed

## M4: Hardening and MVP Release (1.5 weeks)

Goals:
- Achieve reliability/performance/security baseline for public MVP

Tasks:
- Panic reduction and error UX pass
- Performance optimization for initial load and memory
- Timeout/retry/rate-limit behavior tuning
- Golden-set evaluation harness for inference quality
- Release packaging and install instructions
- MVP docs: onboarding, architecture mental model, FAQ, troubleshooting

Acceptance:
- Initial load <60s for representative 100K LOC benchmark repo
- Crash rate target readiness (internal soak)
- C4 quality >70% on golden set
- Public MVP tag candidate ready

## M5: Phase 2 Extensions (post-MVP)

Priority order:
1. Dependency visualization (ASCII graph + cycle detection)
2. Change impact analysis
3. Git integration (recent change/churn/blame/branch compare)
4. Coverage integration
5. Multi-repo workspace

## 7. Testing and Validation Plan

Testing layers:
- Unit tests for domain invariants and parser/scoring logic
- Integration tests for adapters (fs/git/llm mocked)
- Snapshot tests for TUI rendering states (ratatui test backend)
- End-to-end CLI tests for startup, navigation commands, persistence writes
- Golden-set tests for architecture inference quality
- Property tests for graph invariants:
  - Every code unit maps to exactly one component
  - No orphan nodes in persisted graph
  - Dependency edges reference valid nodes

Quality gate policy:
- No implementation merge without corresponding test delta
- Test changes and implementation changes separated by suite run
- `cargo nextest run` required for merge candidate

## 8. Performance Plan

Budgets (MVP):
- Time to interactive: <60s at 100K LOC
- Navigation render latency: <100ms
- Query response start: <3s (network permitting)
- Memory: <500MB at 100K LOC repo

Execution strategy:
- Progressive analysis with cancellation support
- SQLite-backed cache with deterministic cache keys
- Background task scheduler with bounded concurrency
- Incremental recomputation only for changed paths

## 9. Security and Privacy Plan

MVP controls:
- API keys only from environment
- No key logging, no secret persistence
- No telemetry by default (explicit opt-in only)
- Local persistence restricted to `.canopy/`
- Clear disclosure in docs: code is sent to configured LLM provider

## 10. ADR Plan

Create and maintain ADRs in `docs/adrs/`:
- ADR-001: Module and boundary strategy (`lib` modular first, crate split later)
- ADR-002: Inference pipeline design and confidence scoring
- ADR-003: Persistence schema (`c4_model.json`, `edit_log.jsonl`, SQLite cache)
- ADR-004: LLM provider abstraction and fallback behavior
- ADR-005: Progressive loading and background task model
- ADR-006: Provenance conflict resolution rules (AI regen vs human edits)

## 11. Dependencies and Tooling Plan

Add/update dependencies in phases:
- M0: `clap`, `tracing`, `tracing-subscriber`, `ratatui`, `crossterm`
- M1: `ignore`, `walkdir`, `notify`, `petgraph`
- M2: `reqwest`, `serde_json`, `chrono`
- M3: `rusqlite` (or `sqlx` with SQLite), `uuid`
- M4: `proptest`, snapshot testing crate, benchmark support

Toolchain and workflow:
- Keep `.mise.toml` as source of truth for Rust and required dev tools
- Run `cargo fmt`, `cargo clippy`, `cargo nextest run` in CI
- Add lint/test jobs and release build job before MVP tag

## 12. Execution Backlog Template (for `bd`)

Use this issue structure:
- 1 epic per milestone (`M0`...`M5`)
- Tasks per feature slice (UI + domain + persistence + tests together)
- Explicit dependency edges:
  - `M1` depends on `M0`
  - `M2` depends on core of `M1`
  - `M3` depends on `M2`
  - `M4` depends on `M1-M3`

Each task should include:
- User-visible behavior change
- Technical implementation notes
- Test expectations
- Done criteria tied to milestone acceptance

## 13. MVP Exit Criteria

MVP is complete when all are true:
- F1-F7 implemented and validated
- Works on JS/TS, Python, Rust, Go, Java sample repos
- Performance budgets met on benchmark suite
- Quality gate for inference reached (>70% golden-set accuracy)
- Core docs complete (onboarding, architecture, FAQ, troubleshooting)
- Dogfooding active for Canopy’s own repo

## 14. Immediate Next Actions (Week 0)

1. Approve this plan and lock MVP scope
2. Create `docs/adrs/` and write ADR-001 through ADR-003
3. Create `bd` epics/tasks for M0 and M1
4. Replace scaffold lib/bin code with module skeleton and empty TUI frame
5. Set up CI for fmt/clippy/test/build


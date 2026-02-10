# Project View and Async Repo Onboarding Spec

Status: Proposed
Date: 2026-02-10
Owner: Kulesh / Canopy
Related code: `lib/src/application/runner.rs`, `lib/src/application/state/mod.rs`, `lib/src/infrastructure/workspace.rs`

## 1. Problem

Current startup is foreground and blocking per target path. When onboarding or mapping is in progress, users cannot productively work in already-ready repositories from the same session.

## 2. Goal

Add a first-class Project View and background onboarding so users can:
- Manage multiple repositories in one project
- Start onboarding jobs asynchronously
- Continue navigating/querying/editing already-onboarded repos while other repos onboard

## 3. Non-Goals

- Replacing repository-local `.canopy/` persistence for graph/edit/query/cache data
- Distributed job execution across machines
- Cross-project global search in this phase

## 4. Ubiquitous Language

- Project: Named set of repositories and onboarding jobs
- Project Repository: One repo entry tracked by a project
- Onboarding Job: Background task that discovers, maps, validates, and persists one repo graph
- Active Repository: Repo currently shown in Architecture/Semantic/Code panes
- Ready Repository: Repo with validated graph persisted and loadable

## 5. UX Requirements

- New Project View pane/screen listing repos and statuses
- Statuses: `not_started`, `queued`, `discovering`, `policy`, `mapping`, `validating`, `ready`, `failed`, `canceled`
- Real-time progress text per repo
- Actions:
  - queue onboarding
  - cancel running onboarding
  - retry failed onboarding
  - switch active repository to any ready repo
- Existing architecture navigation/query/edit flows remain unchanged for the active ready repo

## 6. System Behavior

### 6.1 Onboarding State Machine

`not_started -> queued -> discovering -> policy -> mapping -> validating -> ready`

Failure/cancel transitions:
- any running state -> `failed(error)`
- any running state -> `canceled`
- `failed`/`canceled` -> `queued` via retry

### 6.2 Concurrency Model

- Add onboarding worker pool with bounded concurrency (default 1; configurable)
- Jobs run in background and emit progress events over channel
- Foreground UI loop remains responsive and consumes onboarding events similarly to inference events

### 6.3 Data Model

Project manifest file (workspace-level):
- project id/name
- repository entries: id, name, path, enabled
- optional default active repo id

Project runtime state (workspace-level persisted JSON):
- repository onboarding status
- last error message
- last successful onboard timestamp
- latest progress phase label/percent

Repository-local data remains in each repo `.canopy/`:
- `c4_model.json`
- `mapping_policy.json`
- `edit_log.jsonl`
- `query_history.jsonl`
- `cache.db`

## 7. Architecture Changes

### 7.1 Domain

Introduce project domain types and onboarding status enums independent from TUI rendering.

### 7.2 Application

Add project/onboarding orchestration layer:
- queue management
- worker execution
- event emission
- state reduction into `AppState`

### 7.3 Infrastructure

Add project manifest/state persistence and reuse existing repository mapping pipeline.

### 7.4 TUI

Add Project View rendering and key bindings while preserving existing panes.

## 8. CLI Contract

Add optional project-oriented startup:
- `--project <PROJECT_FILE>` (workspace project manifest)
- If omitted, current single-repo behavior stays supported

## 9. Backward Compatibility

- Existing single-repo startup remains default and behavior-compatible
- Existing workspace merge path keeps working; project orchestration becomes preferred path for multi-repo interactive flow

## 10. Observability

Add onboarding diagnostics events:
- job queued/start/progress/completed/failed/canceled
- per-repo duration and stage

## 11. Security and Safety

- Resolve repo paths using canonicalized filesystem paths
- Reject duplicate repository roots within one project
- Do not execute arbitrary commands from project files

## 12. Testing Strategy

### Unit

- onboarding state-machine transitions
- queue scheduling/cancel/retry behavior
- project manifest parsing/validation

### Integration

- background onboarding emits progress and persists ready graph
- failed onboarding transitions to `failed` with reason
- active repo switching keeps query/edit behavior isolated to selected repo

### TUI Snapshot

- project list rendering for mixed statuses
- key-binding overlays include project actions

### End-to-End

- start with project file containing multiple repos
- onboard repo A while using ready repo B
- retry failed repo and confirm transition to ready

## 13. Rollout Plan

Phase 1:
- domain + infrastructure project manifest/state
- background onboarding workers + events (no UI yet)

Phase 2:
- Project View UI + keybindings
- active repo switching

Phase 3:
- cancel/retry UX hardening
- diagnostics polish and docs updates

## 14. Acceptance Criteria

- User can queue onboarding for repo X and continue editing/querying ready repo Y in same session
- Project View reflects live status transitions and errors
- Repo-local `.canopy/` artifacts are produced for ready repos
- All mandatory gates pass

---

## File-Level Change Plan

### A. Domain Layer

1. Add `lib/src/domain/project.rs`
- `Project`, `ProjectRepository`, `ProjectRepositoryId`
- `OnboardingStatus`, `OnboardingPhase`, `OnboardingError`
- validation helpers (unique roots, stable ids)

2. Update `lib/src/domain/mod.rs`
- export new project domain types

### B. Infrastructure Layer

3. Add `lib/src/infrastructure/project_store.rs`
- load/save project manifest
- load/save project runtime status
- atomic write helpers for workspace-level project state

4. Update `lib/src/infrastructure/mod.rs`
- export `ProjectStore` and project persistence types

5. Add `lib/src/infrastructure/project_paths.rs`
- resolve project root
- derive workspace-level `.canopy-project/` storage paths

### C. Application Layer

6. Add `lib/src/application/project/mod.rs`
- onboarding job queue API
- worker pool bootstrap
- command methods: queue/cancel/retry/switch

7. Add `lib/src/application/project/events.rs`
- `ProjectEvent` / `OnboardingEvent` payloads

8. Add `lib/src/application/project/scheduler.rs`
- bounded concurrency queue execution

9. Add `lib/src/application/project/reducer.rs`
- reduce events into UI-facing repository status model

10. Update `lib/src/application/config.rs`
- add `project_path: Option<PathBuf>`
- keep existing single-repo fields for compatibility

11. Update `bin/src/main.rs`
- add `--project <PROJECT_FILE>` CLI flag
- pass through `AppConfig`

12. Update `lib/src/application/runner.rs`
- startup branch:
  - single repo (existing path)
  - project mode (init project state + start onboarding workers)
- attach onboarding event receiver to app state

13. Update `lib/src/application/state/mod.rs`
- add project repositories collection with status/progress/error fields
- add active repo id and switching helpers
- add onboarding event polling

14. Update `lib/src/application/state/actions.rs`
- add actions for project navigation and onboarding commands

15. Update `lib/src/application/state/keymap.rs`
- map keys for project actions (`P` open project view, `o` queue, `c` cancel, `R` retry, `s` switch)

### D. TUI Layer

16. Add `lib/src/tui/render/project_panel.rs`
- project repositories list + status badges + progress + error summary

17. Update `lib/src/tui/render/mod.rs`
- include project panel/screen route

18. Update `lib/src/tui/render/overlays.rs`
- help and status text for new project/onboarding commands

### E. Existing Multi-Repo Mapping Integration

19. Update `lib/src/infrastructure/workspace.rs`
- keep `merge_workspace_graphs`
- add helpers for per-repo graph refresh and merged graph update strategy when active repo changes

20. Update `lib/src/application/query.rs` (if needed)
- ensure query references resolve within active repo scope by default

### F. Tests

21. Add `lib/tests/project_onboarding.rs`
- end-to-end onboarding queue/switch/cancel/retry flows

22. Add unit tests near new modules:
- `lib/src/domain/project.rs`
- `lib/src/application/project/scheduler.rs`
- `lib/src/infrastructure/project_store.rs`

23. Update/extend snapshot tests:
- `lib/src/tui/render/mod.rs` tests to include Project View rendering

### G. Documentation

24. Update `docs/index.md`
- link this spec

25. Update `docs/onboarding/getting-started.md`
- add project-file startup and project mode workflow

26. Add ADR: `docs/adrs/ADR-011-project-view-and-async-onboarding.md`
- rationale, alternatives, persistence location decision


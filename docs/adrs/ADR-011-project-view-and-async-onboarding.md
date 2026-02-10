# ADR-011: Project View and Async Onboarding

Date: 2026-02-10
Status: Accepted

## Context
Canopy startup previously mapped repositories in the foreground. During onboarding, users could not continue working in repositories that were already mapped and ready.

The product direction requires:
- managing multiple repositories from one project manifest
- running onboarding in the background
- switching active repository context without leaving the session

## Decision
Adopt a project orchestration model with workspace-level runtime state and background scheduling:

1. Introduce a project manifest (`--project <PROJECT_FILE>`) with repository entries.
2. Persist project runtime onboarding status in `.canopy-project/project_state.json` next to the manifest.
3. Add a bounded-concurrency scheduler for queue/cancel/retry onboarding commands.
4. Keep repository artifacts in existing repository-local `.canopy/` storage.
5. Add a dedicated Project View in TUI and active repository switching (`P`, `o`, `c`, `R`, `s`).

## Alternatives Considered
- Foreground sequential onboarding only:
  rejected because it blocks productive work during long onboarding runs.
- Move all data into a single workspace-level cache:
  rejected because it breaks existing repository-local persistence contract and portability.
- Keep only workspace graph merge without project state:
  rejected because merge output does not model long-running onboarding job state or control actions.

## Consequences
- Multi-repository onboarding is non-blocking with explicit queue state.
- Existing single-repository startup remains backward compatible.
- Project runtime state is durable across restarts and can drive Project View rendering.
- Scheduler logic and UI state handling add complexity that must be covered by integration tests.

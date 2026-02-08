# ADR-005: Progressive Loading and Background Work

Date: 2026-02-07
Status: Accepted

## Context
MVP needs fast time-to-interactive on large repositories.

## Decision
Render architecture tree immediately from structural inference and start semantic inference in a
background worker.

The TUI receives inference progress events and hydrates node summaries incrementally while
remaining interactive.

Cache summaries in SQLite with deterministic keys (`repo_hash + node_id + prompt_digest`).

## Consequences
- Faster perceived startup.
- Stable performance across sessions due to cache reuse.

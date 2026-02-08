# ADR-003: Persistence Contract in .canopy

Date: 2026-02-07
Status: Accepted

## Context
Canopy requires local-first durability for architecture and provenance.

## Decision
Persist under repository-local `.canopy/`:
- `c4_model.json`
- `edit_log.jsonl`
- `query_history.jsonl`
- `cache.db` (SQLite)

`edit_log.jsonl` is append-only and stores timestamp, author, component path, before/after, reason, and provenance.

## Consequences
- Strong local data ownership.
- Durable audit trail for human edits.

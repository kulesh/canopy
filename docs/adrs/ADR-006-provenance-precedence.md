# ADR-006: Provenance Conflict Rules

Date: 2026-02-07
Status: Accepted

## Context
Human corrections must remain authoritative.

## Decision
- Human-edited summaries override AI output by default.
- Regeneration of human-edited nodes requires explicit confirmation.
- Every overwrite action is logged in `edit_log.jsonl`.

## Consequences
- Clear provenance semantics.
- Full auditability of semantic-layer evolution.

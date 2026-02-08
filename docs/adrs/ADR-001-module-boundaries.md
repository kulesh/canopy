# ADR-001: Module Boundary Strategy

Date: 2026-02-07
Status: Accepted

## Context
Canopy must move from a scaffold to a maintainable architecture while preserving delivery speed.

## Decision
Keep the workspace layout (`lib`, `bin`) and establish explicit internal boundaries in `canopy-lib`:
- `domain`
- `application`
- `inference`
- `infrastructure`
- `tui`

## Consequences
- Fast iteration without crate-fragmentation overhead.
- Clear migration path to multi-crate split if modules harden.

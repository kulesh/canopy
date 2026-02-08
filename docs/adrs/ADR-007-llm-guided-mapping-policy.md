# ADR-007: LLM-Guided C4 Mapping Policy

Date: 2026-02-08
Status: Accepted

## Context
Hardcoded file-to-component mapping rules produced brittle C4 structures and low trust in architectural output.

## Decision
Use an LLM-generated mapping policy as the primary architecture mapping input:
- Build a source-tree snapshot (files/directories).
- Prompt provider with repository purpose and strict JSON contract.
- Parse and validate full-file coverage policy.
- Deterministically materialize C4 nodes from policy.
- Persist policy in `.canopy/mapping_policy.json` with provider/model provenance.
- Fall back to deterministic heuristic mapper when provider/policy fails.

## Consequences
- Mapping reflects repository purpose and semantics, not filename heuristics.
- Deterministic execution preserves graph invariants and repeatability.
- Offline/failed-provider mode remains functional through fallback mapper.

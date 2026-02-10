# ADR-012: Strict Model-First Mapping

## Status
Accepted

## Context
Canopy drifted into hybrid behavior where local heuristics inferred component names, dependencies, and fallback summaries when provider output was unavailable. This conflicts with the product philosophy that Canopy must be a harness: orchestration locally, extraction/interpretation/selection by the model.

## Decision
Adopt strict model-first mapping as the default execution mode.

### Invariants
- Architecture mapping requires a model-generated policy.
- Container/component naming and dependency edges come from policy payload only.
- Local code performs orchestration, persistence, validation, and rendering only.
- Missing provider in strict mode blocks onboarding instead of producing heuristic architecture.
- Local semantic fallbacks are allowed only in explicit legacy mode.

### Transitional Escape Hatch
- `settings.mapping_execution_mode = "legacy_hybrid"` is retained temporarily for migration/testing workflows.
- Default mode is `strict_model`.

## Consequences
- Better alignment with product philosophy and traceability.
- Stronger provider dependency for fresh onboarding in strict mode.
- Existing tests and fixtures that relied on heuristic mapping must supply policy payloads.
- Policy schema and validation become stricter (including explicit component dependency edges).

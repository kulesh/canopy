# ADR-010: Claude Agent SDK Harness Integration

Date: 2026-02-08
Status: Accepted

## Context
Canopy has a harness abstraction (`HarnessAdapter`) plus native LLM provider integrations. We need Claude Agent SDK integration without exposing low-level transport choices to users.

## Decision
Adopt direct SDK embedding inside Canopy harness orchestration:

1. Keep harness architecture as the stable orchestrator.
2. Use Claude SDK directly inside harness implementation for mapping-policy generation.
3. Remove runtime harness-selection toggles and bridge-command controls from CLI and env surface.
4. Preserve automatic fallback to native LLM harness when SDK policy generation fails and provider keys exist.

## Consequences
- Program/harness/environment responsibilities remain explicit.
- User experience is simpler: no backend selection or bridge command setup.
- Integration surface is smaller and easier to test deterministically.

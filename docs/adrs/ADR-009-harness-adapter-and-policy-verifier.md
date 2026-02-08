# ADR-009: Harness Adapter and Policy Verifier Loop

Date: 2026-02-08
Status: Accepted

## Context
Canopy previously called `LlmProvider` directly from the runner for mapping policy generation. This coupled orchestration logic to provider calls and made it harder to evolve toward SDK-backed harnesses.

We also needed a stronger verification loop that checks model output quality beyond JSON parse/shape validation.

## Decision
Introduce an application-level harness abstraction:
- `HarnessAdapter` trait for model-to-environment orchestration.
- `LlmHarnessAdapter` as the current implementation.

`LlmHarnessAdapter` now owns mapping policy generation with bounded retries and two-stage verification:
1. Structural verification: parse + full-file coverage + component quality constraints.
2. Model-backed verification: a dedicated verifier prompt that returns JSON verdict (`valid`, `issues`).

If either verification stage fails and attempts remain, Canopy sends a repair prompt with failure details and retries. Exhaustion returns a typed error and the runner falls back to deterministic mapping.

## Consequences
- Clear separation between Program orchestration and provider implementation.
- Extensible path for future harness adapters (OpenAI Agents SDK, Claude Agent SDK).
- Better resilience and quality for C4 mapping policies before persistence.

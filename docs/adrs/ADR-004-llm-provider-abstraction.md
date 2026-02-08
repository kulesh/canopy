# ADR-004: LLM Provider Abstraction

Date: 2026-02-07
Status: Accepted

## Context
Canopy needs BYOK and provider flexibility.

## Decision
Define `LlmProvider` trait with request-driven `complete(CompletionRequest)` and `model_info()`; implement:
- Anthropic (`ANTHROPIC_API_KEY`)
- OpenAI (`OPENAI_API_KEY`)

Selection order: Anthropic first, then OpenAI. If neither key exists, run local fallback summaries.

## Consequences
- Stable extension point for future providers.
- Zero-config startup still works in degraded AI mode.
- Harness orchestration remains decoupled from provider wiring behind `HarnessAdapter`.

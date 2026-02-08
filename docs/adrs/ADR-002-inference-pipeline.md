# ADR-002: Inference Pipeline and Confidence

Date: 2026-02-07
Status: Accepted

## Context
We need deterministic behavior with optional LLM augmentation and graceful fallback.

## Decision
Use a 3-pass pipeline:
1. Structure pass (repository mapper)
2. Dependency pass (relationship inference)
3. Synthesis pass (summary generation)

Confidence model:
- Cached result: `0.92`
- LLM response: `0.86`
- Local heuristic summary: `0.55`

## Consequences
- Predictable behavior without provider lock-in.
- Users always receive summaries even without API keys.

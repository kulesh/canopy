# ADR-008: Prompt Contracts and Repair Loop

Date: 2026-02-08
Status: Accepted

## Context
Canopy depends on LLM output for two critical tasks:
- C4 mapping policy generation (`mapping_policy.json`)
- semantic node summaries

Previous prompting sent only a single user message with weak task contracts. Failures manifested as:
- structurally invalid JSON responses
- low-signal component naming (for example placeholder components)
- inconsistent summaries with limited architectural context

This reduced reliability and increased fallback frequency.

## Decision
Adopt Prompting v2 with explicit request contracts and provider role separation.

1. Introduce typed completion requests (`CompletionRequest`) with:
- task identity (`PromptTask`)
- role-separated prompts (`system_prompt`, `user_prompt`)
- explicit response shape (`ResponseFormat::Text | JsonObject`)
- generation controls (`max_tokens`, `temperature`)

2. Strengthen mapping policy prompt semantics:
- explicit C4 interpretation rules
- strict include/exclude contract for every listed source file
- prohibition on placeholder component naming

3. Add policy generation repair loop:
- validate output via parse + policy checks
- on failure, issue a correction prompt containing validation error and prior invalid output
- retry bounded attempts before deterministic fallback

4. Enrich summary prompt context:
- repository purpose
- node lineage
- dependency/dependent context
- strict concise plain-text format requirements

## Consequences
- Higher first-pass validity for mapping policy outputs.
- Deterministic recovery path when model output is invalid.
- Better summary quality and consistency with architecture goals.
- Provider abstraction now carries prompt intent and output contract, enabling future harness adapters.

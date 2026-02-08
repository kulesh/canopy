# Claude Agent SDK Harness

Canopy uses a built-in Claude SDK harness for mapping-policy generation.

## Behavior
- The harness generates mapping policy from repository tree snapshot + purpose.
- The model may inspect repository files with tool calls (Read/Grep/Glob/Bash) before finalizing mappings.
- Policy output is validated structurally against repository files.
- Failed validation triggers bounded repair retries with explicit feedback.
- A verifier pass checks policy quality before acceptance.
- Startup logs show policy phases, attempts, and tool-call activity while C4 mapping is being built.

## Fallback
- If SDK-based policy generation fails and provider keys are configured, Canopy falls back to the native LLM harness.
- If no provider keys are configured, Canopy falls back to deterministic mapping.

## User Inputs
- `--purpose` or `CANOPY_PURPOSE` to steer architectural intent.
- No harness-selection or bridge-command flags are required.

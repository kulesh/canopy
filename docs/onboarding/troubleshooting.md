# Troubleshooting

## No AI summaries
Set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` and restart.

## Policy mapping fallback triggered
- Canopy falls back to deterministic mapping when policy generation fails.
- Canopy first performs bounded repair retries using validation feedback before fallback.
- If Claude SDK policy generation fails and provider keys are configured, Canopy falls back to the native LLM harness.
- Check status line for fallback reason.
- Verify model access and API key validity.
- Inspect `.canopy/logs/diagnostics.jsonl` for phase, tool-call, verification, and repair events.

## Slow startup
- First load builds architecture, policy, and cache.
- Subsequent runs reuse `.canopy/cache.db`, `.canopy/c4_model.json`, and `.canopy/mapping_policy.json`.

## TUI rendering issues
- Ensure terminal supports alternate screen and raw mode.
- Retry with `TERM=xterm-256color`.

## Corrupted local cache
Delete `.canopy/cache.db` and restart.

## Stale architecture graph
Delete `.canopy/c4_model.json` and `.canopy/mapping_policy.json` then restart.

## Missing diagnostics file
- Diagnostics are created after repository discovery. If missing, verify write permissions under `.canopy/`.

# Troubleshooting

## No AI summaries
Set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` and restart.

## Policy mapping fallback triggered
- Canopy falls back to deterministic mapping when policy generation fails.
- Check status line for fallback reason.
- Verify model access and API key validity.

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

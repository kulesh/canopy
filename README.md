# canopy

Canopy is an AI-native terminal interface for understanding codebases as architecture instead of files.

## Docs
- Docs index: `docs/index.md`
- Product idea: `docs/specs/idea-brief-20260206-033006.md`
- Product specification: `docs/specs/product-spec-20260206-033853.md`
- MVP scope lock: `docs/specs/mvp-scope-lock.md`
- Implementation plan: `docs/implementation/canopy-implementation-plan-20260207.md`
- ADRs: `docs/adrs/`
- Testing strategy: `docs/testing/strategy.md`
- Onboarding: `docs/onboarding/getting-started.md`
- Troubleshooting: `docs/onboarding/troubleshooting.md`
- MVP release checklist: `docs/releases/mvp-release-checklist.md`

## Quick Start
```bash
mise install
cargo build
cargo run -- .
```

## FAQ
- Where is Canopy data stored?
  - In `<repo>/.canopy/` (`c4_model.json`, `mapping_policy.json`, `edit_log.jsonl`, `query_history.jsonl`, `cache.db`).
- Can I steer architecture inference?
  - Yes. Use `--purpose` or `CANOPY_PURPOSE` to provide architectural intent for policy generation.
- How do I enable AI summaries?
  - Export `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`.
- Does Canopy work without API keys?
  - Yes. It runs with local heuristic summaries and query fallback.

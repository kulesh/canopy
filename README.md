# canopy

Canopy is an AI-native terminal interface for understanding codebases as architecture instead of files.

## Docs
- Docs index: `docs/index.md`
- Product idea: `docs/specs/idea-brief-20260206-033006.md`
- Product specification: `docs/specs/product-spec-20260206-033853.md`
- MVP scope lock: `docs/specs/mvp-scope-lock.md`
- Implementation plan: `docs/implementation/canopy-implementation-plan-20260207.md`
- Architecture diagrams: `docs/architecture/ai-engineering-and-c4-pipeline.md`
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
- What improves mapping reliability when model output is malformed?
  - Canopy runs a validation + repair retry loop before falling back to deterministic mapping.
- How is model orchestration structured?
  - Through a harness adapter layer (`HarnessAdapter`) so provider calls and verification loops are decoupled from app orchestration.
- How does Claude Agent SDK fit into policy generation?
  - Mapping-policy generation uses an internal Claude SDK harness path by default.
  - The model can inspect repository files using tool calls before returning the final mapping policy JSON.
  - Startup logs show policy-build progress phases and tool-call activity.
  - If SDK policy generation fails and provider keys are configured, Canopy falls back to the native LLM harness.
- Does Canopy work without API keys?
  - Yes. It runs with local heuristic summaries and query fallback.

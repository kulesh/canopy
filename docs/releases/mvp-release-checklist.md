# MVP Release Checklist

- [x] `cargo fmt -- --check` passes
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [x] `cargo test --workspace` passes
- [x] `cargo nextest run --workspace` passes
- [x] `cargo bench -p canopy-lib --no-run` passes
- [x] CI workflow present (`.github/workflows/ci.yml`)
- [x] Core docs linked from README
- [x] `.canopy/` persistence contract implemented
- [x] `.canopy/mapping_policy.json` persisted when policy generation succeeds
- [x] BYOK provider selection implemented
- [x] Edit log export command implemented
- [x] Golden-set and language-coverage tests added

## Manual pre-tag checks
- [ ] Validate TUI behavior on macOS and Linux terminals
- [ ] Verify API providers with real keys
- [ ] Run benchmark on representative 100K LOC repository

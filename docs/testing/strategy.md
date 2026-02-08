# Testing Strategy

## Layers
- Unit tests for domain invariants and state transitions.
- Integration tests for repository mapping, query behavior, persistence.
- Contract tests for policy parsing/validation and policy-to-graph execution.
- Prompt contract tests for task instruction integrity and response-format constraints.
- Snapshot-style rendering checks for TUI panel outputs.
- End-to-end CLI startup checks.
- Golden-set architecture evaluation for inference regressions.

## Mandatory Gates
- `cargo fmt -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cargo nextest run --workspace`
- `cargo bench -p canopy-lib --no-run`

## Property Invariants
- Every dependency edge references an existing node.
- Graph root is always present.
- Dependents index can be rebuilt deterministically.

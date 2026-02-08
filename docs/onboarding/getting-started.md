# Getting Started

## Prerequisites
- `mise`
- Rust toolchain (managed by `.mise.toml`)

## Setup
```bash
mise install
cargo build
cargo test
```

## Run
```bash
# Analyze current repository
cargo run -p canopy-bin -- .

# Analyze an explicit repository path
cargo run -p canopy-bin -- /path/to/repo

# Analyze with explicit architectural purpose
cargo run -p canopy-bin -- --purpose "Understand auth and billing boundaries" /path/to/repo
```

## Optional AI configuration
```bash
export ANTHROPIC_API_KEY=...
# or
export OPENAI_API_KEY=...

# Optional: default purpose for mapping policy generation
export CANOPY_PURPOSE="Understand architecture for safe refactoring"
```

Without keys, Canopy uses fallback mapping and local summaries.

## Startup model
- Architecture tree loads first.
- Semantic summaries hydrate progressively in the background.
- Progress appears in the status line during inference.

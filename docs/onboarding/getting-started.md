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

# Start in project mode from a manifest file
cargo run -p canopy-bin -- --project /path/to/project.toml
```

## Project manifest
Create a TOML project manifest that lists repositories:

```toml
name = "workspace-demo"
active_repository_id = "repo-a"

[settings]
mapping_execution_mode = "strict_model"
policy_mode = "auto"
refinement_mode = "on"
source_index_mode = "snapshot"
onboarding_concurrency = 1
incremental_max_files = 120
refine_cache_hit = false

[[repositories]]
id = "repo-a"
name = "repo-a"
path = "repo-a"

[[repositories]]
id = "repo-b"
name = "repo-b"
path = "repo-b"
```

Runtime onboarding status is persisted in `.canopy-project/project_state.json` next to the manifest file.

## AI configuration
```bash
export ANTHROPIC_API_KEY=...
# or
export OPENAI_API_KEY=...

# Optional: default purpose for mapping policy generation
export CANOPY_PURPOSE="Understand architecture for safe refactoring"
```

In strict mode, keys are required for fresh onboarding policy generation.

### Processing options are manifest-based
Tune onboarding and refinement behavior in `[settings]` inside `project.toml`:

```toml
[settings]
# strict_model | legacy_hybrid
mapping_execution_mode = "strict_model"

# auto | off
policy_mode = "auto"

# on | off
refinement_mode = "on"

# snapshot | daemon
source_index_mode = "snapshot"

# number of concurrent onboarding workers (>= 1)
onboarding_concurrency = 1

# changed-file threshold for incremental fast-map (>= 1)
incremental_max_files = 120

# whether to run refinement even when tree-hash cache hits
refine_cache_hit = false
```

## Startup model
- TUI launches immediately.
- Repository onboarding runs in the background and streams status into Project View.
- Ready repositories can be switched without restarting the session.
- Progress appears in the status line and Project View.
- Strict mode onboarding uses git-tree-hash cache hits, then provider-driven policy generation and policy-backed graph mapping.

## Project mode workflow
- Press `P` to open Project View.
- `o`: queue onboarding for selected repository.
- `c`: cancel selected onboarding.
- `R`: retry selected repository onboarding.
- `s` or `Enter`: switch active repository (ready repositories only).

## Diagnostics
- Structured startup/harness diagnostics are written to `.canopy/logs/diagnostics.jsonl`.

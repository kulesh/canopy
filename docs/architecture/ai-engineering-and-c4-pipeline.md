# AI Engineering and C4 Pipeline

This document describes how Canopy applies the Program/Harness/Environment/Model framing and how source code becomes an executable C4 model.

## Program, Harness, Environment, Model

```mermaid
flowchart LR
    Human[Human Intent]
    Program[Program\nCLI + AppState + Runner]
    Harness[Harness\nPolicy generation + verify + repair loop]
    Model[Model\nClaude SDK / Provider LLM]
    Env[Environment\nRepository files + git + persistence]

    Human --> Program
    Program --> Harness
    Harness <--> Model
    Harness <--> Env
    Program --> Env
    Program --> Human
```

## C4 Mapping Construction Pipeline

```mermaid
flowchart TD
    A[Discover repository] --> B[Collect source tree snapshot]
    B --> C[Prompt model with purpose + mapping contract]
    C --> D{Model requests tools?}
    D -->|Yes| E[Tool calls\nRead / Grep / Glob / Bash]
    E --> C
    D -->|No or finished| F[Receive policy JSON]
    F --> G[Parse + validate policy invariants]
    G -->|Invalid| H[Repair prompt with validation feedback]
    H --> C
    G -->|Valid| I[Verifier pass]
    I -->|Invalid| H
    I -->|Valid| J[Persist .canopy/mapping_policy.json]
    J --> K[Deterministically materialize C4 graph]
    K --> L[Persist .canopy/c4_model.json]
```

## Accuracy Rules

1. Mapping is content-informed, not filename-only.
2. Every source file must be explicitly included or excluded by policy.
3. Policy output must satisfy structural and semantic validation before acceptance.
4. Deterministic graph materialization preserves repeatability after policy approval.

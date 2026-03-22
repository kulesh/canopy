# Plan: Close Phase 2 Gap via Plugin System

## Problem

The architecture scanning workflow (system prompt instructions, scan trigger, change proposals, re-scan) is hardcoded across `session.ts` and `main.ts`. This makes it non-pluggable — adding a second workflow requires editing both files.

## Solution

Evolve `ToolPlugin` → `Plugin`. A plugin can provide tools, system prompt fragments, and named skills. The registry composes prompts and exposes skills by name.

## Implementation Steps

### Step 1: Evolve Plugin interface and registry

**File: `web/src/agent/tools.ts`**

- Rename `ToolPlugin` → `Plugin`
- Add optional `systemPrompt?(ctx): string` to Plugin
- Add optional `skills?(ctx): Skill[]` to Plugin
- Define `Skill` interface: `{ id, label, prompt(params) → string }`
- Rename `ToolRegistry` → `PluginRegistry`
- Add `PluginRegistry.systemPrompt(ctx): string` — joins all active plugin prompts
- Add `PluginRegistry.skill(id): Skill | undefined` — lookup by ID
- Update `createRegistry()` to use new names
- Rename file from `tools.ts` → `plugins.ts`

### Step 2: Create architecture plugin

**File: `web/src/agent/plugins/architecture.ts`**

- Move scanning strategy, notebook format, change proposal format from `session.ts` into `systemPrompt()`
- Define three skills:
  - `scan-architecture`: prompt template for initial scan (currently in `openProject()`)
  - `propose-changes`: prompt template for cell edit proposals (currently in `requestCellChangeProposal()`)
  - `rescan-components`: prompt template for re-scan after dismissals (currently in `requestRescan()`)
- `available()`: always true (skills are useful even without a project open — agent can explain why it needs one)
- `tools()`: returns `[]` (no tools, just skills and prompts)

### Step 3: Slim down session.ts

**File: `web/src/agent/session.ts`**

- `buildSystemPrompt()` keeps the base identity/hierarchy/rules
- Appends `registry.systemPrompt(ctx)` for plugin-contributed instructions
- Remove scanning strategy, notebook format, change proposal format (moved to architecture plugin)
- `CreateAgentOptions` already has `registry` — use it to compose the prompt

### Step 4: Wire main.ts to use skills

**File: `web/src/main.ts`**

- `openProject()`: replace hardcoded prompt with `registry.skill("scan-architecture")?.prompt({ projectName })`
- `requestCellChangeProposal()`: replace with `registry.skill("propose-changes")?.prompt({ cell, edit })`
- `requestRescan()`: replace with `registry.skill("rescan-components")?.prompt({ names })`
- Update imports: `ToolContext` → `PluginContext`, `createRegistry` stays

### Step 5: Update filesystem plugin

**File: `web/src/agent/plugins/filesystem.ts`**

- Update import: `ToolPlugin` → `Plugin`
- No other changes — it only provides tools

### Step 6: Run tests, fix breakage

- `cargo nextest run` (Rust side, shouldn't be affected)
- `npx vitest run` (TypeScript tests)
- `npx tsc --noEmit` (type checking)
- Fix any import/reference breakage from the rename

## Result

After this change:
- `session.ts` is a thin shell: base prompt + registry-composed fragments
- `main.ts` dispatches lifecycle events through named skills, not hardcoded strings
- `plugins/filesystem.ts` provides tools (unchanged)
- `plugins/architecture.ts` provides system prompt + 3 skills (new)
- Adding a new workflow = dropping a new file in `plugins/`

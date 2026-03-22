# Plan: Close Phase 3 Gap — Agent Proposal Round-Trip

## Problem

The Phase 3 wiring exists: edit → event → skill prompt → agent.prompt(). The parsing exists: canopy-changes fence → ChangeSet → store. The rendering exists: change cards with diffs.

But the round-trip has never been exercised with a live agent. Two specific issues:

1. **The propose-changes skill is implicit.** It says "propose changes" but doesn't explicitly instruct the agent to emit a `canopy-changes` fence. It relies on the system prompt having the format spec. If the system prompt is truncated or the agent ignores it, no structured output is produced.

2. **No live test.** The Phase 2 live test scans architecture but stops there. Phase 3 — edit a cell, get a structured proposal back, parse it, load it — is untested with a real agent.

3. **Escaped newline bug.** UX issue #8: `before`/`after` snippets render literal `\n` instead of actual line breaks.

## Solution

Three focused changes:

### Step 1: Make the propose-changes skill explicit

**File: `web/src/agent/plugins/architecture.ts`**

The propose-changes skill prompt should explicitly tell the agent to emit a `canopy-changes` fence. Don't rely solely on the system prompt — make the instruction part of the skill invocation itself. Same treatment for rescan-components (explicitly ask for `canopy-notebook` fence).

This is a one-line addition to each skill's prompt template. No structural change.

### Step 2: Fix escaped newlines in change proposal rendering

**File: `web/src/notebook/cell.ts`**

The `renderChangeProposal()` function renders `before`/`after` snippets inside `<pre>` tags. If the agent emits literal `\n` in JSON strings, they become real newlines after JSON.parse — so this might be a non-issue. But if the agent emits `\\n` (escaped escape), the renderer shows literal `\n`.

Investigate and fix: ensure snippets render with actual line breaks in the `<pre>` block.

### Step 3: Live agent test for the proposal round-trip

**File: `web/tests/live-agent.test.ts`**

Extend the existing live test with a second test case:

1. Reuse the notebook from the Phase 2 scan (or scan fresh)
2. Pick a component cell (e.g., "Plugin Registry")
3. Construct a propose-changes skill prompt with an edited summary
4. Send to agent
5. Wait for response
6. Verify the agent emits a `canopy-changes` fence
7. Parse it, verify it targets the edited cell
8. Load into the store, verify `changeFor(cellId)` returns a proposal
9. Verify proposal has concrete file changes

This proves the full round-trip: edit intent → agent analysis → structured response → parsed → stored.

### Step 4: Unit tests for skill prompt construction

**File: `web/tests/plugins.test.ts`**

The architecture plugin skills already have basic tests. Add assertions that:
- propose-changes prompt mentions `canopy-changes` (after Step 1)
- rescan-components prompt mentions `canopy-notebook` (after Step 1)

## What's NOT in scope

- **Playwright/browser testing** — deferred per earlier decision
- **Re-scan round-trip test** — would require modifying files and re-scanning. The rescan skill is structurally identical to the scan skill (already proven). Testing it live adds cost without proportional value.
- **Rendering tests** — cell rendering is HTML template code; testing it requires jsdom/Playwright. The escaped newline fix (Step 2) will be verified by visual inspection or deferred to the Playwright phase.

## Execution order

1. Step 1 (explicit skill prompts) — changes the prompts
2. Step 4 (unit tests) — verifies the prompt changes
3. Step 2 (newline fix) — rendering fix
4. Step 3 (live test) — proves the round-trip

Steps 1-3 are fast. Step 4 (live test) takes ~90s per run.

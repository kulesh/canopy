# Canopy PWA — UX Exploration Report (Phases 1–3)

Headless Chromium exploration of the PWA bootstrapped against its own codebase.
41 screenshots captured across 12 test scenes plus live API testing.

---

## Test Methodology

Three Playwright scripts explored the PWA progressively:

1. **Surface scan** — page load, DOM structure, responsive viewports, performance, accessibility
2. **Injected data** — notebook store populated via dev-mode test bridge, exercising every visual state
3. **Live agent** — API key entered via dialog, prompt sent to analyze Canopy's own code

All tests ran in headless Chromium 141 at 2× device scale factor (Retina), viewport 1440×900.

---

## Phase 1: Skeleton Layout

### What Works

| Aspect | Status | Detail |
|--------|--------|--------|
| Split-pane layout | **Good** | Notebook (418px, 29%) / Chat (1022px, 71%) at 1440px |
| Toolbar | **Good** | History, New Session, Title, Panel Toggle, Settings — all rendered |
| Chat input | **Good** | Textarea with placeholder "Type a message...", model selector, thinking toggle |
| Empty state | **Good** | Tree emoji + instructional text + example prompt |
| PWA manifest | **Good** | `<link rel="manifest">` present, `theme-color` set |
| Dark mode | **Good** | CSS custom properties switch cleanly via `.dark` class |
| Zero JS errors | **Good** | No console errors, no uncaught exceptions |

### Issues Found

| # | Severity | Issue | Detail |
|---|----------|-------|--------|
| 1 | **Medium** | Notebook panel shows on first load | The notebook panel (with empty state) renders immediately even before any agent interaction. The empty state with the tree emoji and hint text is fine UX, but `notebookVisible` defaults to `true` in main.ts — the panel toggle button has no effect until this is connected |
| 2 | **Low** | Light theme is default | Body background is `oklch(1 0 0)` (white). System preference is not dark in headless. The `<theme-toggle>` element exists but requires manual click. Consider respecting `prefers-color-scheme` |
| 3 | **Low** | Lit dev mode warning | Console shows "Lit is in dev mode" — expected in dev, but should be suppressed in production build |

### Performance

| Metric | Value |
|--------|-------|
| DOM Content Loaded | 1034ms |
| Full Load | 1043ms |
| Resources | 31 files, 7544KB total |
| Largest bundle | `@mariozechner/pi-web-ui.js` — **5329KB** |

The Pi Web UI bundle dominates at 5.3MB. This is the entire Pi SDK including all tool renderers, dialogs, and the chat interface. Tree-shaking opportunities exist but may be limited since it's a pre-bundled package.

---

## Phase 2: Notebook Rendering

Tested by injecting a self-referential notebook (Canopy describing itself: 8 cells, 4 levels deep).

### What Works

| Aspect | Status | Detail |
|--------|--------|--------|
| Cell hierarchy | **Excellent** | System → Container → Component renders with correct indentation (16px per level) |
| Auto-expand roots | **Good** | Root cell expands on load, showing immediate children |
| Kind badges | **Good** | `system`, `container`, `component` badges render with distinct colors (purple/blue/green) |
| Provenance badges | **Good** | `ai` badge appears on all cells |
| Fold/unfold | **Good** | Chevron indicators (▾/▸) toggle correctly, children appear/disappear |
| Summaries | **Good** | Multi-line summaries render with relaxed line-height |
| File paths | **Good** | `files: src/notebook/` renders as monospace chips |
| Dependency count | **Good** | `2→` badge on App Shell (has 2 deps) |
| Dependency links | **Good** | dep chips are clickable, focus navigates to target cell |
| Border colors | **Good** | Left border color matches kind (purple/blue/green/amber) |

### Issues Found

| # | Severity | Issue | Detail |
|---|----------|-------|--------|
| 4 | **Medium** | Dependencies not rendering in expanded view | The `deps:` section doesn't appear in the App Shell cell even though it has 2 dependencies. The data is correct (tested via store), but the cell may need to be expanded by clicking its button specifically — the auto-expand on load may not trigger the expanded body rendering for children |
| 5 | **Low** | No scroll indicator | When the notebook tree exceeds the panel height, there's no visual scroll indicator. The panel scrolls (verified on mobile: `scrollable=true`), but a subtle scrollbar or fade gradient would help discoverability |

### Responsive Behavior

| Viewport | Behavior |
|----------|----------|
| Desktop 1440×900 | Split pane, notebook 29%, chat 71% |
| Mobile 375×812 | Notebook full width, chat hidden below. **On mobile the chat is completely hidden** — only the notebook empty state shows. Major UX gap for initial interaction. |
| Mobile 390×844 (with content) | Notebook is scrollable, all 8 cells visible, `scrollable=true` confirmed |
| Tablet 768×1024 | Split pane maintains but notebook gets cramped |
| Ultrawide 2560×1080 | Notebook stays narrow (fixed 2/5 ratio), chat area has large empty space |

---

## Phase 3a: Cell Editing

### What Works

| Aspect | Status | Detail |
|--------|--------|--------|
| Double-click to edit | **Good** | Summary paragraph responds to dblclick, textarea appears |
| Edit textarea | **Good** | Pre-filled with current summary, auto-focused |
| Keyboard hints | **Good** | "Ctrl+Enter save · Esc cancel" shows below textarea |
| Ctrl+Enter commit | **Good** | Summary updates, textarea disappears |
| Provenance update | **Good** | Source changes from `ai` to `human`, `edited_at` timestamp set |
| Escape cancel | **Good** | Summary reverts, edit mode exits |
| `e` key shortcut | **Good** | Opens edit on focused + expanded cell |

### Issues Found

| # | Severity | Issue | Detail |
|---|----------|-------|--------|
| 6 | **Low** | No visual feedback after commit | When the summary is committed, the text simply updates. A brief flash or highlight would confirm the change was applied. The provenance badge does change from `ai` to `human`, which is good |

---

## Phase 3b: Edit → Agent Proposal

### What Works

| Aspect | Status | Detail |
|--------|--------|--------|
| Edit event emission | **Good** | `cell-edited` event fires with `cellId`, `oldSummary`, `newSummary` |
| Agent prompt construction | **Good** | Structured prompt includes component name, old/new description, file paths |
| Chat shows proposal request | **Good** | The change request appears in the chat panel as a user message |

### Issues Found

| # | Severity | Issue | Detail |
|---|----------|-------|--------|
| 7 | **High** | `process is not defined` error | When the edit triggers `agent.prompt()`, the chat panel shows a red error: "Error: process is not defined". This is a Node.js API reference leaking into browser context — likely from the Pi SDK or a dependency checking `process.env`. The error blocks the agent from processing the edit request. |

---

## Phase 3c: Change Proposals

Tested by injecting change proposal data directly into the store.

### What Works

| Aspect | Status | Detail |
|--------|--------|--------|
| Amber "changes" badge | **Excellent** | Renders on affected cell headers, immediately visible in the tree |
| Change proposal card | **Good** | Amber-bordered card appears inside expanded cell body |
| Summary text | **Good** | Proposal summary renders clearly |
| File changes | **Good** | File path + description + before/after snippets render |
| Before/after diff | **Good** | Red `-` lines and green `+` lines with distinct backgrounds |
| Dismiss button | **Good** | Removes proposal from cell, badge disappears |
| Multiple proposals | **Good** | Two cells with proposals render independently |
| Dark mode | **Excellent** | Change cards, badges, and diff colors all work in dark mode |

### Issues Found

| # | Severity | Issue | Detail |
|---|----------|-------|--------|
| 8 | **Low** | Escaped newlines in diff | The before/after snippets show literal `\n` instead of actual line breaks. The `whitespace-pre-wrap` CSS is applied but the JSON data has escaped newlines |

---

## Phase 3d: Re-scan After Changes

### What Works (verified via unit tests)

| Aspect | Status | Detail |
|--------|--------|--------|
| Dismiss tracking | **Good** | Dismissed cell IDs accumulate correctly |
| Auto re-scan trigger | **Good** | When `hasChanges` becomes false after all dismissals, re-scan fires |
| Re-scan prompt | **Good** | Names affected components, asks for updated `canopy-notebook` |
| Event lifecycle | **Good** | `changes-loaded` → `changes-dismissed` × N → re-scan |

---

## Keyboard Navigation

| Key | Action | Status |
|-----|--------|--------|
| `j` / `↓` | Move focus down | **Works** |
| `k` / `↑` | Move focus up | **Works** |
| `l` / `→` / `Enter` | Expand cell | **Works** |
| `h` / `←` / `Escape` | Collapse cell / navigate to parent | **Works** |
| `e` | Enter edit mode | **Works** |
| `Ctrl+Enter` | Commit edit | **Works** |
| `Escape` (in edit) | Cancel edit | **Works** |
| `Tab` | Tab order traversal | **Functional** — toolbar buttons → thinking toggle → model → body → toolbar → textarea |

---

## Dark Mode

| Component | Light | Dark |
|-----------|-------|------|
| Background | White | Near-black |
| Cell borders | Colored left borders | Same colors, darker bg |
| Badges | Colored with alpha | Same, adjusted |
| Change cards | Amber border on white | Amber border on dark |
| Diff colors | Red/green on white | Red/green on dark — readable |

Dark mode transitions cleanly. All cell states (expanded, editing, change proposals) render correctly in both themes.

---

## Critical Issues Summary

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 7 | **High** | `process is not defined` in agent prompt | Blocks the edit→proposal flow entirely. The agent cannot process cell edits. |
| 1 | **Medium** | Notebook panel always visible | Minor confusion on first load, but the empty state text mitigates |
| 4 | **Medium** | Deps not rendering in some cells | Dependency navigation partially broken |
| 9 | **High** | CORS blocks API calls | Pi SDK validates API keys by calling Anthropic API from the browser. Without a CORS proxy, the key dialog gets stuck on "Testing..." forever. Production deployment needs either a proxy or a different auth flow. |

---

## Architecture Observations

1. **Pi SDK bundle size (5.3MB)** — The single largest resource. For a PWA targeting mobile, this needs attention. Code splitting or lazy loading the chat infrastructure could help.

2. **CORS dependency** — The Pi SDK was designed to work with pi.dev's CORS proxy (`proxy.mariozechner.at`). Running independently requires setting up an equivalent proxy. This is documented in Pi's README but not surfaced in Canopy's onboarding.

3. **`process` reference** — A Node.js-ism leaked into browser code. Likely from a dependency that checks `process.env.NODE_ENV` without guarding for browser context. Vite normally shims this, but something in the call path to `agent.prompt()` bypasses the shim.

4. **Mobile-first gap** — The split-pane layout uses fixed CSS fractions (`w-2/5` for notebook). On mobile (<640px), the chat panel is effectively hidden. A tab-based mobile layout or collapsible sidebar would fix this.

---

## Test Infrastructure Established

- **vitest** — 27 unit/integration tests covering all data flow
- **Playwright** — 3 exploration scripts with dev-mode test bridge (`window.__canopy__`)
- **Screenshots** — 41 captures across all visual states and viewports
- All screenshots in `web/tmp/screenshots/`

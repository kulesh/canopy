# Plan: Bundle Optimization

## Problem

The PWA ships 5.7MB raw across 19 chunks. The critical-path main chunk is 3.3MB raw / 946KB gzipped. For a PWA that's installed once and served from a service worker cache, the raw size matters less than:

1. **First-visit download** — 946KB gzipped is acceptable but not great
2. **Update efficiency** — any code change rebuilds the entire 3.3MB chunk, invalidating the service worker cache entry. Users re-download 946KB on every deploy.
3. **Parse time** — 3.3MB of JavaScript must be parsed on every cold start, even from cache

## Root Cause Analysis

The 3.3MB chunk contains everything because:

- **pi-web-ui** is a Lit component library. Each component calls `customElements.define()` at module scope — a side effect that prevents tree-shaking. Importing one component pulls in all of them.
- **pi-web-ui** has no `sideEffects: false` in package.json (correctly — it does have side effects).
- **No `manualChunks`** config — Vite merges all synchronous imports into one chunk.

The provider modules (Mistral 111KB gz, Anthropic, Google, etc.) are already lazy-loaded by pi-ai's register-builtins mechanism. PDF worker (284KB gz) is also separate. These are fine.

## Compressed sizes (what users actually download)

| Chunk | Raw | Gzipped |
|-------|-----|---------|
| index.js (main) | 3.3MB | 946KB |
| pdf.worker.min.mjs | 1.1MB | 284KB |
| mistral.js | 817KB | 111KB |
| index.css | 85KB | 15KB |
| All other chunks | ~170KB | ~40KB |
| **Total** | **5.7MB** | **~1.4MB** |

## Solution: manualChunks + Compression

Two changes. No lazy-loading gymnastics, no fragile deep imports into pi-web-ui internals.

### Step 1: manualChunks — Split vendor from app code

**File: `web/vite.config.ts`**

Add `build.rollupOptions.output.manualChunks` to split the monolith into stable, cacheable vendor chunks:

```
vendor-pi-ui  → @mariozechner/pi-web-ui + lit + mini-lit  (rarely changes)
vendor-pi-ai  → @mariozechner/pi-ai + pi-agent-core       (rarely changes)
vendor-pdf    → pdfjs-dist                                 (never changes)
app           → everything else                            (changes often)
```

**Why this matters:** When we ship a code change to our agent logic or notebook rendering, only the `app` chunk (~50-100KB gzipped) gets invalidated. The 800KB+ vendor chunks stay cached. Today, changing one line rebuilds and invalidates the entire 946KB.

### ~~Step 2: Compression~~ — SKIPPED

Dropped. Every modern hosting platform (Cloudflare Pages, Netlify, Vercel, GitHub Pages) serves gzip/brotli automatically. Adding a build plugin for this is unnecessary complexity.

### Step 2: Build target — esnext

**File: `web/vite.config.ts`**

Set `build.target: 'esnext'`. The PWA targets modern browsers only (File System Access API, Web Components, IndexedDB). No need for syntax downleveling, which inflates output.

### Step 3: Verify and measure

- Run `npx vite build` and compare chunk sizes before/after
- Confirm vendor chunks are stable across app-only changes
- Confirm service worker precache list updates correctly
- Run `npx vitest run` and `npx tsc --noEmit`

## What this does NOT do

- **Does not lazy-load pi-web-ui components** — the library's side-effect-heavy Lit architecture makes this impractical without upstream changes. ChatPanel (the critical-path component) transitively pulls in most of the library anyway.
- **Does not reduce total download size** — the same bytes ship, just in more cacheable pieces.
- **Does not add a CDN or edge caching** — that's a deployment concern, not a build concern.

## Actual outcome

| Chunk | Raw | Gzipped | Stability |
|-------|-----|---------|-----------|
| vendor-pi-ui | 2,515KB | 763KB | Stable (changes with pi-web-ui upgrades) |
| vendor-pi-ai | 442KB | 80KB | Stable (changes with pi-ai upgrades) |
| vendor-pdf | 404KB | 118KB | Stable (changes with pdfjs upgrades) |
| **index (app)** | **30KB** | **10KB** | **Changes on every deploy** |
| Provider chunks | ~1,200KB | ~210KB | Lazy-loaded, stable |
| PDF worker | 1,050KB | ~284KB | Separate worker thread, stable |

**Update cost: 10KB gzipped** (down from 946KB). 94x improvement.

52 tests pass. Type checks pass. Service worker precaches 24 entries.

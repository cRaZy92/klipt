# Full-app themes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Themes tint the whole app, the accent reaches every place that is purple today, and there are 8 new themes.

**Architecture:** `packages/shared/src/looks.ts` gains `themeTokens(hue, tint)`, which builds the 10 surface/text tokens on the Klipt lightness/saturation ladder. New themes are generated with it; the 4 existing themes keep their hand-tuned tokens, so existing users see no change. `ThemeDef.tokens` stays, so consumers (server account page, tests, `looksTokens`) are untouched. CSS literals are replaced with tokens or `color-mix()`.

**Tech Stack:** TypeScript, Svelte, vitest, plain CSS.

**Spec:** `docs/superpowers/specs/2026-11-26-full-app-themes-design.md`

## Global Constraints
- Dark themes only. Klipt output stays byte-identical to today.
- Free themes: klipt, oled, crimson, forest, ocean, slate. Rewards: views-100 → sand, first-tag → rose, first-combine → synthwave, new-year → arctic; night-owl → midnight, hours-10 → ember unchanged.
- `--rec`, `--success`, `--danger`, `--warning` stay fixed.
- Brand marks are not themed: favicon / `theme-color` meta in `server/src/web/*` stay `#7c5cff`.
- Near-black video scrims `rgba(5, 4, 10, a)` stay (neutral overlays on video).

## Review Focus
- A user whose stored theme is `oled` (previously earned): still valid, still applied.
- A strongly tinted theme + an accent of a clashing hue (Crimson theme + Mint accent): accent text must stay readable. Covered by the existing accent×theme contrast test, which iterates all THEMES.
- Browsers without `color-mix()`: the Tauri WebView (WebView2/WebKitGTK) and the modern browsers the website targets support it. Accepted.
- Website pages must also pick up the new themes: `layout.tsx` already injects `looksTokens`, nothing to add.

---

### Task 1: Theme generator, new themes, rewards

**Files:** Modify `packages/shared/src/looks.ts`, `packages/shared/src/achievements.ts`, `server/test/catalog.test.ts`

**Interfaces:** Produces `themeTokens(hue: number, tint: number): ThemeTokens` (exported). `THEMES` ids: klipt, midnight, oled, ember, crimson, forest, ocean, slate, sand, rose, synthwave, arctic.

- [ ] Tests in `catalog.test.ts`: free themes equal `['klipt','oled','crimson','forest','ocean','slate']`; `themeTokens(250, 1)` is within ±2/255 per channel of Klipt's hand tokens; `slate` tokens are pure greys (r=g=b); every theme has all 10 tokens as `#rrggbb`; rewards map as in Global Constraints; the existing readability tests pass for all themes.
- [ ] Run `pnpm --filter @klipt/server test`, expect FAIL.
- [ ] Implement: ladder `{token: [lightness%, saturation%]}` from Klipt (bg 5.1/23, bg-2 7.45/21, surface 9.6/18.4, surface-2 12.75/20, surface-3 16.9/18.6, border 17.65/20, border-strong 26.1/20.3, text 94.5/28.6, text-muted 67.65/13.9, text-faint 53.3/11.8); saturation × tint, clamped to 100; HSL → hex. Tune each new theme's hue/tint by eye.
- [ ] Run tests, expect PASS. Commit.

### Task 2: Accent reaches everything (desktop + website CSS)

**Files:** the desktop and website CSS hits listed by
`grep -rnE '124, ?92, ?255|#9479ff|#6a4bf0|#8a6dff|#9a81ff|#b18cff|#c9bcff|#d2c8ff|#6d4ff2|#7152f5|20, ?14, ?40|#2a2638' desktop/src server/public`
(outside `:root`). Test: `desktop/src/lib/no-hardcoded-purple.test.ts`.

- [ ] Test: walks `desktop/src` (`.css`, `.svelte`) and `server/public/app.css`, strips each file's `:root { … }` block, and fails listing file:line for any match of the regex above or `#7c5cff`.
- [ ] Run `pnpm --filter desktop test`, expect FAIL with the list.
- [ ] Replace: `rgba(124,92,255,a)` → `color-mix(in srgb, var(--accent) {a*100}%, transparent)`; light-to-dark brand gradients → `linear-gradient(…, var(--accent-hover), var(--accent))`; `#b18cff` → `var(--accent-text)`; `#d2c8ff` → `var(--accent-text)`; `rgba(20,14,40,a)` → `color-mix(in srgb, var(--bg) {a*100}%, transparent)`; drop `var(--x, #fallback)` purple fallbacks.
- [ ] Run tests, `pnpm --filter desktop check`, expect PASS. Commit.

# Themes that change the whole app — design

## Why
Picking a look barely changes the app. "Crimson" is an accent, and the accent only reaches some
spots: ~18 files hardcode the brand purple (primary button gradient, sidebar glow, logo tile, trim
handles, page glows, clip card hover, login, processing pill, website CSS). Themes only swap
surfaces, all of them violet-tinted, and there are only four. The owner wants a theme pick to
visibly restyle the whole app, and more themes to choose from.

## Goals
- A theme tints every background, border and text grey in the desktop app and the website.
- The accent colour reaches every place that is purple today. No hardcoded brand purple is left
  outside the default token values.
- 8 new themes, mostly free.

## Non-goals / anti-goals
- No light themes. Shadows and overlays assume dark.
- Accent and theme stay two separate picks, with no merge.
- Recording red, success, danger and warning colours stay fixed. They mean something.
- The default Klipt theme must look exactly as it does now.

## Approaches
1. Hand-pick 10 colours per theme (today's shape). Full control, but ~120 values to tune and check
   by eye.
2. **Chosen: generate each theme from a hue + tint strength.** One function produces the 10
   tokens on fixed lightness steps taken from the current Klipt theme. A theme can override
   individual tokens (Klipt keeps its exact values, OLED keeps true black). Themes come out
   consistent, adding one is a single line, and a test checks text contrast on every theme.

## Design
- `packages/shared/src/looks.ts`: `ThemeDef` becomes `{ id, name, hue, tint, overrides?, free? }`.
  `themeTokens(def)` builds the 10 `ThemeToken`s in HSL, using the lightness ladder of the current
  Klipt surfaces, saturation scaled by `tint` (0 = neutral grey). `looksTokens` uses it. Existing
  ids are unchanged, so stored looks and the server's zod enum keep working.
- New themes: Crimson, Forest, Ocean, Slate (tint 0) (free); Sand, Rose, Synthwave, Arctic
  (rewards).
- OLED becomes free (owner's call). Free themes are Klipt, OLED, Crimson, Forest, Ocean and Slate.
- Rewards in `achievements.ts`: `views-100` (Crowd pleaser) now gives Sand instead of OLED,
  `first-tag` gives Rose, `first-combine` gives Synthwave, and `new-year` gives Arctic. Midnight
  and Ember are unchanged.
- Hardcoded purples are replaced with tokens in desktop and website CSS. Gradients use
  `--accent-hover` → `--accent`, and fixed `rgba(124,92,255,a)` uses `color-mix(in srgb,
  var(--accent) N%, transparent)` or the existing soft/glow tokens. Tinted dark fills like
  `rgba(20,14,40,.9)` become surface tokens.
- Tests: every theme has all 10 tokens; `--text` and `--text-muted` on `--bg`/`--surface` meet
  WCAG AA (4.5); Klipt's output equals today's values; a grep-style test fails if brand-purple
  literals reappear in `desktop/src` or `server/public` outside `:root`.

## Left to the builder
The exact hue and tint numbers per theme, as long as contrast passes and each theme reads clearly
as its name. Also whether a few accent-adjacent spots (e.g. `#d2c8ff` text) map to
`--accent-text`.

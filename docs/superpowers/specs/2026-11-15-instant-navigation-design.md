# Instant navigation (desktop)

## Why

The owner says the app "feels slow" outside the editor. Cause: `App.svelte` renders one screen
with `{#if view.name === …}`, so every tab switch destroys the old screen and rebuilds the new
one. Feed refetches clips and games on every visit; Settings re-asks for app info, folder size,
storage, displays and mics, which pop in late; Library remembers filters but reloads both lists.
Every switch flashes half-loaded content.

This is step 1 of 3 (then: the editor's player, which "feels fragile"; then visual design).

## Goals

- Switching between Clips, Feed and Settings shows the screen instantly, as it was left
  (filters, scroll position, loaded data).
- Data stays fresh: refreshed quietly in the background, no spinner when something is already shown.

## Non-goals / anti-goals

- No visual changes (that's step 3).
- No router library.
- The editor is not kept alive: it holds video/audio elements that must not linger.
- Must not break: the "Discard changes?" guard, logout clearing everything (the next account
  mustn't see this one's clips), keyboard shortcuts firing only on the screen you're looking at.

## Approaches

1. **Keep visited screens alive, hidden** (chosen). Instant, keeps scroll and state for free.
2. Cache data only, still rebuild screens: still flashes and loses scroll; doesn't fix it.
3. A router library: adds weight, solves neither problem by itself.

## Design

- `App.svelte` mounts Library, Feed and Settings the first time each is visited and then only
  hides them (`hidden`/`display: none`) while another screen is shown. The editor is rendered on
  top of / instead of the hidden Library, built fresh per clip as now.
- Each kept screen gets its own scroll area, so scroll position survives naturally. The scroll
  bookkeeping in `libraryMemory` goes away.
- Each screen gets an `active` prop. While inactive:
  - its window key handlers do nothing (Ctrl+A in Settings must not select library clips);
  - anything playing is stopped (Library's and Feed's clip player / image viewer close).
- On becoming active again a screen refreshes in the background: Library always (the editor may
  have changed a clip); Feed when its data is older than ~30 s.
- Settings: on "Discard", the screen is rebuilt (keyed) so the draft resets to the saved values.
  App info, clips-folder size, storage, displays and mics come from a small in-memory cache: shown
  at once from the last answer, refreshed quietly.
- Logout: all kept screens are dropped (unmounted) and the caches cleared, as `forgetLibrary`
  does today.

## Left to the builder

Exact cache shape, how "hidden" is implemented, and whether `libraryMemory` survives in reduced
form (e.g. only for logout reset).

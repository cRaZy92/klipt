import { describe, expect, it } from "vitest";

// The brand purple and its hand-mixed shades, as they were written before themes reached the whole
// app. Outside the `:root` defaults, colours come from the accent and theme tokens instead.
const PURPLE =
  /124,\s*92,\s*255|#7c5cff|#9076ff|#c4b6ff|#9479ff|#6a4bf0|#8a6dff|#9a81ff|#b18cff|#c9bcff|#d2c8ff|#6d4ff2|#7152f5|20,\s*14,\s*40|#2a2638/i;

const SOURCES = import.meta.glob<string>(["../**/*.{css,svelte}", "../../../server/public/app.css"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** Blanks the first `:root { … }` block, keeping line numbers. */
const withoutRoot = (css: string) => css.replace(/:root\s*\{[^}]*\}/, (m) => m.replace(/[^\n]/g, ""));

describe("themes reach the whole app", () => {
  it("has no hardcoded brand purple outside the token defaults", () => {
    const hits: string[] = [];
    expect(Object.keys(SOURCES).length).toBeGreaterThan(30);
    expect(Object.keys(SOURCES)).toContain("../../../server/public/app.css");
    for (const [file, text] of Object.entries(SOURCES)) {
      withoutRoot(text)
        .split("\n")
        .forEach((line, i) => {
          if (PURPLE.test(line)) hits.push(`${file}:${i + 1}: ${line.trim()}`);
        });
    }
    expect(hits).toEqual([]);
  });
});

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

/** The rules of `.frame-*` classes in a stylesheet. */
const frameRules = (css: string) => css.match(/\.frame-[a-z]+[^{]*\{[^}]*\}/g) ?? [];

describe('avatar frames', () => {
  // Frames are what friends see on you: they must look the same to every viewer, whatever their own accent.
  for (const file of ['public/app.css', '../desktop/src/looks.css']) {
    it(`${file} draws frames without the viewer's accent`, () => {
      const rules = frameRules(readFileSync(file, 'utf8'));
      expect(rules.length).toBeGreaterThan(0);
      for (const rule of rules) expect(rule).not.toMatch(/var\(--accent/);
    });
  }
});

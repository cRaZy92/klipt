import { describe, expect, it } from 'vitest';
import {
  ACCENTS,
  ACHIEVEMENTS,
  BANNERS,
  DEFAULT_LOOKS,
  FRAMES,
  THEMES,
  accentTokens,
  contrastRatio,
  cosmeticName,
  earnedIds,
  sanitizeLooks,
  themeTokens,
  unlockedCosmetics,
} from '@klipt/shared';

const EDITOR_FIRSTS = ['cut', 'zoom', 'subtitle', 'blur', 'mutePart', 'combine', 'part', 'thumbnailImage'];

describe('looks catalog', () => {
  // No worse than today's Klipt purple (#7c5cff, 4.35 with white), which stays the default.
  it('every accent keeps white text as readable as the default', () => {
    for (const a of ACCENTS) expect(contrastRatio(a.color, '#ffffff'), a.id).toBeGreaterThanOrEqual(4.3);
  });

  it('every theme is readable', () => {
    for (const t of THEMES) {
      expect(contrastRatio(t.tokens['--text'], t.tokens['--bg']), t.id).toBeGreaterThanOrEqual(7);
      expect(contrastRatio(t.tokens['--text-muted'], t.tokens['--surface']), t.id).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('every accent text colour is readable on every theme surface', () => {
    for (const a of ACCENTS) {
      for (const t of THEMES) {
        expect(contrastRatio(accentTokens(a.color)['--accent-text']!, t.tokens['--surface']), `${a.id}/${t.id}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it('has the free basics and nothing else free', () => {
    const freeAccents = ACCENTS.filter((a) => a.free).map((a) => a.id);
    expect(freeAccents).toHaveLength(6);
    expect(freeAccents).toContain('klipt');
    const freeThemes = THEMES.filter((t) => t.free).map((t) => t.id);
    expect(freeThemes).toEqual(['klipt', 'oled', 'crimson', 'forest', 'ocean', 'slate']);
    const free = unlockedCosmetics([]);
    expect([...free].sort()).toEqual([...freeAccents.map((id) => `accent:${id}`), ...freeThemes.map((id) => `theme:${id}`)].sort());
    expect(FRAMES.length).toBeGreaterThan(0);
    expect(BANNERS.length).toBeGreaterThan(0);
  });

  it('has every theme token as a colour', () => {
    for (const t of THEMES) {
      expect(Object.keys(t.tokens), t.id).toHaveLength(10);
      for (const v of Object.values(t.tokens)) expect(v, t.id).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it('generates themes on the klipt ladder', () => {
    const chan = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
    const klipt = THEMES.find((t) => t.id === 'klipt')!.tokens;
    const made = themeTokens(250, 1);
    for (const [k, v] of Object.entries(klipt)) {
      const [a, b] = [chan(v), chan(made[k as keyof typeof made])];
      for (let i = 0; i < 3; i++) expect(Math.abs(a[i]! - b[i]!), k).toBeLessThanOrEqual(2);
    }
    for (const v of Object.values(themeTokens(0, 0))) {
      const [r, g, b] = chan(v);
      expect(r).toBe(g);
      expect(g).toBe(b);
    }
  });

  it('keeps klipt purple as it is today', () => {
    const t = accentTokens('#7c5cff');
    expect(t['--accent']).toBe('#7c5cff');
    expect(t['--accent-soft']).toBe('rgba(124, 92, 255, 0.16)');
    expect(t['--accent-softer']).toBe('rgba(124, 92, 255, 0.08)');
    expect(t['--accent-glow']).toBe('rgba(124, 92, 255, 0.45)');
  });

  it('sanitizes stored looks field by field', () => {
    expect(sanitizeLooks({ accent: 'nope', theme: 'midnight', frame: 42, banner: 'curator' })).toEqual({
      accent: 'klipt',
      theme: 'midnight',
      frame: null,
      banner: 'curator',
    });
    expect(sanitizeLooks(null)).toEqual(DEFAULT_LOOKS);
    expect(sanitizeLooks('garbage')).toEqual(DEFAULT_LOOKS);
  });
});

describe('achievements catalog', () => {
  it('has unique ids, real rewards and real allOf parts', () => {
    const ids = ACHIEVEMENTS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const a of ACHIEVEMENTS) {
      if (a.reward) expect(cosmeticName(a.reward), a.id).not.toBeNull();
      if ('allOf' in a.rule) for (const part of a.rule.allOf) expect(ids, a.id).toContain(part);
    }
  });

  it('earns by thresholds', () => {
    const got = earnedIds({ uploads: 10, anyUploads: 10 }, new Set());
    expect(got).toContain('clips-10');
    expect(got).toContain('first-upload');
    expect(got).not.toContain('clips-100');
  });

  it('earns editor-all only with every editor first', () => {
    expect(earnedIds({}, new Set(EDITOR_FIRSTS))).toContain('editor-all');
    expect(earnedIds({}, new Set(EDITOR_FIRSTS.slice(1)))).not.toContain('editor-all');
  });

  it('unlocks the rewards of earned achievements', () => {
    expect(unlockedCosmetics(['night-owl']).has('theme:midnight')).toBe(true);
    expect(unlockedCosmetics([]).has('theme:midnight')).toBe(false);
    const rewardOf = (id: string) => ACHIEVEMENTS.find((a) => a.id === id)!.reward;
    expect(rewardOf('views-100')).toBe('theme:sand');
    expect(rewardOf('first-tag')).toBe('theme:rose');
    expect(rewardOf('first-combine')).toBe('theme:synthwave');
    expect(rewardOf('new-year')).toBe('theme:arctic');
    expect(rewardOf('hours-10')).toBe('theme:ember');
  });
});

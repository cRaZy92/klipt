/**
 * How Klipt looks for you and how others see you: accent colours and themes (only you see them),
 * avatar frames and profile banners (seen by everyone). A few basics are free; the rest are
 * unlocked by achievements (`achievements.ts`).
 */

export type ThemeToken =
  | '--bg'
  | '--bg-2'
  | '--surface'
  | '--surface-2'
  | '--surface-3'
  | '--border'
  | '--border-strong'
  | '--text'
  | '--text-muted'
  | '--text-faint';
export type ThemeTokens = Record<ThemeToken, string>;

export interface AccentDef {
  id: string;
  name: string;
  /** `#rrggbb`; white text on it stays readable. */
  color: string;
  free?: true;
}
export interface ThemeDef {
  id: string;
  name: string;
  tokens: ThemeTokens;
  free?: true;
}
export interface FrameDef {
  id: string;
  name: string;
}
export interface BannerDef {
  id: string;
  name: string;
}

export const ACCENTS: AccentDef[] = [
  { id: 'klipt', name: 'Klipt', color: '#7c5cff', free: true },
  { id: 'ocean', name: 'Ocean', color: '#2563eb', free: true },
  { id: 'teal', name: 'Teal', color: '#0f766e', free: true },
  { id: 'forest', name: 'Forest', color: '#15803d', free: true },
  { id: 'sunset', name: 'Sunset', color: '#c2410c', free: true },
  { id: 'rose', name: 'Rose', color: '#db2777', free: true },
  { id: 'gold', name: 'Gold', color: '#a16207' },
  { id: 'crimson', name: 'Crimson', color: '#b91c1c' },
  { id: 'mint', name: 'Mint', color: '#047857' },
  { id: 'ice', name: 'Ice', color: '#0369a1' },
];

export const THEMES: ThemeDef[] = [
  {
    id: 'klipt',
    name: 'Klipt',
    free: true,
    tokens: {
      '--bg': '#0b0a10',
      '--bg-2': '#100f17',
      '--surface': '#15141d',
      '--surface-2': '#1c1a27',
      '--surface-3': '#262333',
      '--border': '#272436',
      '--border-strong': '#3a3550',
      '--text': '#eeedf5',
      '--text-muted': '#a5a1b8',
      '--text-faint': '#7f7a96',
    },
  },
  {
    id: 'midnight',
    name: 'Midnight',
    tokens: {
      '--bg': '#070b18',
      '--bg-2': '#0a1020',
      '--surface': '#0f1729',
      '--surface-2': '#152036',
      '--surface-3': '#1d2a45',
      '--border': '#1e2a44',
      '--border-strong': '#2e3f63',
      '--text': '#e8eefc',
      '--text-muted': '#9fb0d0',
      '--text-faint': '#7487ab',
    },
  },
  {
    id: 'oled',
    name: 'OLED',
    tokens: {
      '--bg': '#000000',
      '--bg-2': '#050505',
      '--surface': '#0c0c0e',
      '--surface-2': '#141417',
      '--surface-3': '#1e1e22',
      '--border': '#1f1f24',
      '--border-strong': '#33333a',
      '--text': '#f2f2f5',
      '--text-muted': '#a8a8b3',
      '--text-faint': '#7c7c88',
    },
  },
  {
    id: 'ember',
    name: 'Ember',
    tokens: {
      '--bg': '#110a08',
      '--bg-2': '#170e0b',
      '--surface': '#1d1310',
      '--surface-2': '#261a15',
      '--surface-3': '#33231c',
      '--border': '#34241d',
      '--border-strong': '#4d3529',
      '--text': '#f7ece6',
      '--text-muted': '#c2a99b',
      '--text-faint': '#957a6c',
    },
  },
];

export const FRAMES: FrameDef[] = [
  { id: 'bronze', name: 'Bronze ring' },
  { id: 'silver', name: 'Silver ring' },
  { id: 'gold', name: 'Gold ring' },
  { id: 'rainbow', name: 'Rainbow ring' },
  { id: 'pulse', name: 'Pulsing ring' },
];

export const BANNERS: BannerDef[] = [
  { id: 'curator', name: 'Curator' },
  { id: 'editor', name: "Director's cut" },
  { id: 'squad', name: 'Squad' },
];

export type CosmeticKind = 'accent' | 'theme' | 'frame' | 'banner';
export type CosmeticId = `${CosmeticKind}:${string}`;

/** What you've picked; `frame`/`banner` null = none. */
export interface Looks {
  accent: string;
  theme: string;
  frame: string | null;
  banner: string | null;
}

export const DEFAULT_LOOKS: Looks = { accent: 'klipt', theme: 'klipt', frame: null, banner: null };

const CATALOG: Record<CosmeticKind, { id: string; name: string; free?: true }[]> = {
  accent: ACCENTS,
  theme: THEMES,
  frame: FRAMES,
  banner: BANNERS,
};

/** The name of a cosmetic ("Midnight"), null for an unknown id. */
export function cosmeticName(id: CosmeticId | string): string | null {
  const sep = id.indexOf(':');
  const list = CATALOG[id.slice(0, sep) as CosmeticKind];
  return list?.find((c) => c.id === id.slice(sep + 1))?.name ?? null;
}

export const FREE_COSMETICS: CosmeticId[] = (Object.keys(CATALOG) as CosmeticKind[]).flatMap((kind) =>
  CATALOG[kind].filter((c) => c.free).map((c) => `${kind}:${c.id}` as CosmeticId),
);

const known = (kind: CosmeticKind, id: unknown): id is string =>
  typeof id === 'string' && CATALOG[kind].some((c) => c.id === id);

/** Stored looks as they can be used: anything unknown or malformed falls back to the default, field by field. */
export function sanitizeLooks(raw: unknown): Looks {
  const r = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    accent: known('accent', r.accent) ? r.accent : DEFAULT_LOOKS.accent,
    theme: known('theme', r.theme) ? r.theme : DEFAULT_LOOKS.theme,
    frame: known('frame', r.frame) ? r.frame : null,
    banner: known('banner', r.banner) ? r.banner : null,
  };
}

/** Whether `id` is a frame in the catalog. */
export const isFrame = (id: unknown): id is string => known('frame', id);

function rgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '');
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}

const toHex = (c: number[]) => '#' + c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
const mixWhite = (c: [number, number, number], t: number) => toHex(c.map((v) => v + (255 - v) * t));

/** The `--accent*` tokens from one base colour. */
export function accentTokens(color: string): Record<string, string> {
  const c = rgb(color);
  const a = (alpha: number) => `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})`;
  return {
    '--accent': color.toLowerCase(),
    '--accent-hover': mixWhite(c, 0.15),
    '--accent-text': mixWhite(c, 0.55),
    '--accent-soft': a(0.16),
    '--accent-softer': a(0.08),
    '--accent-glow': a(0.45),
  };
}

/** Every token a look sets: the theme's surfaces and the accent's colours. */
export function looksTokens(looks: Looks): Record<string, string> {
  const l = sanitizeLooks(looks);
  const theme = THEMES.find((t) => t.id === l.theme)!;
  const accent = ACCENTS.find((a) => a.id === l.accent)!;
  return { ...theme.tokens, ...accentTokens(accent.color) };
}

/** WCAG contrast ratio of two `#rrggbb` colours. */
export function contrastRatio(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = rgb(hex).map((v) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    }) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p) as [number, number];
  return (x + 0.05) / (y + 0.05);
}

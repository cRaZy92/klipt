/**
 * Achievements: earned automatically, collected as badges, some unlock a cosmetic. The server
 * decides who earned what (`earnedIds`) from what it knows plus what the desktop app reports
 * (`ActivityReport`). Adding one = adding a definition here.
 */
import { FREE_COSMETICS, type CosmeticId } from './looks.js';

/** Counters the desktop app reports (it alone knows them). */
export const ACTIVITY_COUNTERS = ['clipsSaved', 'screenshots', 'recordedMs', 'recordings', 'imports', 'games'] as const;
/** Things the app reports as done at least once. */
export const ACTIVITY_FIRSTS = [
  'cut',
  'zoom',
  'subtitle',
  'blur',
  'mutePart',
  'combine',
  'part',
  'thumbnailImage',
  'nightOwl',
  'marathon',
] as const;
export type ActivityCounter = (typeof ACTIVITY_COUNTERS)[number];
export type ActivityFirst = (typeof ACTIVITY_FIRSTS)[number];

/** What the server counts itself. */
export type ServerStat =
  | 'uploads'
  | 'anyUploads'
  | 'totalViews'
  | 'taggedIn'
  | 'tagsGiven'
  | 'maxAlbumClips'
  | 'seenByEveryone'
  | 'blinkUploads'
  | 'newYearUploads'
  | 'yearOld';
export type StatKey = ServerStat | ActivityCounter;

export type Rule = { stat: StatKey; min: number } | { first: ActivityFirst } | { allOf: string[] };

export interface AchievementDef {
  id: string;
  name: string;
  description: string;
  /** One emoji. */
  icon: string;
  category: 'milestones' | 'social' | 'editor' | 'hidden';
  /** Shown as "???" until earned. */
  hidden?: true;
  reward?: CosmeticId;
  rule: Rule;
}

const EDITOR_IDS = [
  'first-cut',
  'first-zoom',
  'first-subtitle',
  'first-blur',
  'first-mute',
  'first-combine',
  'first-part',
  'first-thumbnail',
];

export const ACHIEVEMENTS: AchievementDef[] = [
  // Milestones
  { id: 'first-clip', name: 'First clip', description: 'Saved your first clip', icon: '🎬', category: 'milestones', rule: { stat: 'clipsSaved', min: 1 } },
  { id: 'first-upload', name: 'Going live', description: 'Uploaded your first clip', icon: '🚀', category: 'milestones', rule: { stat: 'anyUploads', min: 1 } },
  { id: 'clips-10', name: 'Getting started', description: 'Uploaded 10 clips', icon: '🥉', category: 'milestones', reward: 'frame:bronze', rule: { stat: 'uploads', min: 10 } },
  { id: 'clips-100', name: 'Regular', description: 'Uploaded 100 clips', icon: '🥈', category: 'milestones', reward: 'frame:silver', rule: { stat: 'uploads', min: 100 } },
  { id: 'clips-500', name: 'Legend', description: 'Uploaded 500 clips', icon: '🥇', category: 'milestones', reward: 'frame:gold', rule: { stat: 'uploads', min: 500 } },
  { id: 'hours-10', name: 'Always rolling', description: 'Recorded 10 hours of clips and recordings', icon: '⏱️', category: 'milestones', reward: 'theme:ember', rule: { stat: 'recordedMs', min: 10 * 3_600_000 } },
  { id: 'games-5', name: 'Variety', description: 'Clips from 5 different games', icon: '🎮', category: 'milestones', reward: 'accent:gold', rule: { stat: 'games', min: 5 } },
  { id: 'games-15', name: 'Game hopper', description: 'Clips from 15 different games', icon: '🕹️', category: 'milestones', reward: 'accent:crimson', rule: { stat: 'games', min: 15 } },
  { id: 'first-recording', name: 'Rolling', description: 'Made your first full recording', icon: '🔴', category: 'milestones', rule: { stat: 'recordings', min: 1 } },
  { id: 'first-screenshot', name: 'Snap', description: 'Took your first screenshot', icon: '📸', category: 'milestones', rule: { stat: 'screenshots', min: 1 } },
  { id: 'first-import', name: 'Moving in', description: 'Imported clips from a folder', icon: '📦', category: 'milestones', rule: { stat: 'imports', min: 1 } },
  // Social
  { id: 'views-10', name: 'Noticed', description: 'Your clips have 10 views', icon: '👀', category: 'social', rule: { stat: 'totalViews', min: 10 } },
  { id: 'views-100', name: 'Crowd pleaser', description: 'Your clips have 100 views', icon: '🍿', category: 'social', reward: 'theme:oled', rule: { stat: 'totalViews', min: 100 } },
  { id: 'views-1000', name: 'Famous', description: 'Your clips have 1,000 views', icon: '🌟', category: 'social', reward: 'frame:pulse', rule: { stat: 'totalViews', min: 1000 } },
  { id: 'seen-by-everyone', name: 'Seen by everyone', description: 'Everyone on the server watched one of your clips', icon: '🌈', category: 'social', reward: 'frame:rainbow', rule: { stat: 'seenByEveryone', min: 1 } },
  { id: 'tagged-10', name: 'Squad member', description: 'Friends tagged you in 10 clips', icon: '🤝', category: 'social', reward: 'banner:squad', rule: { stat: 'taggedIn', min: 10 } },
  { id: 'first-tag', name: 'Shout-out', description: 'Tagged a friend in a clip', icon: '🏷️', category: 'social', rule: { stat: 'tagsGiven', min: 1 } },
  { id: 'curator', name: 'Curator', description: 'Made an album with 10 or more clips', icon: '🗂️', category: 'social', reward: 'banner:curator', rule: { stat: 'maxAlbumClips', min: 10 } },
  // Editor (a tour of the editor)
  { id: 'first-cut', name: 'Snip', description: 'Cut a part out of a clip', icon: '✂️', category: 'editor', rule: { first: 'cut' } },
  { id: 'first-zoom', name: 'Enhance', description: 'Zoomed in on a part of a clip', icon: '🔍', category: 'editor', rule: { first: 'zoom' } },
  { id: 'first-subtitle', name: 'Narrator', description: 'Added a subtitle', icon: '💬', category: 'editor', rule: { first: 'subtitle' } },
  { id: 'first-blur', name: 'Nothing to see here', description: 'Blurred a part of the picture', icon: '🌫️', category: 'editor', rule: { first: 'blur' } },
  { id: 'first-mute', name: 'Hush', description: 'Muted a part of a clip', icon: '🔇', category: 'editor', rule: { first: 'mutePart' } },
  { id: 'first-combine', name: 'Montage', description: 'Combined clips into one', icon: '🧩', category: 'editor', rule: { first: 'combine' } },
  { id: 'first-part', name: 'Spin-off', description: 'Made a new clip from a part', icon: '🪄', category: 'editor', rule: { first: 'part' } },
  { id: 'first-thumbnail', name: 'Cover art', description: 'Used a picture as a thumbnail', icon: '🖼️', category: 'editor', rule: { first: 'thumbnailImage' } },
  { id: 'editor-all', name: "Director's cut", description: 'Used every editor tool', icon: '🎞️', category: 'editor', reward: 'banner:editor', rule: { allOf: EDITOR_IDS } },
  // Hidden
  { id: 'night-owl', name: 'Night owl', description: 'Saved a clip between 3 and 5 in the morning', icon: '🦉', category: 'hidden', hidden: true, reward: 'theme:midnight', rule: { first: 'nightOwl' } },
  { id: 'blink', name: 'Blink', description: 'Uploaded a clip shorter than 5 seconds', icon: '⚡', category: 'hidden', hidden: true, rule: { stat: 'blinkUploads', min: 1 } },
  { id: 'marathon', name: 'Marathon', description: 'Made a full recording longer than 3 hours', icon: '🏃', category: 'hidden', hidden: true, reward: 'accent:mint', rule: { first: 'marathon' } },
  { id: 'new-year', name: 'Happy new year', description: 'Uploaded a clip on 1 January', icon: '🎆', category: 'hidden', hidden: true, rule: { stat: 'newYearUploads', min: 1 } },
  { id: 'archivist', name: 'Archivist', description: 'A clip of yours is still on the server a year after upload', icon: '🏛️', category: 'hidden', hidden: true, reward: 'accent:ice', rule: { stat: 'yearOld', min: 1 } },
];

export function achievementById(id: string): AchievementDef | undefined {
  return ACHIEVEMENTS.find((a) => a.id === id);
}

/** The achievements these stats and firsts earn, in catalog order. */
export function earnedIds(stats: Partial<Record<StatKey, number>>, firsts: ReadonlySet<string>): string[] {
  const got = new Set<string>();
  for (const a of ACHIEVEMENTS) {
    const r = a.rule;
    if ('stat' in r ? (stats[r.stat] ?? 0) >= r.min : 'first' in r ? firsts.has(r.first) : false) got.add(a.id);
  }
  for (const a of ACHIEVEMENTS) {
    if ('allOf' in a.rule && a.rule.allOf.every((id) => got.has(id))) got.add(a.id);
  }
  return ACHIEVEMENTS.filter((a) => got.has(a.id)).map((a) => a.id);
}

/** The free cosmetics plus the rewards of `earned`. */
export function unlockedCosmetics(earned: Iterable<string>): Set<CosmeticId> {
  const out = new Set<CosmeticId>(FREE_COSMETICS);
  for (const id of earned) {
    const reward = achievementById(id)?.reward;
    if (reward) out.add(reward);
  }
  return out;
}

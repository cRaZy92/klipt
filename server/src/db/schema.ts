import { bigint, boolean, index, integer, pgTable, primaryKey, text, timestamp, uuid } from 'drizzle-orm/pg-core';

const ts = (name: string) => timestamp(name, { withTimezone: true });

export const users = pgTable('users', {
  id: uuid('id').primaryKey().defaultRandom(),
  username: text('username').notNull(),
  usernameKey: text('username_key').notNull().unique(),
  passwordHash: text('password_hash').notNull(),
  isAdmin: boolean('is_admin').notNull().default(false),
  disabledAt: ts('disabled_at'),
  createdAt: ts('created_at').notNull().defaultNow(),
});

export const sessions = pgTable('sessions', {
  id: uuid('id').primaryKey().defaultRandom(),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  tokenHash: text('token_hash').notNull().unique(),
  kind: text('kind', { enum: ['web', 'desktop'] }).notNull(),
  label: text('label'),
  createdAt: ts('created_at').notNull().defaultNow(),
  lastUsedAt: ts('last_used_at').notNull().defaultNow(),
  expiresAt: ts('expires_at'),
});

export const invites = pgTable('invites', {
  code: text('code').primaryKey(),
  createdBy: uuid('created_by').notNull().references(() => users.id),
  createdAt: ts('created_at').notNull().defaultNow(),
  expiresAt: ts('expires_at').notNull(),
  usedBy: uuid('used_by').references(() => users.id),
  usedAt: ts('used_at'),
  revokedAt: ts('revoked_at'),
});

export const uploads = pgTable('uploads', {
  id: text('id').primaryKey(),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  createdAt: ts('created_at').notNull().defaultNow(),
  /**
   * Set while a finalize is processing this upload (claim marker; stale after a timeout).
   * On a tombstone (clipId set) it records when the finalize completed.
   */
  finalizingAt: ts('finalizing_at'),
  /** Set once finalized: the row stays as a tombstone so a retried finalize can return the clip. */
  clipId: text('clip_id').references(() => clips.id, { onDelete: 'cascade' }),
});

export const clips = pgTable(
  'clips',
  {
    id: text('id').primaryKey(),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    title: text('title').notNull(),
    gameName: text('game_name'),
    steamAppId: integer('steam_app_id'),
    visibility: text('visibility', { enum: ['feed', 'link'] }).notNull().default('feed'),
    durationMs: integer('duration_ms').notNull(),
    width: integer('width').notNull(),
    height: integer('height').notNull(),
    sizeBytes: bigint('size_bytes', { mode: 'number' }).notNull(),
    /** A video clip, or a screenshot (stored as `<id>.png`). */
    kind: text('kind', { enum: ['clip', 'screenshot'] }).notNull().default('clip'),
    /** Plays by visitors who weren't logged in (share links); logged-in viewers are in clip_views. */
    anonViews: integer('anon_views').notNull().default(0),
    /**
     * Server-side compression of a video clip (null for screenshots): `queued` and `running` clips
     * play the uploaded file meanwhile; `done` replaced it with a smaller one; `skipped` kept the
     * upload because compressing barely helped; `failed` kept it too (an admin can retry).
     */
    transcodeState: text('transcode_state', { enum: ['queued', 'running', 'done', 'skipped', 'failed'] }),
    /** Bumped each time the video file is replaced: version n > 0 is stored as `<id>.v<n>.mp4`. */
    mediaVersion: integer('media_version').notNull().default(0),
    /** The uploaded file's size, once a compression ran (sizeBytes is then the stored file's). */
    originalBytes: bigint('original_bytes', { mode: 'number' }),
    transcodeError: text('transcode_error'),
    transcodeStartedAt: ts('transcode_started_at'),
    transcodeFinishedAt: ts('transcode_finished_at'),
    createdAt: timestamp('created_at', { withTimezone: true, precision: 3 }).notNull().defaultNow(),
  },
  (t) => [
    index('clips_created_idx').on(t.createdAt.desc(), t.id.desc()),
    index('clips_game_idx').on(t.gameName),
    index('clips_transcode_idx').on(t.transcodeState),
  ],
);

/** Who has watched a clip: one row per logged-in viewer (never the owner), the first time they played it. */
export const clipViews = pgTable(
  'clip_views',
  {
    clipId: text('clip_id').notNull().references(() => clips.id, { onDelete: 'cascade' }),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    viewedAt: ts('viewed_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.clipId, t.userId] })],
);

/** Friends in a clip ("with bob, alice"): the clip also shows on their profiles. */
export const clipTags = pgTable(
  'clip_tags',
  {
    clipId: text('clip_id').notNull().references(() => clips.id, { onDelete: 'cascade' }),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  },
  (t) => [primaryKey({ columns: [t.clipId, t.userId] }), index('clip_tags_user_idx').on(t.userId)],
);

/** Clips a user starred: any clip they can open (their own, friends', link-only ones they have the link to). */
export const clipFavorites = pgTable(
  'clip_favorites',
  {
    clipId: text('clip_id').notNull().references(() => clips.id, { onDelete: 'cascade' }),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    createdAt: ts('created_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.clipId, t.userId] }), index('clip_favorites_user_idx').on(t.userId)],
);

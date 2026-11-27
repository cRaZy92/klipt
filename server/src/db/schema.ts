import { bigint, boolean, index, integer, jsonb, pgTable, primaryKey, text, timestamp, uuid } from 'drizzle-orm/pg-core';

const ts = (name: string) => timestamp(name, { withTimezone: true });

export const users = pgTable('users', {
  id: uuid('id').primaryKey().defaultRandom(),
  username: text('username').notNull(),
  usernameKey: text('username_key').notNull().unique(),
  /** The name shown to friends; null shows the username. */
  displayName: text('display_name'),
  passwordHash: text('password_hash').notNull(),
  isAdmin: boolean('is_admin').notNull().default(false),
  disabledAt: ts('disabled_at'),
  createdAt: ts('created_at').notNull().defaultNow(),
  /**
   * The profile picture's version (`<data>/avatars/<id>.jpg?v=<n>`). 0 or negative: none, draw the
   * initial; a removal negates it so the next picture still gets a new URL.
   */
  avatarVersion: integer('avatar_version').notNull().default(0),
  /** Equipped accent, theme, frame and banner (`Looks`); read through `sanitizeLooks`. Null = defaults. */
  looks: jsonb('looks'),
  /** What the desktop app reported (`StoredActivity`): counters (highest seen) and firsts. */
  activity: jsonb('activity'),
  /** Who sees the profile in full: friends only, or everyone. Also the default for new uploads. */
  profileVisibility: text('profile_visibility', { enum: ['friends', 'public'] }).notNull().default('friends'),
});

/**
 * A friendship or a pending request. One row per pair, stored with `userA < userB`;
 * `requestedBy` says who asked.
 */
export const friendships = pgTable(
  'friendships',
  {
    userA: uuid('user_a').notNull().references(() => users.id, { onDelete: 'cascade' }),
    userB: uuid('user_b').notNull().references(() => users.id, { onDelete: 'cascade' }),
    requestedBy: uuid('requested_by').notNull().references(() => users.id, { onDelete: 'cascade' }),
    status: text('status', { enum: ['pending', 'accepted'] }).notNull(),
    createdAt: ts('created_at').notNull().defaultNow(),
    acceptedAt: ts('accepted_at'),
  },
  (t) => [primaryKey({ columns: [t.userA, t.userB] }), index('friendships_user_b_idx').on(t.userB)],
);

/** Achievements a user earned; never removed. `quiet`: earned for past activity, announced in one summary. */
export const userAchievements = pgTable(
  'user_achievements',
  {
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    achievementId: text('achievement_id').notNull(),
    earnedAt: ts('earned_at').notNull().defaultNow(),
    seenAt: ts('seen_at'),
    quiet: boolean('quiet').notNull().default(false),
  },
  (t) => [primaryKey({ columns: [t.userId, t.achievementId] })],
);

export const sessions = pgTable(
  'sessions',
  {
  id: uuid('id').primaryKey().defaultRandom(),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  tokenHash: text('token_hash').notNull().unique(),
  kind: text('kind', { enum: ['web', 'desktop'] }).notNull(),
  label: text('label'),
  createdAt: ts('created_at').notNull().defaultNow(),
  lastUsedAt: ts('last_used_at').notNull().defaultNow(),
  expiresAt: ts('expires_at'),
  /** What the desktop app reports about its PC (`POST /api/me/device`), for the admin page. */
  appVersion: text('app_version'),
  os: text('os'),
  gpu: text('gpu'),
  },
  (t) => [index('sessions_user_idx').on(t.userId)],
);

/** A one-time link an admin makes so a user can set a new password. */
export const passwordResets = pgTable('password_resets', {
  code: text('code').primaryKey(),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  createdBy: uuid('created_by').notNull().references(() => users.id, { onDelete: 'cascade' }),
  createdAt: ts('created_at').notNull().defaultNow(),
  expiresAt: ts('expires_at').notNull(),
  usedAt: ts('used_at'),
});

/** A problem report or a feature idea, from the app (with logs and system info) or the website. */
export const feedback = pgTable(
  'feedback',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    kind: text('kind', { enum: ['bug', 'idea'] }).notNull(),
    message: text('message').notNull(),
    source: text('source', { enum: ['desktop', 'web'] }).notNull(),
    /** Version, OS, CPU, GPU, recorder… as label → value. */
    system: jsonb('system').$type<Record<string, string>>(),
    /** The end of the app's log. */
    logs: text('logs'),
    createdAt: ts('created_at').notNull().defaultNow(),
    resolvedAt: ts('resolved_at'),
  },
  (t) => [index('feedback_created_idx').on(t.createdAt.desc())],
);

export const invites = pgTable('invites', {
  code: text('code').primaryKey(),
  createdBy: uuid('created_by').notNull().references(() => users.id),
  createdAt: ts('created_at').notNull().defaultNow(),
  expiresAt: ts('expires_at').notNull(),
  usedBy: uuid('used_by').references(() => users.id),
  usedAt: ts('used_at'),
  revokedAt: ts('revoked_at'),
});

export const uploads = pgTable(
  'uploads',
  {
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
  },
  (t) => [index('uploads_user_idx').on(t.userId)],
);

export const clips = pgTable(
  'clips',
  {
    id: text('id').primaryKey(),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    title: text('title').notNull(),
    gameName: text('game_name'),
    steamAppId: integer('steam_app_id'),
    visibility: text('visibility', { enum: ['link', 'friends', 'public'] }).notNull().default('friends'),
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
    /** An admin's high-quality upload: kept as uploaded, never compressed by the server. */
    highQuality: boolean('high_quality').notNull().default(false),
    /** Bumped each time the video file is replaced: version n > 0 is stored as `<id>.v<n>.mp4`. */
    mediaVersion: integer('media_version').notNull().default(0),
    /** Bumped each time the owner sets the thumbnail from an image: part of the thumbnail URL's `?v=`. */
    thumbVersion: integer('thumb_version').notNull().default(0),
    /**
     * The embed settings (`EMBED_VERSION`) that made the clip's `<id>.embed.mp4`; 0 for copies made
     * before this was recorded. A lower one than current is remade by the embed backfill.
     */
    embedVersion: integer('embed_version').notNull().default(0),
    /**
     * A long clip waiting for its embed copy, made in the background: its share link isn't handed
     * out until this clears (made, or given up on).
     */
    embedPending: boolean('embed_pending').notNull().default(false),
    /** The uploaded file's size, once a compression ran (sizeBytes is then the stored file's). */
    originalBytes: bigint('original_bytes', { mode: 'number' }),
    transcodeError: text('transcode_error'),
    transcodeStartedAt: ts('transcode_started_at'),
    transcodeFinishedAt: ts('transcode_finished_at'),
    createdAt: timestamp('created_at', { withTimezone: true, precision: 3 }).notNull().defaultNow(),
  },
  (t) => [
    index('clips_created_idx').on(t.createdAt.desc(), t.id.desc()),
    index('clips_user_idx').on(t.userId, t.createdAt.desc()),
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

/** A named list of clips (anyone's, in the owner's order) with one share link: `/a/<id>`, the id is the secret. */
export const albums = pgTable(
  'albums',
  {
    id: text('id').primaryKey(),
    userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
    title: text('title').notNull(),
    createdAt: ts('created_at').notNull().defaultNow(),
    /** Bumped when the title or the clips change; albums are listed most recently changed first. */
    updatedAt: ts('updated_at').notNull().defaultNow(),
  },
  (t) => [index('albums_user_idx').on(t.userId, t.updatedAt.desc())],
);

/** The clips in an album, in order (`position`, lowest first). A deleted clip leaves its albums. */
export const albumClips = pgTable(
  'album_clips',
  {
    albumId: text('album_id').notNull().references(() => albums.id, { onDelete: 'cascade' }),
    clipId: text('clip_id').notNull().references(() => clips.id, { onDelete: 'cascade' }),
    position: integer('position').notNull(),
    addedAt: ts('added_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.albumId, t.clipId] }), index('album_clips_clip_idx').on(t.clipId)],
);

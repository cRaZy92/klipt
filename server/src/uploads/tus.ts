import type { IncomingMessage } from 'node:http';
import { mkdir, readFile, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { FileStore } from '@tus/file-store';
import { Server } from '@tus/server';
import { eq, lt } from 'drizzle-orm';
import type { FastifyInstance } from 'fastify';
import type { Config } from '../config.js';
import type { Db } from '../db/client.js';
import { uploads } from '../db/schema.js';
import type { AuthUser } from '../auth/sessions.js';

const UPLOAD_PATH = '/api/uploads';
const DAY_MS = 24 * 60 * 60 * 1000;

const tusDir = (dataDir: string) => join(dataDir, 'tus');
export const tusFilePath = (dataDir: string, id: string) => join(tusDir(dataDir), id);

type RawWithUser = IncomingMessage & { klipUser?: AuthUser | null };

/** Deletes the tus data file and its `.json` sidecar (missing files are fine). */
export async function removeTusFiles(dataDir: string, id: string): Promise<void> {
  const base = tusFilePath(dataDir, id);
  await Promise.all([rm(base, { force: true }), rm(`${base}.json`, { force: true })]);
}

/** Whether every byte declared at creation has been received. */
export async function isUploadComplete(dataDir: string, id: string): Promise<boolean> {
  try {
    const base = tusFilePath(dataDir, id);
    const meta = JSON.parse(await readFile(`${base}.json`, 'utf8')) as { size?: number | null };
    if (typeof meta.size !== 'number') return false;
    return (await stat(base)).size === meta.size;
  } catch {
    return false;
  }
}

export async function removeUpload(db: Db, dataDir: string, id: string): Promise<void> {
  await removeTusFiles(dataDir, id);
  await db.delete(uploads).where(eq(uploads.id, id));
}

export async function cleanupStaleUploads(
  db: Db,
  dataDir: string,
  now: Date,
  maxAgeMs = DAY_MS,
): Promise<number> {
  const stale = await db
    .select({ id: uploads.id })
    .from(uploads)
    .where(lt(uploads.createdAt, new Date(now.getTime() - maxAgeMs)));
  for (const { id } of stale) await removeUpload(db, dataDir, id);
  return stale.length;
}

export async function mountTus(
  app: FastifyInstance,
  { db, config }: { db: Db; config: Pick<Config, 'dataDir' | 'maxUploadBytes' | 'baseUrl'> },
): Promise<void> {
  const directory = tusDir(config.dataDir);
  await mkdir(directory, { recursive: true });

  const userOf = (req: { runtime?: { node?: { req?: unknown } } }) =>
    (req.runtime?.node?.req as RawWithUser | undefined)?.klipUser ?? null;

  const tus = new Server({
    path: UPLOAD_PATH,
    datastore: new FileStore({ directory }),
    maxSize: config.maxUploadBytes,
    generateUrl: (_req, { id }) => `${config.baseUrl}${UPLOAD_PATH}/${id}`,
    async onIncomingRequest(req, uploadId) {
      const user = userOf(req);
      if (!user) throw { status_code: 401, body: 'Unauthorized\n' };
      if (uploadId) {
        const [row] = await db.select().from(uploads).where(eq(uploads.id, uploadId));
        // Unknown ids fall through to tus (404); other users' uploads look nonexistent.
        if (row && row.userId !== user.id) throw { status_code: 404, body: 'Not found\n' };
      }
    },
    async onUploadCreate(req, upload) {
      const user = userOf(req);
      if (!user) throw { status_code: 401, body: 'Unauthorized\n' };
      await db.insert(uploads).values({ id: upload.id, userId: user.id });
      return {};
    },
  });

  // tus reads the raw stream itself, so Fastify must leave this body untouched.
  app.addContentTypeParser('application/offset+octet-stream', (_req, _payload, done) => done(null));

  const handler = async (
    req: import('fastify').FastifyRequest,
    reply: import('fastify').FastifyReply,
  ) => {
    (req.raw as RawWithUser).klipUser = req.user;
    reply.hijack();
    await tus.handle(req.raw, reply.raw);
  };
  const methods = ['POST', 'HEAD', 'PATCH', 'OPTIONS', 'DELETE'] as const;
  const opts = { bodyLimit: config.maxUploadBytes + 1024 * 1024 };
  app.route({ method: [...methods], url: UPLOAD_PATH, ...opts, handler });
  app.route({ method: [...methods], url: `${UPLOAD_PATH}/*`, ...opts, handler });
}

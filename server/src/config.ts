import { z } from 'zod';

export interface Config {
  baseUrl: string;
  databaseUrl: string;
  dataDir: string;
  port: number;
  maxUploadBytes: number;
  adminUsername?: string;
  adminPassword?: string;
  secureCookies: boolean;
  /** Number of trusted reverse-proxy hops in front of the server (X-Forwarded-For). */
  trustProxyHops: number;
}

const emptyToUndefined = (v: unknown) => (v === '' ? undefined : v);

const envSchema = z.object({
  BASE_URL: z.string().url(),
  DATABASE_URL: z.string().min(1),
  DATA_DIR: z.preprocess(emptyToUndefined, z.string().default('/data')),
  PORT: z.preprocess(emptyToUndefined, z.coerce.number().int().min(1).max(65535).default(3000)),
  MAX_UPLOAD_MB: z.preprocess(emptyToUndefined, z.coerce.number().positive().default(2048)),
  TRUST_PROXY_HOPS: z.preprocess(emptyToUndefined, z.coerce.number().int().min(0).max(10).default(1)),
  ADMIN_USERNAME: z.preprocess(emptyToUndefined, z.string().optional()),
  ADMIN_PASSWORD: z.preprocess(emptyToUndefined, z.string().optional()),
});

export function loadConfig(env: Record<string, string | undefined>): Config {
  const parsed = envSchema.safeParse(env);
  if (!parsed.success) {
    const details = parsed.error.issues
      .map((i) => `${i.path.join('.')}: ${i.message}`)
      .join('; ');
    throw new Error(`Invalid configuration: ${details}`);
  }
  const e = parsed.data;
  const baseUrl = e.BASE_URL.replace(/\/+$/, '');
  return {
    baseUrl,
    databaseUrl: e.DATABASE_URL,
    dataDir: e.DATA_DIR,
    port: e.PORT,
    maxUploadBytes: Math.floor(e.MAX_UPLOAD_MB * 1024 * 1024),
    adminUsername: e.ADMIN_USERNAME,
    adminPassword: e.ADMIN_PASSWORD,
    trustProxyHops: e.TRUST_PROXY_HOPS,
    secureCookies: baseUrl.startsWith('https://'),
  };
}

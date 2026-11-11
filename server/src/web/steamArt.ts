/**
 * Steam header art for the game chips.
 *
 * Steam serves newer store art from hashed paths (`…/apps/<id>/<hash>/header.jpg`), so the old
 * `cdn.cloudflare.steamstatic.com/steam/apps/<id>/header.jpg` URL 404s for many games. The real
 * URL comes from the store's appdetails API; lookups are cached in memory.
 */

const APPDETAILS = 'https://store.steampowered.com/api/appdetails';
const FOUND_TTL_MS = 24 * 60 * 60 * 1000;
const MISSING_TTL_MS = 10 * 60 * 1000;
const TIMEOUT_MS = 5000;
const MAX_ENTRIES = 1000;

type Fetch = (url: string, init?: { signal?: AbortSignal }) => Promise<Response>;

/** A Steam app id from a URL segment, or null when it isn't one. */
export function parseAppId(raw: string): number | null {
  if (!/^[1-9]\d{0,9}$/.test(raw)) return null;
  const id = Number(raw);
  return id <= 0xffffffff ? id : null;
}

/** Only https Steam CDN URLs are redirected to. */
function isSteamCdn(url: unknown): url is string {
  if (typeof url !== 'string') return false;
  try {
    const u = new URL(url);
    return u.protocol === 'https:' && (u.hostname === 'steamstatic.com' || u.hostname.endsWith('.steamstatic.com'));
  } catch {
    return false;
  }
}

export function createSteamArt(opts: { fetch?: Fetch; now?: () => number } = {}) {
  const fetchFn: Fetch = opts.fetch ?? ((url, init) => fetch(url, init));
  const now = opts.now ?? Date.now;
  const cache = new Map<number, { expires: number; url: Promise<string | null> }>();

  async function resolve(appId: number): Promise<string | null> {
    const res = await fetchFn(`${APPDETAILS}?appids=${appId}&filters=basic`, { signal: AbortSignal.timeout(TIMEOUT_MS) });
    if (!res.ok) return null;
    const body = (await res.json()) as Record<string, { success?: boolean; data?: { header_image?: unknown } }>;
    const entry = body?.[String(appId)];
    const url = entry?.success ? entry.data?.header_image : null;
    return isSteamCdn(url) ? url : null;
  }

  /** The header image URL for a Steam app, or null when Steam has none (or can't be reached). */
  return function headerUrl(appId: number): Promise<string | null> {
    const hit = cache.get(appId);
    if (hit && hit.expires > now()) return hit.url;
    if (cache.size >= MAX_ENTRIES) cache.clear();
    const url = resolve(appId).catch(() => null);
    const entry = { expires: now() + MISSING_TTL_MS, url };
    cache.set(appId, entry);
    void url.then((u) => {
      if (u) entry.expires = now() + FOUND_TTL_MS;
    });
    return url;
  };
}

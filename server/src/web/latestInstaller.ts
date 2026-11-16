/**
 * The newest Windows installer, for the "Get the app" page's download button.
 *
 * The installer's file name carries its version, so there is no fixed GitHub URL for "the latest
 * setup.exe". The release's `latest.json` (the one the desktop updater reads) names it; lookups
 * are cached in memory, and any failure falls back to the latest release's page.
 */

const REPO = 'https://github.com/cRaZy92/klipt';
export const RELEASES_PAGE = `${REPO}/releases/latest`;
const LATEST_JSON = `${REPO}/releases/latest/download/latest.json`;
const FOUND_TTL_MS = 10 * 60 * 1000;
const FAILED_TTL_MS = 60 * 1000;
const TIMEOUT_MS = 5000;

type Fetch = (url: string, init?: { signal?: AbortSignal }) => Promise<Response>;

/** Only installers from this repo's releases are redirected to. */
function isReleaseAsset(url: unknown): url is string {
  return typeof url === 'string' && url.startsWith(`${REPO}/releases/download/`) && url.endsWith('-setup.exe');
}

export function createLatestInstaller(opts: { fetch?: Fetch; now?: () => number } = {}) {
  const fetchFn: Fetch = opts.fetch ?? ((url, init) => fetch(url, init));
  const now = opts.now ?? Date.now;
  let cached: { expires: number; url: Promise<string | null> } | null = null;

  async function resolve(): Promise<string | null> {
    try {
      const res = await fetchFn(LATEST_JSON, { signal: AbortSignal.timeout(TIMEOUT_MS) });
      if (!res.ok) return null;
      const body = (await res.json()) as { platforms?: Record<string, { url?: unknown }> };
      const url = body.platforms?.['windows-x86_64']?.url;
      return isReleaseAsset(url) ? url : null;
    } catch {
      return null;
    }
  }

  /** The installer's URL, or the latest release's page when it can't be found. */
  return async function latestInstaller(): Promise<string> {
    if (!cached || cached.expires <= now()) {
      const url = resolve();
      cached = { expires: now() + FOUND_TTL_MS, url };
      void url.then((u) => {
        if (u === null && cached?.url === url) cached.expires = now() + FAILED_TTL_MS;
      });
    }
    return (await cached.url) ?? RELEASES_PAGE;
  };
}

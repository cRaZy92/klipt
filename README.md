# Klipt

Klipt is a small, self-hosted alternative to Medal.tv. Friends get an invite, upload game clips from the desktop app, browse a shared feed, and paste share links into Discord, where they show up as playable embeds. Everything runs as one Docker Compose stack (Postgres plus the Klipt server) that you host yourself.

## Deploy on Dokploy

1. In Dokploy, create a new **Compose** service and pick **GitHub** as the source: repository `cRaZy92/klipt`, branch `main`, compose file `docker-compose.yml`.
2. Open the **Environment** tab and set:

   | Variable | Meaning |
   |---|---|
   | `BASE_URL` | The public address, e.g. `https://clips.example.com`. Must be the public **https** URL, because share links and secure cookies are built from it. |
   | `ADMIN_USERNAME` | Name of the first (admin) account. |
   | `ADMIN_PASSWORD` | Password of the admin account (at least 8 characters). |
   | `POSTGRES_PASSWORD` | Any long random password for the database. Set it once: changing it after the first deploy has no effect, because the `pgdata` volume is already initialised with the old password. |
   | `MAX_UPLOAD_MB` | Optional. Largest clip allowed, default `2048`. |
   | `KLIPT_PORT` | Optional. Host port the server is published on, default `3260`. Point your tunnel or proxy (e.g. Pangolin) at `<host IP>:<KLIPT_PORT>`. |
   | `TRANSCODE` | Optional. `on` (default) re-encodes uploaded clips to H.264 in the background so they take much less space and still play everywhere (older iPhones too); `off` stores uploads as they are. Progress is on the admin page. |
   | `TRANSCODE_CRF` | Optional. Compression quality, default `23`. Lower is better quality and bigger files (`20` ≈ near the original, `26` ≈ noticeably smaller). |
   | `TRANSCODE_PRESET` | Optional. x264 speed, default `medium`. `fast` uses less CPU for slightly bigger files; `slow` saves ~2% more for ~40% more CPU time. |
   | `TRANSCODE_THREADS` | Optional. CPU threads compression may use, default `2`, so it leaves the rest of the server alone (it also runs at low priority). On an i5-10600KF expect roughly 1.5 minutes of work per minute of 1080p60 clip with 2 threads. |
   | `TRUST_PROXY_HOPS` | Optional. How many reverse proxies sit in front of Klipt, default `1`. Use `1` behind Dokploy/Traefik, `0` if Klipt is exposed directly with no proxy, `2` if Cloudflare is in front of Traefik. |

3. **Without Dokploy's Traefik** (e.g. a Pangolin tunnel): skip the Domains tab and point the tunnel at `<Dokploy host IP>:<KLIPT_PORT>` with HTTPS terminated by the tunnel; keep `TRUST_PROXY_HOPS=1` (Pangolin's proxy is one hop). **With Dokploy's Traefik:** open the **Domains** tab, add your domain, and set service name `server`, port `3000`, and enable **HTTPS** (Let's Encrypt). Point the domain's DNS at your Dokploy server first.
4. Press **Deploy**. The first build takes a few minutes.

## First login

Open your `BASE_URL`, sign in with `ADMIN_USERNAME` / `ADMIN_PASSWORD`. The admin account is only created the first time, when there are no users yet. Changing these variables later does not change the password.

## Inviting friends

Klipt is invite-only. As admin, open the admin page, create an invite, and send the invite link to your friend. They pick a username and password and are in. Invites can be revoked from the same page.

## Where your data lives

Two Docker volumes hold everything:

- `pgdata`: the database (accounts, clip info).
- `klipt-data`: the clip files and thumbnails.

Backup hint: back up both volumes together, for example with Dokploy's volume backups or `docker run --rm -v <project>_klipt-data:/d -v $PWD:/b alpine tar czf /b/klipt-data.tgz -C /d .` (and a `pg_dump` for the database).

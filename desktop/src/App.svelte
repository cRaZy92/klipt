<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorMessage, on, onWindowHidden, type Account, type ClipView, type FeedbackKind, type HotkeyStatus, type Settings, type SettingsSaved } from "./lib/api";
  import { menu } from "./lib/a11y";
  import { hotkeyLabel } from "./lib/format";
  import { toast } from "./lib/toasts.svelte";
  import { isBusy, loadPendingUploads, resumeAfterLogin, stopUploadsForLogout, uploads, watchUploads } from "./lib/uploads.svelte";
  import { importing, watchImports } from "./lib/imports.svelte";
  import { jobs, runningCount, watchJobs } from "./lib/jobs.svelte";
  import { forgetCache } from "./lib/cache";
  import { keepScroll } from "./lib/keepScroll";
  import Avatar from "./components/Avatar.svelte";
  import Icon from "./components/Icon.svelte";
  import Toast from "./components/Toast.svelte";
  import Modal from "./components/Modal.svelte";
  import FeedbackDialog from "./components/FeedbackDialog.svelte";
  import DropZones from "./components/DropZones.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import Editor from "./views/Editor.svelte";
  import Feed from "./views/Feed.svelte";
  import Albums from "./views/Albums.svelte";
  import Jobs from "./views/Jobs.svelte";
  import Friends from "./views/Friends.svelte";
  import Library from "./views/Library.svelte";
  import Login from "./views/Login.svelte";
  import Onboarding from "./views/Onboarding.svelte";
  import SettingsView from "./views/Settings.svelte";
  import Achievements from "./views/Achievements.svelte";
  import Appearance from "./views/Appearance.svelte";
  import Profile from "./views/Profile.svelte";
  import UserHoverCard from "./components/UserHoverCard.svelte";
  import { forgetProfiles, hideCard, setProfileOpener } from "./lib/profiles.svelte";
  import { applyLooks } from "./lib/looks";

  // The screens. Library is home.
  type Screen = "library" | "feed" | "albums" | "friends" | "jobs" | "settings" | "achievements" | "appearance";
  type View =
    | { name: "library" }
    | { name: "feed" }
    | { name: "albums" }
    | { name: "friends" }
    | { name: "jobs" }
    | { name: "editor"; clip: ClipView }
    | { name: "settings" }
    | { name: "achievements" }
    | { name: "appearance" }
    /** Someone's profile, built per person; Back returns to `from`. */
    | { name: "profile"; username: string; from: View };
  /** Where the user asked to go while the current screen had unsaved changes. */
  type Leaving = { to: "view"; view: View } | { to: "logout" };

  let account = $state<Account | null>(null);
  let settings = $state<Settings | null>(null);
  let ready = $state(false);
  let startupError = $state<string | null>(null);
  let loginNotice = $state<string | null>(null);
  let view = $state<View>({ name: "library" });
  /** The save-clip shortcut as the system reports it, or why it isn't working. */
  let hotkeyStatus = $state<HotkeyStatus>({ display: null, error: null, errorCode: null });
  let accountMenu = $state(false);
  /** The "Report a problem" dialog from the account menu. */
  let feedbackKind = $state<FeedbackKind | null>(null);
  /** A newer version waiting to be installed; `command` installs it on Linux. */
  let update = $state<{ version: string; command: string | null; installing: boolean } | null>(null);
  /** Settings has unsaved changes (the editor saves by itself). */
  let viewDirty = $state(false);
  /** Friends' clips you haven't seen (the Feed tab's badge). */
  let feedNew = $state(0);
  /** Friend requests waiting for an answer (the rail's Friends badge). */
  let friendRequests = $state(0);
  function countRequests() {
    api.listFriends().then((l) => (friendRequests = l.incoming.length)).catch(() => {});
  }
  let leaving = $state<Leaving | null>(null);

  // Your accent and theme, everywhere (the defaults while logged out).
  $effect(() => applyLooks(account?.looks));

  const hotkey = $derived(hotkeyStatus.display ?? (settings ? hotkeyLabel(settings.hotkey) : ""));

  async function start() {
    startupError = null;
    try {
      const [s, a, status] = await Promise.all([api.getSettings(), api.getAccount(), api.getHotkeyStatus()]);
      settings = s;
      account = a;
      // Events fired before this window listened are lost; the core keeps the latest state.
      hotkeyStatus = status;
      if (status.error) toast(status.error, "error");
      ready = true;
      if (a) syncAccount();
    } catch (e) {
      startupError = errorMessage(e);
    }
  }

  onMount(() => {
    void start();
    api.feedNewCount().then((n) => (feedNew = n)).catch(() => {});
    // Announced before this window opened (it's made again after being released in the tray).
    api
      .pendingUpdate()
      .then((u) => u && !update && (update = { ...u, installing: false }))
      .catch(() => {});
    void loadPendingUploads();
    const stops = [
      on("save-busy", () => toast("Already saving a clip…")),
      on("recorder-error", (e) => toast(e.message, "error")),
      on("hotkey-error", (e) => {
        hotkeyStatus = { display: null, error: e.message, errorCode: e.code };
        toast(e.message, "error");
      }),
      on(
        "hotkey-bound",
        ({ display, screenshotDisplay, screenshotError, recordDisplay, recordError }) =>
          (hotkeyStatus = {
            display,
            error: null,
            errorCode: null,
            screenshotDisplay,
            screenshotError,
            recordDisplay,
            recordError,
          }),
      ),
      on("clip-saved", (clip) =>
        toast(clip.kind === "screenshot" ? "Screenshot saved" : clip.isRecording ? "Recording saved" : "Clip saved", "success"),
      ),
      // An achievement earned while the window is in front (in the background it's a notification).
      on("achievement", ({ title, body }) =>
        toast(`${title}. ${body}`, "success", { label: "View", run: () => go({ name: "achievements" }) }),
      ),
      on("update-available", ({ version, command }) => (update = { version, command, installing: false })),
      on("feed-new", ({ count }) => (feedNew = count)),
      // Closed to the tray or minimized: nothing keeps playing unseen (the editor pauses itself).
      onWindowHidden(() => {
        for (const media of document.querySelectorAll<HTMLMediaElement>("video, audio")) media.pause();
      }),
      // Hidden in the tray for a while: let the core close the window to free its memory, unless
      // that would lose something. Uploads and imports run on in the core, but their progress
      // lives here.
      on("release-window-check", () => {
        const busy =
          view.name === "editor" ||
          viewDirty ||
          importing.phase !== "idle" ||
          Object.values(uploads).some((job) => isBusy(job));
        if (!busy) api.releaseWindow().catch(() => {});
      }),
      watchImports(),
      watchJobs(),
      watchUploads({
        // The login expired mid-upload: log in again, then the upload carries on.
        unauthorized: () => {
          account = null;
          loginNotice = "Your session has expired. Log in again and your upload will continue.";
        },
        notice: ({ title, link, copied }) =>
          toast(
            copied ? "Link copied!" : title,
            "success",
            link
              ? {
                  label: "Copy link",
                  run: () =>
                    api
                      .copyText(link)
                      .then(() => toast("Link copied!", "success"))
                      .catch((e) => toast(errorMessage(e), "error")),
                }
              : undefined,
          ),
      }),
    ];
    return () => stops.forEach((stop) => stop());
  });

  async function installUpdate() {
    if (!update) return;
    update.installing = true;
    try {
      await api.installUpdate(); // restarts Klipt when it succeeds
    } catch (e) {
      update.installing = false;
      toast(errorMessage(e), "error");
    }
  }

  async function copyUpdateCommand() {
    if (!update?.command) return;
    try {
      await api.copyText(update.command);
      toast("Command copied. Paste it into a terminal.", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  /** A name changed on the website shows up here; the server also learns which version runs here. */
  function syncAccount() {
    countRequests();
    api
      .refreshAccount()
      .then((fresh) => {
        if (fresh && account && fresh.serverUrl === account.serverUrl && fresh.username === account.username) account = fresh;
      })
      .catch(() => {});
  }

  function loggedIn(a: Account) {
    account = a;
    loginNotice = null;
    if (settings) settings = { ...settings, serverUrl: a.serverUrl };
    // `view` is kept, so the user is back where they were (e.g. the editor) after re-login.
    resumeAfterLogin();
  }

  /** Logs out, asking first when that would throw away unsaved changes. */
  function requestLogout() {
    accountMenu = false;
    if (viewDirty) {
      leaving = { to: "logout" };
      return;
    }
    void logout();
  }

  async function logout() {
    accountMenu = false;
    // Logging out on purpose starts the next login on Clips (an expired session keeps `view`).
    viewDirty = false;
    view = { name: "library" };
    stopUploadsForLogout();
    try {
      await api.logout();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
    account = null;
  }

  /** Opens someone's profile over the current screen (a name or picture was clicked). */
  function openProfile(username: string) {
    accountMenu = false;
    if (view.name === "profile" && view.username.toLowerCase() === username.toLowerCase()) return;
    go({ name: "profile", username, from: view });
  }
  setProfileOpener(openProfile);

  /** The screens kept alive (hidden while another is shown); the editor and profiles are never kept. */
  let visited = $state<Set<Screen>>(new Set(["library"]));
  /** Bumped to rebuild Settings from the saved settings (after "Discard"). */
  let settingsEpoch = $state(0);

  $effect(() => {
    const name = view.name;
    if (name !== "editor" && name !== "profile" && !untrack(() => visited.has(name))) visited = new Set([...untrack(() => visited), name]);
  });

  // Logged out, or the login expired: the kept screens are dropped (they're unmounted with the
  // logged-in UI) and the next account mustn't see this one's data. `view` stays, so a re-login
  // after an expired session returns to where the user was.
  $effect(() => {
    if (account) return;
    const name = untrack(() => view.name);
    visited = new Set(name === "editor" || name === "profile" ? [] : [name]);
    forgetCache();
    forgetProfiles();
    hideCard();
  });

  /** Switches screens, asking first when that would throw away unsaved changes. */
  function go(next: View) {
    if (next.name === view.name && next.name !== "editor" && next.name !== "profile") return;
    hideCard();
    if (viewDirty && next !== view) {
      leaving = { to: "view", view: next };
      return;
    }
    viewDirty = false;
    view = next;
  }

  function discardAndLeave() {
    const next = leaving;
    leaving = null;
    if (!next) return;
    viewDirty = false;
    // Rebuild Settings from the saved settings, so coming back doesn't show the discarded draft.
    if (view.name === "settings") settingsEpoch++;
    if (next.to === "logout") {
      view = { name: "library" };
      void logout();
    } else {
      view = next.view;
    }
  }

  function settingsSaved(saved: Settings, result: SettingsSaved) {
    settings = saved;
    if (result.hotkeyDisplay) {
      hotkeyStatus = {
        display: result.hotkeyDisplay,
        error: null,
        errorCode: null,
        screenshotDisplay: result.screenshotHotkeyDisplay,
        screenshotError: result.screenshotHotkeyError,
        recordDisplay: result.recordHotkeyDisplay,
        recordError: result.recordHotkeyError,
      };
    }
  }

  function hostOf(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  function closeAccountMenu(e: MouseEvent) {
    if (accountMenu && !(e.target as Element).closest?.(".account")) accountMenu = false;
  }

  function openWebsite() {
    accountMenu = false;
    api.openWebsite().catch((e) => toast(errorMessage(e), "error"));
  }

  const inClips = $derived(view.name === "library" || view.name === "editor");
  const runningJobs = $derived(runningCount(jobs.list));
</script>

<svelte:window
  onfocus={() => account && countRequests()}
  onclick={closeAccountMenu}
  onkeydown={(e) => {
    if (e.key === "Escape") accountMenu = false;
  }}
/>

{#if !ready}
  <div class="center">
    {#if startupError}
      <p>{startupError}</p>
      <button class="btn" onclick={start}>Try again</button>
    {:else}
      <span class="spinner"></span>
    {/if}
  </div>
{:else if settings && !settings.onboarded}
  <Onboarding {settings} {hotkeyStatus} ondone={settingsSaved} />
{:else if !account}
  <Login initialServer={settings?.serverUrl ?? null} notice={loginNotice} onlogin={loggedIn} />
{:else}
  <div class="shell">
    <nav class="rail" aria-label="Sections">
      <button class="brand" onclick={() => go({ name: "library" })} title="Klipt" aria-label="Klipt, my clips">
        <Icon name="logo" size={26} />
      </button>

      <div class="nav">
        <button
          class="item"
          class:active={inClips}
          aria-current={inClips ? "page" : undefined}
          onclick={() => go({ name: "library" })}
          title={importing.phase === "running" && importing.progress
            ? `Importing clips: ${Math.round(importing.progress.fraction * 100)}%`
            : undefined}
        >
          <span class="item-icon">
            <Icon name="film" size={20} />
            {#if importing.phase === "running"}
              <span class="import-pct" aria-hidden="true">
                {importing.progress ? `${Math.round(importing.progress.fraction * 100)}%` : "…"}
              </span>
            {/if}
          </span>
          Clips
        </button>
        <button
          class="item"
          class:active={view.name === "feed"}
          aria-current={view.name === "feed" ? "page" : undefined}
          onclick={() => go({ name: "feed" })}
          aria-label={feedNew > 0 ? `Feed, ${feedNew} new` : undefined}
        >
          <span class="item-icon">
            <Icon name="users" size={20} />
            {#if feedNew > 0}<span class="new-count" aria-hidden="true">{feedNew > 9 ? "9+" : feedNew}</span>{/if}
          </span>
          Feed
        </button>
        <button
          class="item"
          class:active={view.name === "albums"}
          aria-current={view.name === "albums" ? "page" : undefined}
          onclick={() => go({ name: "albums" })}
        >
          <span class="item-icon"><Icon name="album" size={20} /></span>
          Albums
        </button>
        <button
          class="item"
          class:active={view.name === "friends"}
          aria-current={view.name === "friends" ? "page" : undefined}
          onclick={() => go({ name: "friends" })}
        >
          <span class="item-icon">
            <Icon name="user" size={20} />
            {#if friendRequests > 0}<span class="new-count" aria-hidden="true">{friendRequests > 9 ? "9+" : friendRequests}</span>{/if}
          </span>
          Friends
        </button>
        <button
          class="item"
          class:active={view.name === "jobs"}
          aria-current={view.name === "jobs" ? "page" : undefined}
          onclick={() => go({ name: "jobs" })}
          aria-label={runningJobs > 0 ? `Jobs, ${runningJobs} running` : undefined}
        >
          <span class="item-icon">
            <Icon name="pulse" size={20} />
            {#if runningJobs > 0}<span class="new-count" aria-hidden="true">{runningJobs > 9 ? "9+" : runningJobs}</span>{/if}
          </span>
          Jobs
        </button>
      </div>

      <div class="nav bottom">
        <button
          class="item"
          class:active={view.name === "achievements"}
          aria-current={view.name === "achievements" ? "page" : undefined}
          onclick={() => go({ name: "achievements" })}
        >
          <span class="item-icon"><Icon name="trophy" size={20} /></span>
          Achievements
        </button>
        <button
          class="item"
          class:active={view.name === "appearance"}
          aria-current={view.name === "appearance" ? "page" : undefined}
          onclick={() => go({ name: "appearance" })}
        >
          <span class="item-icon"><Icon name="palette" size={20} /></span>
          Appearance
        </button>
        <button
          class="item"
          class:active={view.name === "settings"}
          aria-current={view.name === "settings" ? "page" : undefined}
          onclick={() => go({ name: "settings" })}
        >
          <span class="item-icon"><Icon name="settings" size={20} /></span>
          Settings
        </button>
        <div class="account">
          <button
            class="me"
            onclick={() => (accountMenu = !accountMenu)}
            aria-expanded={accountMenu}
            aria-haspopup="menu"
            aria-label="Account: {account.displayName}"
            title={account.displayName}
          >
            <Avatar name={account.displayName} url={account.avatarUrl} frame={account.looks?.frame} size={36} />
          </button>
          {#if accountMenu}
            <div class="menu" role="menu" use:menu={{ close: () => (accountMenu = false) }}>
              <div class="who">
                <Avatar name={account.displayName} url={account.avatarUrl} frame={account.looks?.frame} size={38} />
                <div>
                  <strong>{account.displayName}</strong>
                  <span>{hostOf(account.serverUrl)}</span>
                </div>
              </div>
              <hr />
              <button role="menuitem" onclick={() => openProfile(account!.username)}><Icon name="user" size={16} /> Your profile</button>
              <button role="menuitem" onclick={openWebsite}><Icon name="external" size={16} /> Open website</button>
              <button role="menuitem" onclick={() => ((accountMenu = false), (feedbackKind = "bug"))}>
                <Icon name="alert" size={16} /> Report a problem
              </button>
              <button role="menuitem" onclick={requestLogout}><Icon name="logout" size={16} /> Log out</button>
            </div>
          {/if}
        </div>
      </div>
    </nav>

    <div class="content">
      {#if update}
        <div class="update-bar" role="status">
          <span class="update-icon"><Icon name="download" size={18} /></span>
          {#if update.command}
            <span class="update-text"><strong>Klipt {update.version}</strong> is out. To install it, run in a terminal:</span>
            <code class="update-command" title={update.command}>{update.command}</code>
            <button class="btn btn-sm btn-ghost" onclick={() => (update = null)}>Later</button>
            <button class="btn btn-sm btn-primary" onclick={copyUpdateCommand}>Copy</button>
          {:else}
            <span class="update-text"><strong>Klipt {update.version}</strong> is ready to install.</span>
            <button class="btn btn-sm btn-ghost" onclick={() => (update = null)} disabled={update.installing}>Later</button>
            <button class="btn btn-sm btn-primary" onclick={installUpdate} disabled={update.installing}>
              {#if update.installing}<span class="spinner"></span> Installing…{:else}Restart to install{/if}
            </button>
          {/if}
        </div>
      {/if}

      <!-- Screens stay alive once visited and are only hidden (see .screen:not(.here)), so switching back is instant and
           keeps scroll and filters. Each has its own scroll area. The editor is built per clip. -->
      {#if visited.has("library")}
        <main class="screen" class:here={view.name === "library"} use:keepScroll={view.name === "library"} inert={view.name !== "library"}>
          <Library
            active={view.name === "library"}
            {hotkey}
            bufferSeconds={settings?.bufferSeconds ?? 30}
            cleanupDeletesFavorites={!!settings?.cleanupEnabled && !!settings?.cleanupFavorites}
            onedit={(clip) => go({ name: "editor", clip })}
          />
        </main>
      {/if}
      {#if visited.has("feed")}
        <main class="screen" class:here={view.name === "feed"} use:keepScroll={view.name === "feed"} inert={view.name !== "feed"}>
          <Feed
            active={view.name === "feed"}
            server={hostOf(account.serverUrl)}
            me={account.username}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to see the feed.";
            }}
          />
        </main>
      {/if}
      {#if visited.has("albums")}
        <main class="screen" class:here={view.name === "albums"} use:keepScroll={view.name === "albums"} inert={view.name !== "albums"}>
          <Albums
            active={view.name === "albums"}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to see your albums.";
            }}
          />
        </main>
      {/if}
      {#if visited.has("friends")}
        <main class="screen" class:here={view.name === "friends"} use:keepScroll={view.name === "friends"} inert={view.name !== "friends"}>
          <Friends
            active={view.name === "friends"}
            onrequests={(n) => (friendRequests = n)}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to see your friends.";
            }}
          />
        </main>
      {/if}
      {#if visited.has("jobs")}
        <main class="screen" class:here={view.name === "jobs"} use:keepScroll={view.name === "jobs"} inert={view.name !== "jobs"}>
          <Jobs active={view.name === "jobs"} />
        </main>
      {/if}
      {#if view.name === "editor"}
        <main class="screen here">
          {#key view.clip.id}
            <Editor clip={view.clip} onclose={() => go({ name: "library" })} ondeleted={() => (view = { name: "library" })} />
          {/key}
        </main>
      {/if}
      {#if view.name === "profile"}
        <main class="screen here">
          {#key view.username}
            <Profile
              username={view.username}
              onback={() => view.name === "profile" && go(view.from)}
              onunauthorized={() => {
                account = null;
                loginNotice = "Your session has expired. Log in again to see this profile.";
              }}
            />
          {/key}
        </main>
      {/if}
      {#if visited.has("achievements")}
        <main class="screen" class:here={view.name === "achievements"} use:keepScroll={view.name === "achievements"} inert={view.name !== "achievements"}>
          <Achievements
            active={view.name === "achievements"}
            name={account.displayName}
            avatarUrl={account.avatarUrl}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to see your achievements.";
            }}
          />
        </main>
      {/if}
      {#if visited.has("appearance")}
        <main class="screen" class:here={view.name === "appearance"} use:keepScroll={view.name === "appearance"} inert={view.name !== "appearance"}>
          <Appearance
            active={view.name === "appearance"}
            {account}
            onaccount={(a) => (account = a)}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to change your appearance.";
            }}
          />
        </main>
      {/if}
      {#if visited.has("settings") && settings}
        <main class="screen" class:here={view.name === "settings"} use:keepScroll={view.name === "settings"} inert={view.name !== "settings"}>
          {#key settingsEpoch}
            <SettingsView
              active={view.name === "settings"}
              {settings}
              {account}
              {hotkeyStatus}
              bind:dirty={viewDirty}
              onsaved={settingsSaved}
              onlogout={requestLogout}
              onaccount={(a) => (account = a)}
              onappearance={() => go({ name: "appearance" })}
            />
          {/key}
        </main>
      {/if}
    </div>
  </div>
{/if}

{#if ready && account}
  <!-- App-wide, so dropping files and the import's confirmation work on every screen. -->
  <DropZones />
  <ImportDialog />
{/if}

{#if feedbackKind}
  <FeedbackDialog kind={feedbackKind} onclose={() => (feedbackKind = null)} />
{/if}

{#if leaving}
  <Modal title="Discard changes?" onclose={() => (leaving = null)}>
    <p>Your settings changes haven't been saved.</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (leaving = null)}>Keep editing</button>
      <button class="btn btn-danger" onclick={discardAndLeave}>Discard</button>
    {/snippet}
  </Modal>
{/if}

<Toast />
<UserHoverCard />

<style>
  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    height: 100%;
    color: var(--text-muted);
  }

  .shell {
    display: flex;
    height: 100%;
  }

  /* The navigation rail on the left. */
  .rail {
    position: relative;
    z-index: 20;
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: center;
    /* Wide enough for the longest label, "Achievements". */
    width: 5.75rem;
    padding: 0.9rem 0;
    border-right: 1px solid var(--border);
    background: var(--bg-2);
  }

  .brand {
    display: grid;
    place-items: center;
    width: 2.75rem;
    height: 2.75rem;
    padding: 0;
    border: none;
    border-radius: 13px;
    background: linear-gradient(145deg, var(--accent-hover), color-mix(in srgb, var(--accent) 88%, #000));
    box-shadow:
      0 8px 22px -8px var(--accent-glow),
      inset 0 1px 0 rgba(255, 255, 255, 0.22);
    color: #fff;
    cursor: pointer;
    transition: transform 0.15s var(--ease-out);
  }

  .brand:hover {
    transform: rotate(-4deg) scale(1.05);
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    width: 100%;
    margin-top: 1.4rem;
    padding: 0 0.3rem;
  }

  .nav.bottom {
    align-items: center;
    margin-top: auto;
  }

  .item {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    width: 100%;
    padding: 0.4rem 0 0.45rem;
    border: none;
    border-radius: 12px;
    background: none;
    color: var(--text-faint);
    font-size: 0.72rem;
    font-weight: 650;
    white-space: nowrap;
    cursor: pointer;
    transition: color 0.15s;
  }

  .item-icon {
    display: grid;
    place-items: center;
    width: 2.75rem;
    height: 2.1rem;
    border-radius: 11px;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .item:hover {
    color: var(--text);
  }

  .item-icon {
    position: relative;
  }

  /* A running import's progress on the Clips tab, so it's visible from every page. */
  .import-pct {
    position: absolute;
    top: -0.25rem;
    left: calc(50% + 0.15rem);
    padding: 0 0.3rem;
    white-space: nowrap;
    border: 2px solid var(--bg, #0b0a10);
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--accent-text);
    font-size: 0.62rem;
    font-weight: 750;
    line-height: 0.95rem;
    font-variant-numeric: tabular-nums;
  }

  /* Sits on the glyph's top-right corner (the glyph is 20px, centred), overlapping it only
     slightly; anchored by `left` so a wider "9+" grows outward rather than over the icon. */
  .new-count {
    position: absolute;
    top: -0.25rem;
    left: calc(50% + 0.15rem);
    min-width: 0.6rem;
    height: 0.95rem;
    padding: 0 0.2rem;
    border: 2px solid var(--bg, #0b0a10);
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 0.6rem;
    font-weight: 750;
    line-height: 0.95rem;
    text-align: center;
    font-variant-numeric: tabular-nums;
    box-sizing: content-box;
  }

  .item:hover .item-icon {
    background: var(--surface-2);
  }

  .item.active {
    color: var(--text);
  }

  .item.active .item-icon {
    background: var(--accent-soft);
    color: var(--accent-text);
  }

  .item.active::before {
    content: "";
    position: absolute;
    top: 0.65rem;
    left: -0.5rem;
    width: 3px;
    height: 1.6rem;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
    box-shadow: 0 0 10px var(--accent-glow);
  }

  .account {
    position: relative;
    margin-top: 0.6rem;
  }

  /* A short window: tighter rail, so all seven sections and your picture still fit. */
  @media (max-height: 720px) {
    .rail {
      padding: 0.6rem 0;
    }
    .nav {
      gap: 0.1rem;
      margin-top: 0.8rem;
    }
    .item {
      gap: 0.1rem;
      padding: 0.2rem 0 0.25rem;
    }
    .item-icon {
      height: 1.9rem;
    }
    .account {
      margin-top: 0.3rem;
    }
  }

  .me {
    display: grid;
    place-items: center;
    padding: 2px;
    border: 2px solid transparent;
    border-radius: 50%;
    background: none;
    cursor: pointer;
    transition: border-color 0.15s;
  }

  .me:hover,
  .me[aria-expanded="true"] {
    border-color: var(--border-strong);
  }

  .menu {
    position: absolute;
    bottom: 0;
    left: calc(100% + 0.75rem);
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 15rem;
    padding: 0.35rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-2);
    box-shadow: var(--shadow);
    animation: menu-in 0.15s var(--ease-out);
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateX(-4px);
    }
  }

  .who {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0.5rem 0.55rem 0.45rem;
  }

  .who div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .who span {
    overflow: hidden;
    color: var(--text-muted);
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  hr {
    width: 100%;
    margin: 0.3rem 0;
    border: none;
    border-top: 1px solid var(--border);
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--surface-3);
    color: var(--text);
  }

  /* Everything right of the rail. A faint glow sits behind the top of every page. */
  .content {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    background:
      radial-gradient(60rem 22rem at 20% -6rem, color-mix(in srgb, var(--accent) 11%, transparent), transparent 70%),
      var(--bg);
  }

  .update-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex: none;
    margin: 0.85rem 2rem 0;
    padding: 0.55rem 0.6rem 0.55rem 0.7rem;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: var(--radius);
    background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 24%, transparent), color-mix(in srgb, var(--accent) 6%, transparent));
    animation: rise 0.35s var(--ease-out);
  }

  .update-icon {
    display: grid;
    place-items: center;
    width: 2rem;
    height: 2rem;
    border-radius: 9px;
    background: var(--accent);
    color: #fff;
  }

  .update-text {
    margin-right: auto;
  }

  .update-bar:has(.update-command) .update-text {
    flex: none;
    margin-right: 0;
  }

  .update-command {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    padding: 0.3rem 0.5rem;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.25);
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: all;
  }

  .screen {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /* A screen kept alive while another is shown. Not display: none: that would replay every entrance
     animation (the cards' staggered rise) each time it's shown again. Hidden and laid on top
     instead; content-visibility skips drawing it, and `inert` keeps it out of focus and clicks. */
  .screen:not(.here) {
    position: absolute;
    inset: 0;
    visibility: hidden;
    content-visibility: hidden;
  }
</style>

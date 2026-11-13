<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, on, type Account, type ClipView, type HotkeyStatus, type Settings, type SettingsSaved } from "./lib/api";
  import { menu } from "./lib/a11y";
  import { hotkeyLabel } from "./lib/format";
  import { toast } from "./lib/toasts.svelte";
  import { resumeAfterLogin, stopUploadsForLogout, watchUploads } from "./lib/uploads.svelte";
  import { importing, watchImports } from "./lib/imports.svelte";
  import { forgetLibrary } from "./lib/libraryMemory";
  import Avatar from "./components/Avatar.svelte";
  import Icon from "./components/Icon.svelte";
  import Toast from "./components/Toast.svelte";
  import Modal from "./components/Modal.svelte";
  import Editor from "./views/Editor.svelte";
  import Feed from "./views/Feed.svelte";
  import Library from "./views/Library.svelte";
  import Login from "./views/Login.svelte";
  import SettingsView from "./views/Settings.svelte";

  // The screens. Library is home.
  type View = { name: "library" } | { name: "feed" } | { name: "editor"; clip: ClipView } | { name: "settings" };
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
  /** A newer version waiting to be installed; `command` installs it on Linux. */
  let update = $state<{ version: string; command: string | null; installing: boolean } | null>(null);
  /** The current screen (editor or settings) has unsaved changes. */
  let viewDirty = $state(false);
  /** Friends' clips you haven't seen (the Feed tab's badge). */
  let feedNew = $state(0);
  let leaving = $state<Leaving | null>(null);

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
    } catch (e) {
      startupError = errorMessage(e);
    }
  }

  onMount(() => {
    void start();
    api.feedNewCount().then((n) => (feedNew = n)).catch(() => {});
    const stops = [
      on("save-busy", () => toast("Already saving a clip…")),
      on("recorder-error", (e) => toast(e.message, "error")),
      on("hotkey-error", (e) => {
        hotkeyStatus = { display: null, error: e.message, errorCode: e.code };
        toast(e.message, "error");
      }),
      on(
        "hotkey-bound",
        ({ display, screenshotDisplay, screenshotError }) =>
          (hotkeyStatus = { display, error: null, errorCode: null, screenshotDisplay, screenshotError }),
      ),
      on("clip-saved", (clip) => toast(clip.kind === "screenshot" ? "Screenshot saved" : "Clip saved", "success")),
      on("update-available", ({ version, command }) => (update = { version, command, installing: false })),
      on("feed-new", ({ count }) => (feedNew = count)),
      watchImports(),
      watchUploads({
        // The login expired mid-upload: log in again, then the upload carries on.
        unauthorized: () => {
          account = null;
          loginNotice = "Your session has expired. Log in again and your upload will continue.";
        },
        done: (_url, copied) =>
          toast(copied ? "Link copied!" : "Uploaded! Use “Copy link” on the clip to share it.", "success"),
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
    stopUploadsForLogout();
    try {
      await api.logout();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
    account = null;
  }

  // Logged out, or the login expired: the next account mustn't see this one's clips and uploads.
  $effect(() => {
    if (!account) forgetLibrary();
  });

  /** Switches screens, asking first when that would throw away unsaved changes. */
  function go(next: View) {
    if (next.name === view.name && next.name !== "editor") return;
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
</script>

<svelte:window
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
      </div>

      <div class="nav bottom">
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
            aria-label="Account: {account.username}"
            title={account.username}
          >
            <Avatar name={account.username} size={36} />
          </button>
          {#if accountMenu}
            <div class="menu" role="menu" use:menu={{ close: () => (accountMenu = false) }}>
              <div class="who">
                <Avatar name={account.username} size={38} />
                <div>
                  <strong>{account.username}</strong>
                  <span>{hostOf(account.serverUrl)}</span>
                </div>
              </div>
              <hr />
              <button role="menuitem" onclick={openWebsite}><Icon name="external" size={16} /> Open website</button>
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

      <main>
        {#if view.name === "library"}
          <Library
            {hotkey}
            bufferSeconds={settings?.bufferSeconds ?? 30}
            onedit={(clip) => go({ name: "editor", clip })}
          />
        {:else if view.name === "feed"}
          <Feed
            server={hostOf(account.serverUrl)}
            onunauthorized={() => {
              account = null;
              loginNotice = "Your session has expired. Log in again to see the feed.";
            }}
          />
        {:else if view.name === "editor"}
          {#key view.clip.id}
            <Editor
            clip={view.clip}
            bind:dirty={viewDirty}
            onclose={() => go({ name: "library" })}
            ondeleted={() => {
              viewDirty = false;
              view = { name: "library" };
            }}
          />
          {/key}
        {:else if settings}
          <SettingsView
            {settings}
            {account}
            {hotkeyStatus}
            bind:dirty={viewDirty}
            onsaved={settingsSaved}
            onlogout={requestLogout}
          />
        {/if}
      </main>
    </div>
  </div>
{/if}

{#if leaving}
  <Modal title="Discard changes?" onclose={() => (leaving = null)}>
    {#if view.name === "settings"}
      <p>Your settings changes haven't been saved.</p>
    {:else}
      <p>Your trim and volume changes to this clip haven't been saved.</p>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (leaving = null)}>Keep editing</button>
      <button class="btn btn-danger" onclick={discardAndLeave}>Discard</button>
    {/snippet}
  </Modal>
{/if}

<Toast />

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
    width: 5rem;
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
    background: linear-gradient(145deg, #9479ff, #6a4bf0);
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
    padding: 0 0.5rem;
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
    top: -0.3rem;
    right: -0.9rem;
    padding: 0 0.3rem;
    border: 2px solid var(--bg, #0b0a10);
    border-radius: 999px;
    background: var(--surface-3, #2a2638);
    color: var(--accent-text, #c9bcff);
    font-size: 0.62rem;
    font-weight: 750;
    line-height: 0.95rem;
    font-variant-numeric: tabular-nums;
  }

  .new-count {
    position: absolute;
    top: -0.2rem;
    right: 0.1rem;
    min-width: 1.1rem;
    height: 1.1rem;
    padding: 0 0.3rem;
    border: 2px solid var(--bg, #0b0a10);
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 0.66rem;
    font-weight: 750;
    line-height: 0.75rem;
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
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    background:
      radial-gradient(60rem 22rem at 20% -6rem, rgba(124, 92, 255, 0.11), transparent 70%),
      var(--bg);
  }

  .update-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex: none;
    margin: 0.85rem 2rem 0;
    padding: 0.55rem 0.6rem 0.55rem 0.7rem;
    border: 1px solid rgba(124, 92, 255, 0.35);
    border-radius: var(--radius);
    background: linear-gradient(90deg, rgba(124, 92, 255, 0.24), rgba(124, 92, 255, 0.06));
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

  main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
</style>

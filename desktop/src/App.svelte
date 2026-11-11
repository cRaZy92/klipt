<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, on, type Account, type ClipView, type HotkeyStatus, type Settings } from "./lib/api";
  import { hotkeyLabel } from "./lib/format";
  import { toast } from "./lib/toasts.svelte";
  import { resumeAfterLogin, watchUploads } from "./lib/uploads.svelte";
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
  /** A newer version waiting to be installed (Windows). */
  let update = $state<{ version: string; installing: boolean } | null>(null);
  /** The current screen (editor or settings) has unsaved changes. */
  let viewDirty = $state(false);
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
    const stops = [
      on("save-busy", () => toast("Already saving a clip…")),
      on("recorder-error", (e) => toast(e.message, "error")),
      on("hotkey-error", (e) => {
        hotkeyStatus = { display: null, error: e.message, errorCode: e.code };
        toast(e.message, "error");
      }),
      on("hotkey-bound", ({ display }) => (hotkeyStatus = { display, error: null, errorCode: null })),
      on("clip-saved", () => toast("Clip saved", "success")),
      on("update-available", ({ version }) => (update = { version, installing: false })),
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
    try {
      await api.logout();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
    account = null;
  }

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

  function settingsSaved(saved: Settings, hotkeyDisplay: string | null) {
    settings = saved;
    if (hotkeyDisplay) hotkeyStatus = { display: hotkeyDisplay, error: null, errorCode: null };
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
</script>

<svelte:window onclick={closeAccountMenu} />

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
    <header class="topbar">
      <button class="brand" onclick={() => go({ name: "library" })}>
        <span class="logo"><Icon name="record" size={16} /></span>
        Klipt
      </button>
      <nav class="tabs" aria-label="Sections">
        <button class:active={view.name === "library" || view.name === "editor"} onclick={() => go({ name: "library" })}>
          My clips
        </button>
        <button class:active={view.name === "feed"} onclick={() => go({ name: "feed" })}>Feed</button>
      </nav>
      <div class="right">
        <button
          class="btn btn-ghost btn-icon"
          aria-label="Settings"
          title="Settings"
          onclick={() => go({ name: "settings" })}
        >
          <Icon name="settings" />
        </button>
        <div class="account">
          <button class="btn btn-ghost" onclick={() => (accountMenu = !accountMenu)} aria-expanded={accountMenu}>
            <span class="avatar">{account.username.slice(0, 1).toUpperCase()}</span>
            {account.username}
          </button>
          {#if accountMenu}
            <div class="menu" role="menu">
              <p class="server muted">Logged in to {hostOf(account.serverUrl)}</p>
              <button role="menuitem" onclick={requestLogout}><Icon name="logout" size={16} /> Log out</button>
            </div>
          {/if}
        </div>
      </div>
    </header>

    {#if update}
      <div class="update-bar" role="status">
        <span>Update available: Klipt {update.version}</span>
        <button class="btn btn-primary" onclick={installUpdate} disabled={update.installing}>
          {#if update.installing}<span class="spinner"></span> Installing…{:else}Restart to install{/if}
        </button>
        <button class="btn btn-ghost" onclick={() => (update = null)} disabled={update.installing}>Later</button>
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
          onunauthorized={() => {
            account = null;
            loginNotice = "Your session has expired. Log in again to see the feed.";
          }}
        />
      {:else if view.name === "editor"}
        {#key view.clip.id}
          <Editor clip={view.clip} bind:dirty={viewDirty} onclose={() => go({ name: "library" })} />
        {/key}
      {:else if settings}
        <SettingsView
          {settings}
          {account}
          {hotkeyStatus}
          bind:dirty={viewDirty}
          onsaved={settingsSaved}
          onlogout={requestLogout}
          onclose={() => go({ name: "library" })}
        />
      {/if}
    </main>
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
    flex-direction: column;
    height: 100%;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    height: 3.5rem;
    padding: 0 1rem 0 1.25rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .update-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex: none;
    padding: 0.5rem 1.25rem;
    border-bottom: 1px solid var(--border);
    background: var(--accent-soft);
  }

  .update-bar span {
    margin-right: auto;
    font-weight: 600;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-right: auto;
    margin-left: 1.5rem;
  }

  .tabs button {
    padding: 0.4rem 0.85rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .tabs button:hover {
    color: var(--text);
    background: var(--surface-2);
  }

  .tabs button.active {
    color: var(--text);
    background: var(--surface-3);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0;
    border: none;
    background: none;
    font-size: 1.1rem;
    font-weight: 750;
    letter-spacing: -0.01em;
    cursor: pointer;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 1.8rem;
    height: 1.8rem;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .account {
    position: relative;
  }

  .avatar {
    display: grid;
    place-items: center;
    width: 1.6rem;
    height: 1.6rem;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 0.8rem;
  }

  .menu {
    position: absolute;
    top: calc(100% + 0.35rem);
    right: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 13rem;
    padding: 0.3rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    box-shadow: var(--shadow);
  }

  .server {
    padding: 0.45rem 0.6rem;
    font-size: 0.82rem;
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.6rem;
    border: none;
    border-radius: 6px;
    background: none;
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--surface-3);
  }

  main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
</style>

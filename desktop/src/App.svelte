<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, on, type Account, type ClipView, type Settings } from "./lib/api";
  import { hotkeyLabel } from "./lib/format";
  import { toast } from "./lib/toasts.svelte";
  import { resumeAfterLogin, watchUploads } from "./lib/uploads.svelte";
  import Icon from "./components/Icon.svelte";
  import Toast from "./components/Toast.svelte";
  import Library from "./views/Library.svelte";
  import Login from "./views/Login.svelte";

  // The screens. Library is home; the editor (Task 11) and settings (Task 12) plug in here.
  type View = { name: "library" } | { name: "editor"; clip: ClipView } | { name: "settings" };

  let account = $state<Account | null>(null);
  let settings = $state<Settings | null>(null);
  let ready = $state(false);
  let startupError = $state<string | null>(null);
  let loginNotice = $state<string | null>(null);
  let view = $state<View>({ name: "library" });
  let hotkeyDisplay = $state<string | null>(null);
  let accountMenu = $state(false);

  const hotkey = $derived(hotkeyDisplay ?? (settings ? hotkeyLabel(settings.hotkey) : ""));

  async function start() {
    startupError = null;
    try {
      const [s, a, status] = await Promise.all([api.getSettings(), api.getAccount(), api.getHotkeyStatus()]);
      settings = s;
      account = a;
      // Events fired before this window listened are lost; the core keeps the latest state.
      if (status.display) hotkeyDisplay = status.display;
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
      on("hotkey-error", (e) => toast(e.message, "error")),
      on("hotkey-bound", ({ display }) => (hotkeyDisplay = display)),
      on("clip-saved", () => toast("Clip saved", "success")),
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

  function loggedIn(a: Account) {
    account = a;
    loginNotice = null;
    if (settings) settings = { ...settings, serverUrl: a.serverUrl };
    // `view` is kept, so the user is back where they were (e.g. the editor) after re-login.
    resumeAfterLogin();
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
      <button class="brand" onclick={() => (view = { name: "library" })}>
        <span class="logo"><Icon name="record" size={16} /></span>
        Klipt
      </button>
      <div class="right">
        <!-- Task 12 replaces the placeholder settings view. -->
        <button
          class="btn btn-ghost btn-icon"
          aria-label="Settings"
          title="Settings"
          onclick={() => (view = { name: "settings" })}
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
              <button role="menuitem" onclick={logout}><Icon name="logout" size={16} /> Log out</button>
            </div>
          {/if}
        </div>
      </div>
    </header>

    <main>
      {#if view.name === "library"}
        <Library {hotkey} bufferSeconds={settings?.bufferSeconds ?? 30} />
      {:else}
        <div class="center">
          <p class="muted">This screen is coming soon.</p>
          <button class="btn" onclick={() => (view = { name: "library" })}>Back to clips</button>
        </div>
      {/if}
    </main>
  </div>
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
    flex: 1;
    overflow-y: auto;
  }
</style>

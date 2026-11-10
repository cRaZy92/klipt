<script lang="ts">
  import { api, errorMessage, type Account } from "../lib/api";
  import Icon from "../components/Icon.svelte";

  let {
    initialServer = null,
    notice = null,
    onlogin,
  }: {
    /** A server already checked (from Settings): skips straight to the login form. */
    initialServer?: string | null;
    /** Shown above the form, e.g. "Your session has expired…". */
    notice?: string | null;
    onlogin: (account: Account) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let server = $state<string | null>(initialServer);
  // svelte-ignore state_referenced_locally
  let address = $state(initialServer ?? "");
  let username = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function checkServer(e: SubmitEvent) {
    e.preventDefault();
    if (!address.trim()) {
      error = "Enter your server's address.";
      return;
    }
    busy = true;
    error = null;
    try {
      server = await api.checkServer(address);
      address = server;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function logIn(e: SubmitEvent) {
    e.preventDefault();
    if (!server) return;
    if (!username.trim() || !password) {
      error = "Enter your username and password.";
      return;
    }
    busy = true;
    error = null;
    try {
      const account = await api.login(server, username, password);
      password = "";
      onlogin(account);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  function changeServer() {
    server = null;
    error = null;
  }

  function hostOf(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
</script>

<div class="screen">
  <div class="card">
    <div class="brand">
      <div class="logo"><Icon name="record" size={26} /></div>
      <h1>Klipt</h1>
      <p class="muted">Save your best moments and share them with friends.</p>
    </div>

    {#if notice}
      <p class="notice">{notice}</p>
    {/if}

    {#if !server}
      <form onsubmit={checkServer}>
        <label class="field">
          <span>Server address</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input"
            bind:value={address}
            placeholder="clips.example.com"
            autocomplete="url"
            spellcheck="false"
            autofocus
          />
        </label>
        <p class="hint">Your friend who runs the Klipt server can tell you the address.</p>
        {#if error}<p class="form-error" role="alert">{error}</p>{/if}
        <button class="btn btn-primary btn-block" type="submit" disabled={busy}>
          {#if busy}<span class="spinner"></span> Checking…{:else}Continue{/if}
        </button>
      </form>
    {:else}
      <div class="server">
        <Icon name="server" size={16} />
        <span class="host" title={server}>{hostOf(server)}</span>
        <button class="btn btn-ghost btn-sm" type="button" onclick={changeServer} disabled={busy}>Change</button>
      </div>
      <form onsubmit={logIn}>
        <label class="field">
          <span>Username</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" bind:value={username} autocomplete="username" spellcheck="false" autofocus />
        </label>
        <label class="field">
          <span>Password</span>
          <input class="input" type="password" bind:value={password} autocomplete="current-password" />
        </label>
        {#if error}<p class="form-error" role="alert">{error}</p>{/if}
        <button class="btn btn-primary btn-block" type="submit" disabled={busy}>
          {#if busy}<span class="spinner"></span> Logging in…{:else}Log in{/if}
        </button>
      </form>
    {/if}

    <p class="invite">Don't have an account? Ask your admin for an invite link.</p>
  </div>
</div>

<style>
  .screen {
    display: grid;
    place-items: center;
    min-height: 100%;
    padding: 2rem;
    background:
      radial-gradient(60rem 30rem at 50% -10%, rgba(124, 92, 255, 0.18), transparent 70%),
      var(--bg);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    width: 24rem;
    max-width: 100%;
    padding: 2rem;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: var(--surface);
    box-shadow: var(--shadow);
  }

  .brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    text-align: center;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 3rem;
    height: 3rem;
    border-radius: 14px;
    background: var(--accent);
    color: #fff;
  }

  h1 {
    font-size: 1.6rem;
    letter-spacing: -0.02em;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  .hint {
    margin-top: -0.4rem;
    color: var(--text-faint);
    font-size: 0.85rem;
  }

  .notice {
    padding: 0.65rem 0.8rem;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font-size: 0.9rem;
  }

  .server {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.4rem 0.4rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text-muted);
  }

  .host {
    flex: 1;
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .invite {
    color: var(--text-muted);
    font-size: 0.87rem;
    text-align: center;
  }
</style>

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
  let username = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  // There's one Klipt server: no address to type. A login saved for another one keeps it.
  $effect(() => {
    if (!server) {
      api
        .getDefaultServer()
        .then((s) => (server = s))
        .catch((e) => (error = errorMessage(e)));
    }
  });

  function signUp() {
    if (server) api.openSignup(server).catch((e) => (error = errorMessage(e)));
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

</script>

<div class="screen">
  <aside class="hero">
    <div class="hero-inner">
      <div class="brand">
        <div class="logo"><Icon name="logo" size={34} /></div>
        <span class="wordmark">Klipt</span>
      </div>
      <h1>Your best moments, one key away.</h1>
      <ul class="features">
        <li>
          <span class="f-icon rec"><span class="rec-dot"></span></span>
          <div><strong>Always ready</strong><small>Klipt records in the background. Press the hotkey to keep what just happened.</small></div>
        </li>
        <li>
          <span class="f-icon"><Icon name="scissors" size={18} /></span>
          <div><strong>Trim in seconds</strong><small>Cut it down and balance game sound and your mic.</small></div>
        </li>
        <li>
          <span class="f-icon"><Icon name="users" size={18} /></span>
          <div><strong>Share with friends</strong><small>Clips show up in your friends' feed and play right in Discord.</small></div>
        </li>
      </ul>
    </div>
  </aside>

  <div class="panel">
    <div class="form-card">
      <div class="head">
        <h2>Log in</h2>
        <p class="muted">
          No account yet? <button class="link-button" type="button" onclick={signUp} disabled={!server}>Create one</button> on the website.
        </p>
      </div>

      {#if notice}
        <p class="notice">{notice}</p>
      {/if}

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
        <button class="btn btn-primary btn-block btn-lg" type="submit" disabled={busy || !server}>
          {#if busy}<span class="spinner"></span> Logging in…{:else}Log in{/if}
        </button>
      </form>
    </div>
  </div>
</div>

<style>
  .screen {
    display: grid;
    grid-template-columns: minmax(20rem, 42%) 1fr;
    min-height: 100%;
  }

  .hero {
    position: relative;
    display: flex;
    align-items: center;
    overflow: hidden;
    padding: 3rem 3rem 3rem 3.25rem;
    border-right: 1px solid var(--border);
    background:
      radial-gradient(36rem 28rem at -10% -10%, color-mix(in srgb, var(--accent) 42%, transparent), transparent 65%),
      radial-gradient(28rem 24rem at 110% 115%, rgba(255, 77, 109, 0.2), transparent 65%),
      var(--bg-2);
  }

  /* A faint dot grid, fading out towards the bottom. */
  .hero::before {
    content: "";
    position: absolute;
    inset: 0;
    background-image: radial-gradient(rgba(255, 255, 255, 0.07) 1px, transparent 1px);
    background-size: 22px 22px;
    mask-image: linear-gradient(180deg, #000, transparent 85%);
    pointer-events: none;
  }

  .hero-inner {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
    max-width: 26rem;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 3rem;
    height: 3rem;
    border-radius: 14px;
    background: linear-gradient(145deg, var(--accent-hover), color-mix(in srgb, var(--accent) 88%, #000));
    box-shadow:
      0 10px 28px -8px var(--accent-glow),
      inset 0 1px 0 rgba(255, 255, 255, 0.22);
    color: #fff;
  }

  .wordmark {
    font-family: var(--font-display);
    font-size: 1.6rem;
    font-weight: 750;
    letter-spacing: -0.02em;
  }

  h1 {
    font-size: 2.3rem;
    font-weight: 750;
    line-height: 1.08;
  }

  .features {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .features li {
    display: flex;
    align-items: flex-start;
    gap: 0.85rem;
  }

  .features div {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .features small {
    color: var(--text-muted);
    font-size: 0.88rem;
    line-height: 1.4;
  }

  .f-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 2.25rem;
    height: 2.25rem;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 11px;
    background: var(--accent-soft);
    color: var(--accent-text);
  }

  .f-icon.rec {
    border-color: rgba(255, 77, 109, 0.3);
    background: var(--rec-soft);
  }

  .panel {
    display: grid;
    place-items: center;
    padding: 2rem;
  }

  .form-card {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    width: 22rem;
    max-width: 100%;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  h2 {
    font-size: 1.6rem;
    font-weight: 750;
  }

  .head p {
    font-size: 0.92rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
  }

  .input {
    height: 2.85rem;
  }

  .btn-lg {
    height: 2.85rem;
    margin-top: 0.25rem;
    font-size: 1rem;
  }

  .notice {
    padding: 0.7rem 0.85rem;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    font-size: 0.9rem;
  }

  .link-button {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .link-button:hover:not(:disabled) {
    text-decoration: underline;
  }

  @media (max-width: 760px) {
    .screen {
      grid-template-columns: 1fr;
    }

    .hero {
      display: none;
    }
  }
</style>

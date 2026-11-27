<script lang="ts">
  import { api, errorCode, errorMessage, nameOf, type FriendList, type Person, type Referral } from "../lib/api";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "../components/Avatar.svelte";
  import Icon from "../components/Icon.svelte";

  // Your friends, requests either way, adding by username, and referral links (a link someone
  // signs up with makes you friends; it works once, for 14 days).

  let {
    active = true,
    onunauthorized,
    onrequests,
  }: {
    /** This is the screen shown (it's kept alive, hidden, while another one is). */
    active?: boolean;
    onunauthorized: () => void;
    /** How many requests wait for an answer, after each load (for the rail's badge). */
    onrequests: (count: number) => void;
  } = $props();

  let list = $state<FriendList | null>(null);
  let referrals = $state<Referral[]>([]);
  let loadError = $state<string | null>(null);
  let busy = $state<Set<string>>(new Set());
  let addName = $state("");
  let adding = $state(false);
  let making = $state(false);

  function failed(e: unknown) {
    if (errorCode(e) === "unauthorized") onunauthorized();
    else toast(errorMessage(e), "error");
  }

  async function load() {
    try {
      const [friends, links] = await Promise.all([api.listFriends(), api.listReferrals()]);
      list = friends;
      referrals = links;
      loadError = null;
      onrequests(friends.incoming.length);
    } catch (e) {
      if (errorCode(e) === "unauthorized") onunauthorized();
      else loadError = errorMessage(e);
    }
  }

  $effect(() => {
    if (active) void load();
  });

  async function act(name: string, run: () => Promise<unknown>, done: string) {
    busy = new Set([...busy, name]);
    try {
      await run();
      toast(done, "success");
      await load();
    } catch (e) {
      failed(e);
    } finally {
      busy = new Set([...busy].filter((n) => n !== name));
    }
  }

  const accept = (p: Person) => act(p.username, () => api.addFriend(p.username), `You and ${nameOf(p)} are friends now`);
  const decline = (p: Person) => act(p.username, () => api.removeFriend(p.username), "Request declined");
  const cancel = (p: Person) => act(p.username, () => api.removeFriend(p.username), "Request cancelled");
  const unfriend = (p: Person) => act(p.username, () => api.removeFriend(p.username), `${nameOf(p)} is no longer your friend`);

  async function addByName(e: SubmitEvent) {
    e.preventDefault();
    const name = addName.trim();
    if (!name) return;
    adding = true;
    try {
      const { state } = await api.addFriend(name);
      toast(state === "friends" ? "You are friends now" : "Request sent", "success");
      addName = "";
      await load();
    } catch (err) {
      failed(err);
    } finally {
      adding = false;
    }
  }

  async function makeLink() {
    making = true;
    try {
      const link = await api.makeReferral();
      await api.copyText(link.url);
      toast("Link copied. It works once, for 14 days.", "success");
      await load();
    } catch (e) {
      failed(e);
    } finally {
      making = false;
    }
  }

  async function copyLink(r: Referral) {
    try {
      await api.copyText(r.url);
      toast("Link copied.", "success");
    } catch (e) {
      failed(e);
    }
  }

  const revoke = (r: Referral) => act(r.code, () => api.revokeReferral(r.code), "Link revoked");

  const until = (iso: string) => new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });
</script>

{#snippet person(p: Person, actions: import("svelte").Snippet)}
  <li class="person">
    <Avatar name={nameOf(p)} url={p.avatarUrl ?? null} frame={p.frame ?? null} size={36} />
    <span class="who">
      <strong>{nameOf(p)}</strong>
      {#if nameOf(p) !== p.username}<small class="muted">@{p.username}</small>{/if}
    </span>
    <span class="actions">{@render actions()}</span>
  </li>
{/snippet}

<div class="friends">
  <header>
    <h1>Friends</h1>
    <p class="muted">Friends see each other's clips in the feed. Everyone else only finds your public ones.</p>
  </header>

  {#if loadError}
    <p class="error">{loadError} <button class="btn btn-sm" onclick={load}>Try again</button></p>
  {/if}

  {#if list && list.incoming.length > 0}
    <section class="card">
      <h2>Requests</h2>
      <ul>
        {#each list.incoming as p (p.username)}
          {#snippet incomingActions()}
            <button class="btn btn-primary btn-sm" disabled={busy.has(p.username)} onclick={() => accept(p)}>Accept</button>
            <button class="btn btn-sm" disabled={busy.has(p.username)} onclick={() => decline(p)}>Decline</button>
          {/snippet}
          {@render person(p, incomingActions)}
        {/each}
      </ul>
    </section>
  {/if}

  <section class="card">
    <h2>Bring a friend</h2>
    <p class="muted">Make a link and send it. Whoever signs up with it becomes your friend. Each link works once, for 14 days.</p>
    <button class="btn btn-primary" disabled={making} onclick={makeLink}><Icon name="link" size={16} /> Make link</button>
    {#if referrals.length > 0}
      <ul class="links">
        {#each referrals as r (r.code)}
          <li>
            <code>{r.url}</code>
            <small class="muted">until {until(r.expiresAt)}</small>
            <button class="btn btn-sm" onclick={() => copyLink(r)}>Copy</button>
            <button class="btn btn-ghost btn-sm" disabled={busy.has(r.code)} onclick={() => revoke(r)}>Revoke</button>
          </li>
        {/each}
      </ul>
    {/if}
    <form class="add" onsubmit={addByName}>
      <input class="input" bind:value={addName} placeholder="Add by username" maxlength="24" autocomplete="off" aria-label="Username" />
      <button class="btn" type="submit" disabled={adding || !addName.trim()}>Send request</button>
    </form>
  </section>

  <section class="card">
    <h2>Your friends{#if list && list.friends.length > 0}<span class="muted"> · {list.friends.length}</span>{/if}</h2>
    {#if list && list.friends.length === 0}
      <p class="muted">No friends yet. Make a link above and send it to someone.</p>
    {:else if list}
      <ul>
        {#each list.friends as p (p.username)}
          {#snippet friendActions()}
            <button class="btn btn-ghost btn-sm" disabled={busy.has(p.username)} onclick={() => unfriend(p)}>Unfriend</button>
          {/snippet}
          {@render person(p, friendActions)}
        {/each}
      </ul>
    {:else if !loadError}
      <p class="muted">Loading…</p>
    {/if}
  </section>

  {#if list && list.outgoing.length > 0}
    <section class="card">
      <h2>Waiting for an answer</h2>
      <ul>
        {#each list.outgoing as p (p.username)}
          {#snippet outgoingActions()}
            <button class="btn btn-ghost btn-sm" disabled={busy.has(p.username)} onclick={() => cancel(p)}>Cancel</button>
          {/snippet}
          {@render person(p, outgoingActions)}
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .friends {
    max-width: 760px;
    margin: 0 auto;
    padding: 28px 32px 48px;
    display: grid;
    gap: 16px;
  }
  header h1 {
    margin: 0 0 4px;
  }
  .card {
    padding: 1.2rem 1.4rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: linear-gradient(180deg, var(--surface), rgba(21, 20, 29, 0.55));
  }
  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .person {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .links {
    margin-top: 12px;
  }
  .links li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .links code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
  .add .input {
    flex: 1;
  }
  .error {
    color: var(--danger-text, var(--danger));
  }
</style>

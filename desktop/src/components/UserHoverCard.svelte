<script lang="ts">
  import { api, errorMessage, type UserProfile } from "../lib/api";
  import { forgetProfiles, hideCard, holdCard, hoverCard, loadProfile, openProfile, releaseCard } from "../lib/profiles.svelte";
  import { toast } from "../lib/toasts.svelte";
  import Avatar from "./Avatar.svelte";

  // The mini profile over a name or picture marked `use:userHover` (lib/profiles.svelte.ts).
  // One for the whole app, in App.svelte.

  const WIDTH = 300;
  const GAP = 8;

  let height = $state(0);
  let busy = $state(false);

  const profile = $derived(hoverCard.profile);

  /** Below what it points at, or above when there's no room below; kept inside the window. */
  const position = $derived.by(() => {
    const a = hoverCard.anchor;
    if (!a) return { left: 0, top: 0 };
    const left = Math.min(Math.max(GAP, a.left), window.innerWidth - WIDTH - GAP);
    const below = a.bottom + GAP;
    const fitsBelow = below + height <= window.innerHeight - GAP;
    const top = fitsBelow || a.top - height - GAP < GAP ? below : a.top - height - GAP;
    return { left, top };
  });

  const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
  const plural = (n: number, one: string) => `${n === 1 ? "1" : compact.format(n)} ${one}${n === 1 ? "" : "s"}`;

  async function friend(p: UserProfile, add: boolean) {
    busy = true;
    try {
      if (add) await api.addFriend(p.user.username);
      else await api.removeFriend(p.user.username);
      forgetProfiles();
      const fresh = await loadProfile(p.user.username, true);
      if (hoverCard.profile?.user.username === p.user.username) hoverCard.profile = fresh;
      const was = p.friendState;
      toast(
        fresh.friendState === "friends" ? `You and ${p.user.displayName} are friends now`
        : fresh.friendState === "outgoing" ? "Friend request sent"
        : was === "incoming" ? "Request declined"
        : "Request cancelled",
        "success",
      );
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && profile) hideCard();
  }
</script>

<svelte:window {onkeydown} onscroll={hideCard} onblur={hideCard} />
<svelte:document onscrollcapture={hideCard} />

{#if profile}
  <div
    bind:clientHeight={height}
    class="hovercard banner-{profile.banner ?? 'none'}"
    class:has-banner={!!profile.banner}
    style:left="{position.left}px"
    style:top="{position.top}px"
    style:width="{WIDTH}px"
    role="dialog"
    aria-label={profile.user.displayName}
    tabindex="-1"
    onmouseenter={holdCard}
    onmouseleave={releaseCard}
  >
    <div class="banner"></div>
    <button class="head" onclick={() => openProfile(profile.user.username)} title="Open profile">
      <span class="ring"><Avatar name={profile.user.displayName} url={profile.user.avatarUrl ?? null} frame={profile.user.frame ?? null} size={56} /></span>
      <span class="names">
        <strong>{profile.user.displayName}</strong>
        <span class="handle">@{profile.user.username}</span>
      </span>
    </button>

    {#if profile.stats}
      <p class="line">{plural(profile.stats.clips, "clip")} · {plural(profile.stats.views, "view")}</p>
    {:else if !profile.full}
      <p class="line">Only friends see everything {profile.user.displayName} posts.</p>
    {/if}

    {#if profile.badges.length > 0}
      <ul class="badges" aria-label="Achievements">
        {#each profile.badges.slice(0, 4) as b (b.id)}
          <li title="{b.name}: {b.description}"><span aria-hidden="true">{b.icon}</span>{b.name}</li>
        {/each}
        {#if profile.badges.length > 4}<li class="more">+{profile.badges.length - 4}</li>{/if}
      </ul>
    {/if}

    <div class="actions">
      {#if profile.friendState === "none"}
        <button class="btn btn-primary btn-sm" disabled={busy} onclick={() => friend(profile, true)}>Add friend</button>
      {:else if profile.friendState === "outgoing"}
        <button class="btn btn-sm" disabled={busy} onclick={() => friend(profile, false)}>Requested · Cancel</button>
      {:else if profile.friendState === "incoming"}
        <button class="btn btn-primary btn-sm" disabled={busy} onclick={() => friend(profile, true)}>Accept</button>
        <button class="btn btn-sm" disabled={busy} onclick={() => friend(profile, false)}>Decline</button>
      {:else if profile.friendState === "friends"}
        <span class="friends">Friends</span>
      {/if}
      <button class="view" onclick={() => openProfile(profile.user.username)}>View profile</button>
    </div>
  </div>
{/if}

<style>
  .hovercard {
    position: fixed;
    z-index: 200;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--surface-2);
    box-shadow:
      0 0 0 1px var(--border-strong),
      var(--shadow);
    animation: card-in 0.16s var(--ease-out);
  }
  @keyframes card-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .hovercard {
      animation: none;
    }
  }
  .banner {
    height: 60px;
    background: linear-gradient(120deg, var(--accent-soft), var(--surface-3));
  }
  .has-banner .banner {
    background: var(--banner);
  }
  .head {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    width: calc(100% - 32px);
    margin: -26px 16px 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ring {
    display: grid;
    border-radius: 50%;
    box-shadow: 0 0 0 3px var(--surface-2);
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding-bottom: 2px;
  }
  .names strong {
    overflow: hidden;
    font-family: var(--font-display);
    font-size: 1.1rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head:hover strong {
    text-decoration: underline;
    text-decoration-color: var(--accent);
    text-underline-offset: 3px;
  }
  .handle {
    overflow: hidden;
    color: var(--text-faint);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line {
    margin: 12px 16px 0;
    color: var(--text-muted);
    font-size: 0.88rem;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 10px 16px 0;
    padding: 0;
    list-style: none;
  }
  .badges li {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px 3px 6px;
    border-radius: 999px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--border);
    color: var(--text-muted);
    font-size: 0.78rem;
    font-weight: 600;
  }
  .badges li.more {
    padding: 3px 4px;
    background: none;
    box-shadow: none;
    color: var(--text-faint);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 16px 16px;
  }
  .friends {
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--success-soft);
    color: var(--success);
    font-size: 0.8rem;
    font-weight: 650;
  }
  .view {
    margin-left: auto;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
  }
  .view:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
</style>

<script lang="ts">
  import { FRAMES } from "@klipt/shared";
  import { hueOf } from "../lib/format";
  import { avatarState } from "../lib/looks";

  // A friend's picture, or a round initial in a colour of their own; `frame` is the ring they wear
  // (looks.css), drawn from 28 px up.
  let {
    name,
    size = 28,
    url = null,
    frame = null,
  }: { name: string; size?: number; url?: string | null; frame?: string | null } = $props();

  const initial = $derived((Array.from(name.trim())[0] ?? "?").toUpperCase());
  let failed = $state(false);
  // A new picture gets its own chance to load.
  $effect(() => {
    void url;
    failed = false;
  });
  const ring = $derived(size >= 28 && frame && FRAMES.some((f) => f.id === frame) ? frame : null);
</script>

<span
  class="avatar frame-{ring ?? 'none'}"
  class:frame={!!ring}
  class:big={size >= 64}
  style:--hue={hueOf(name)}
  style:--size="{size}px"
  aria-hidden="true"
>
  {initial}
  {#if avatarState(url, failed) === "image"}
    <img src={url} alt="" draggable="false" onerror={() => (failed = true)} />
  {/if}
</span>

<style>
  .avatar {
    position: relative;
    display: grid;
    place-items: center;
    flex: none;
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    background: linear-gradient(140deg, hsl(var(--hue) 75% 62%), hsl(calc(var(--hue) + 35) 65% 42%));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.25);
    color: #fff;
    font-size: calc(var(--size) * 0.44);
    font-weight: 700;
    line-height: 1;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
    user-select: none;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
  }
</style>

<script lang="ts">
  import { hueOf } from "../lib/format";

  // A round initial in a colour of its own, so friends are told apart at a glance.
  let { name, size = 28 }: { name: string; size?: number } = $props();

  const initial = $derived((Array.from(name.trim())[0] ?? "?").toUpperCase());
</script>

<span class="avatar" style:--hue={hueOf(name)} style:--size="{size}px" aria-hidden="true">{initial}</span>

<style>
  .avatar {
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
</style>

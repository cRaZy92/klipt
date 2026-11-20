<script lang="ts">
  import Icon from "./Icon.svelte";

  // A search field that reports what's typed once typing pauses (Escape clears it).
  let {
    value = $bindable(""),
    placeholder,
    label,
    delay = 0,
    onsearch,
    input = $bindable(),
    shortcut,
  }: {
    value?: string;
    placeholder: string;
    label: string;
    /** Milliseconds to wait after the last key before `onsearch`; 0 reports at once. */
    delay?: number;
    onsearch?: (value: string) => void;
    /** The text field, e.g. to focus it from a shortcut. */
    input?: HTMLInputElement;
    /** The key that jumps here, shown in the empty field (the page handles the key). */
    shortcut?: string;
  } = $props();

  let timer: ReturnType<typeof setTimeout> | undefined;

  function report() {
    clearTimeout(timer);
    if (!onsearch) return;
    if (delay > 0) timer = setTimeout(() => onsearch(value), delay);
    else onsearch(value);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && value) {
      // Clear the field, not the dialog or page the Escape would otherwise close.
      e.stopPropagation();
      value = "";
      clearTimeout(timer);
      onsearch?.("");
    }
  }

  $effect(() => () => clearTimeout(timer));
</script>

<label class="search">
  <Icon name="search" size={16} />
  <input type="search" bind:this={input} bind:value oninput={report} {onkeydown} {placeholder} aria-label={label} aria-keyshortcuts={shortcut} maxlength="100" />
  {#if shortcut && !value}<kbd aria-hidden="true">{shortcut}</kbd>{/if}
</label>

<style>
  .search {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    width: min(20rem, 100%);
    padding: 0 0.75rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    transition: border-color 0.15s;
  }

  .search:hover {
    border-color: var(--border-strong);
  }

  .search:focus-within {
    border-color: var(--accent);
    color: var(--text);
  }

  kbd {
    padding: 0 0.4rem;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.35;
  }

  .search:focus-within kbd {
    display: none;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 0.45rem 0;
    border: none;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 0.9rem;
  }
</style>

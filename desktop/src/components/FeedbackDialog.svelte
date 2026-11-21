<script lang="ts">
  import { api, errorMessage, type FeedbackKind, type InfoLine } from "../lib/api";
  import { toast } from "../lib/toasts.svelte";
  import Modal from "./Modal.svelte";

  let {
    kind: initialKind = "bug",
    onclose,
  }: {
    kind?: FeedbackKind;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let kind = $state<FeedbackKind>(initialKind);
  let message = $state("");
  // svelte-ignore state_referenced_locally
  let includeLogs = $state(initialKind === "bug");
  let sending = $state(false);
  let error = $state<string | null>(null);
  let info = $state<InfoLine[] | null>(null);
  let showInfo = $state(false);

  api.systemInfo().then((lines) => (info = lines)).catch(() => (info = []));

  function pick(next: FeedbackKind) {
    // Logs help with problems; an idea doesn't need them unless asked for.
    if (next !== kind) includeLogs = next === "bug";
    kind = next;
  }

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (!message.trim()) {
      error = kind === "bug" ? "Say what happened." : "Say what you'd like.";
      return;
    }
    sending = true;
    error = null;
    try {
      await api.sendFeedback(kind, message, includeLogs);
      toast(kind === "bug" ? "Problem sent to your admin. Thanks!" : "Idea sent. Thanks!", "success");
      onclose();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      sending = false;
    }
  }
</script>

<Modal title={kind === "bug" ? "Report a problem" : "Suggest a feature"} {onclose} width="34rem" closeOnBackdrop={false}>
  <form class="feedback" onsubmit={send} id="feedback-form">
    <div class="segmented" role="radiogroup" aria-label="Kind">
      <button type="button" role="radio" aria-checked={kind === "bug"} class:on={kind === "bug"} onclick={() => pick("bug")}>
        Problem
      </button>
      <button type="button" role="radio" aria-checked={kind === "idea"} class:on={kind === "idea"} onclick={() => pick("idea")}>
        Idea
      </button>
    </div>
    <label class="field">
      <span>{kind === "bug" ? "What happened? What did you expect?" : "What would you like Klipt to do?"}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="input"
        rows="6"
        maxlength="10000"
        bind:value={message}
        autofocus
        placeholder={kind === "bug"
          ? "e.g. Pressing the shortcut in Valorant does nothing, no sound either."
          : "e.g. Let me pick a clip length when I save."}
      ></textarea>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={includeLogs} />
      <span>
        <strong>Include system info and the app's log</strong>
        <small>
          Your PC's OS, CPU, graphics card, recording settings and recent log lines (they contain file paths, never your
          password). Only your admin sees them.
          {#if info && info.length}
            <button type="button" class="link" onclick={() => (showInfo = !showInfo)}>
              {showInfo ? "Hide" : "Show"} what's sent
            </button>
          {/if}
        </small>
      </span>
    </label>
    {#if showInfo && info}
      <dl class="info">
        {#each info as line (line.label)}
          <dt>{line.label}</dt>
          <dd>{line.value}</dd>
        {/each}
        <dt>Log</dt>
        <dd>
          the last part of it
          <button type="button" class="link" onclick={() => api.openLogsFolder().catch((e) => toast(errorMessage(e), "error"))}>
            open the folder
          </button>
        </dd>
      </dl>
    {/if}
    {#if error}<p class="form-error" role="alert">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" type="button" onclick={onclose} disabled={sending}>Cancel</button>
    <button class="btn btn-primary" type="submit" form="feedback-form" disabled={sending}>
      {#if sending}<span class="spinner"></span> Sending…{:else}Send{/if}
    </button>
  {/snippet}
</Modal>

<style>
  .feedback {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .segmented {
    display: inline-flex;
    align-self: flex-start;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-2);
  }

  .segmented button {
    padding: 0.4rem 1rem;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .segmented button.on {
    background: var(--surface-3, var(--border));
    color: var(--text);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    font-size: 0.9rem;
    color: var(--text-muted);
  }

  textarea {
    resize: vertical;
    min-height: 7rem;
    padding: 0.6rem 0.75rem;
    font: inherit;
    line-height: 1.45;
  }

  .check {
    display: flex;
    gap: 0.65rem;
    align-items: flex-start;
    cursor: pointer;
  }

  .check input {
    margin-top: 0.2rem;
  }

  .check span {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .check small {
    color: var(--text-muted);
    line-height: 1.4;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text, var(--accent));
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }

  .info {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.25rem 1rem;
    margin: 0;
    padding: 0.75rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg-2);
    font-size: 0.82rem;
  }

  .info dt {
    color: var(--text-muted);
  }

  .info dd {
    margin: 0;
    word-break: break-word;
  }

  .form-error {
    margin: 0;
    color: var(--danger);
  }
</style>

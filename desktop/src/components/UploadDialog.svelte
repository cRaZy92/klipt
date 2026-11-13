<script lang="ts">
  import { api, errorMessage, type ClipView, type Visibility } from "../lib/api";
  import { uploadTitle } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import { fraction, isBusy, retryUpload, startUpload, statusLabel, uploads } from "../lib/uploads.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let { clip, onclose }: { clip: ClipView; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let title = $state(uploadTitle(clip));
  let visibility = $state<Visibility>("feed");
  /** Friends you can tag (everyone on the server but you); null until loaded or if it failed. */
  let friends = $state<string[] | null>(null);
  let tagged = $state<string[]>([]);

  $effect(() => {
    Promise.all([api.listUsers(), api.getAccount()])
      .then(([users, account]) => {
        const me = account?.username.toLowerCase();
        friends = users.map((u) => u.username).filter((name) => name.toLowerCase() !== me);
      })
      .catch(() => (friends = null));
  });

  function toggleTag(name: string) {
    tagged = tagged.includes(name) ? tagged.filter((n) => n !== name) : [...tagged, name];
  }
  // svelte-ignore state_referenced_locally
  let started = $state(isBusy(uploads[clip.id]));

  const job = $derived(started ? uploads[clip.id] : undefined);
  const busy = $derived(isBusy(job));
  const progress = $derived(job ? fraction(job) : null);
  const trimmedTitle = $derived(title.trim());

  const STEPS = [
    { status: "exporting", label: "Prepare" },
    { status: "uploading", label: "Upload" },
    { status: "finalizing", label: "Finish" },
  ] as const;
  const stepIndex = $derived(
    job?.status === "done" ? 3 : Math.max(0, STEPS.findIndex((s) => s.status === job?.status)),
  );

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!trimmedTitle) return;
    started = true;
    void startUpload(clip.id, trimmedTitle, visibility, tagged);
  }

  async function copyAgain() {
    try {
      await api.copyShareLink(clip.id);
      toast("Link copied!", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }
</script>

<Modal title={job?.status === "done" ? "Clip uploaded" : "Upload clip"} {onclose} closeOnBackdrop={false}>
  {#if !job}
    <form id="upload-form" onsubmit={submit}>
      <label class="field">
        <span>Title</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={title} maxlength="100" autofocus />
      </label>
      <fieldset>
        <legend>Who can see it</legend>
        <label class="option" class:selected={visibility === "feed"}>
          <input type="radio" name="visibility" value="feed" bind:group={visibility} />
          <span class="opt-icon"><Icon name="users" size={18} /></span>
          <span class="opt-text">
            <strong>Feed</strong>
            <small>Your friends see it in the feed.</small>
          </span>
          <span class="ring" aria-hidden="true"></span>
        </label>
        <label class="option" class:selected={visibility === "link"}>
          <input type="radio" name="visibility" value="link" bind:group={visibility} />
          <span class="opt-icon"><Icon name="link" size={18} /></span>
          <span class="opt-text">
            <strong>Link only</strong>
            <small>Only people you send the link to can watch it.</small>
          </span>
          <span class="ring" aria-hidden="true"></span>
        </label>
      </fieldset>
      {#if friends && friends.length > 0}
        <fieldset>
          <legend>Who's in it <span class="optional">optional</span></legend>
          <div class="friends">
            {#each friends as name (name)}
              <button
                type="button"
                class="friend"
                class:on={tagged.includes(name)}
                aria-pressed={tagged.includes(name)}
                onclick={() => toggleTag(name)}
              >
                {#if tagged.includes(name)}<Icon name="check" size={13} />{/if}
                {name}
              </button>
            {/each}
          </div>
        </fieldset>
      {/if}
    </form>
  {:else if job.status === "done"}
    <div class="done">
      <div class="done-icon"><Icon name="check" size={28} /></div>
      <p>{job.copied ? "The link is copied — paste it anywhere to share." : "Your clip is online."}</p>
      {#if job.shareUrl}<code class="link">{job.shareUrl}</code>{/if}
    </div>
  {:else}
    <div class="steps">
      {#each STEPS as step, i (step.status)}
        <div class="step" class:active={i === stepIndex && busy} class:complete={i < stepIndex}>
          <span class="bullet">{#if i < stepIndex}<Icon name="check" size={13} />{:else}{i + 1}{/if}</span>
          {step.label}
        </div>
      {/each}
    </div>
    <p class="status" class:failed={job.status === "failed"} role={job.status === "failed" ? "alert" : "status"}>
      {statusLabel(job)}
    </p>
    {#if busy}
      <div class="progress" class:indeterminate={progress === null} style:--value={progress ?? undefined}>
        <div></div>
      </div>
      <p class="muted small">You can close this window — the upload keeps going.</p>
    {/if}
  {/if}

  {#snippet footer()}
    {#if !job}
      <button class="btn" type="button" onclick={onclose}>Cancel</button>
      <button class="btn btn-primary" type="submit" form="upload-form" disabled={!trimmedTitle}>
        <Icon name="upload" size={16} /> Upload
      </button>
    {:else if job.status === "done"}
      {#if !job.copied}
        <button class="btn" onclick={copyAgain}><Icon name="link" size={16} /> Copy link</button>
      {/if}
      <button class="btn btn-primary" onclick={onclose}>Done</button>
    {:else if job.status === "failed"}
      <button class="btn" onclick={onclose}>Close</button>
      <button class="btn btn-primary" onclick={() => retryUpload(clip.id)}><Icon name="retry" size={16} /> Try again</button>
    {:else}
      <button class="btn" onclick={onclose}>Hide</button>
    {/if}
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    border: none;
  }

  legend {
    margin-bottom: 0.4rem;
    color: var(--text-muted);
    font-size: 0.87rem;
    font-weight: 600;
  }

  .optional {
    margin-left: 0.3rem;
    color: var(--text-faint);
    font-weight: 500;
  }

  .friends {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    max-height: 7.5rem;
    overflow-y: auto;
  }

  .friend {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.87rem;
    font-weight: 600;
    cursor: pointer;
    transition:
      border-color 0.15s,
      color 0.15s,
      background 0.15s;
  }

  .friend:hover {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .friend.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }

  .option {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.75rem 0.9rem 0.75rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg);
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s,
      box-shadow 0.15s;
  }

  .option:hover {
    border-color: var(--border-strong);
  }

  .option.selected {
    border-color: rgba(124, 92, 255, 0.7);
    background: var(--accent-softer);
    box-shadow: 0 0 0 3px var(--accent-softer);
  }

  /* The radio is drawn by `.ring`; the real one stays for keyboard and screen readers. */
  .option input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .option:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .opt-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 10px;
    background: var(--surface-3);
    color: var(--text-muted);
    transition:
      background 0.15s,
      color 0.15s;
  }

  .selected .opt-icon {
    background: var(--accent-soft);
    color: var(--accent-text);
  }

  .opt-text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.1rem;
  }

  .ring {
    flex: none;
    width: 1.15rem;
    height: 1.15rem;
    border: 2px solid var(--border-strong);
    border-radius: 50%;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }

  .selected .ring {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 3px var(--bg), inset 0 0 0 10px var(--accent);
  }

  small {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  .steps {
    display: flex;
    gap: 1.25rem;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--text-faint);
    font-size: 0.88rem;
    font-weight: 600;
  }

  .step.active {
    color: var(--text);
  }

  .step.complete {
    color: var(--success);
  }

  .bullet {
    display: grid;
    place-items: center;
    width: 1.4rem;
    height: 1.4rem;
    border: 1px solid currentColor;
    border-radius: 50%;
    font-size: 0.75rem;
  }

  .step.active .bullet {
    border-color: var(--accent);
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent-glow);
    color: #fff;
  }

  .step.complete .bullet {
    border-color: transparent;
    background: var(--success-soft);
  }

  .status {
    font-weight: 600;
  }

  .status.failed {
    color: var(--danger-text);
    font-weight: 500;
  }

  .small {
    font-size: 0.85rem;
  }

  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    text-align: center;
  }

  .done-icon {
    display: grid;
    place-items: center;
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 50%;
    background: var(--success-soft);
    box-shadow:
      0 0 0 8px rgba(60, 207, 142, 0.06),
      0 0 30px rgba(60, 207, 142, 0.25);
    color: var(--success);
    animation: done-pop 0.4s var(--ease-out);
  }

  @keyframes done-pop {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
  }

  .link {
    max-width: 100%;
    overflow: hidden;
    padding: 0.5rem 0.7rem;
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text-muted);
    font-size: 0.85rem;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: all;
  }
</style>

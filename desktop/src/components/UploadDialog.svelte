<script lang="ts">
  import { api, errorMessage, nameOf, type ClipView, type Visibility } from "../lib/api";
  import { toVisibility, VISIBILITY } from "../lib/visibility";
  import { uploadTitle } from "../lib/format";
  import { toast } from "../lib/toasts.svelte";
  import { fraction, isBusy, retryUpload, startUpload, statusLabel, transferLabel, uploads } from "../lib/uploads.svelte";
  import Avatar from "./Avatar.svelte";
  import AlbumPicker from "./AlbumPicker.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let { clip, onclose }: { clip: ClipView; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let title = $state(uploadTitle(clip));
  // svelte-ignore state_referenced_locally
  let visibility = $state<Visibility>(toVisibility(clip.visibility) ?? "friends");
  /** A clip uploaded before keeps its level; a new upload starts at your profile's setting. */
  // svelte-ignore state_referenced_locally
  const hadVisibility = toVisibility(clip.visibility) !== null;
  /** Already online: the new upload can replace the video at its link (a screenshot can't). */
  const replaceable = $derived(!!clip.serverClipId && clip.kind === "clip");
  let replace = $state(true);
  /** Friends you can tag; null until loaded or if it failed. */
  let friends = $state<string[] | null>(null);
  /** Display names by username, for the friend chips. */
  let names = $state<Record<string, string>>({});
  /** Pictures by username (null: the initial). */
  let pictures = $state<Record<string, string | null>>({});
  let tagged = $state<string[]>([]);
  /** Admins may keep a video as it is (no server compression): for edits and compilations. */
  let isAdmin = $state(false);
  let quality = $state<"standard" | "high">("standard");
  /** Albums it goes into once it's online. */
  let albums = $state<string[]>([]);

  $effect(() => {
    Promise.all([api.listUsers(), api.getAccount()])
      .then(([users, account]) => {
        isAdmin = !!account?.isAdmin;
        if (!hadVisibility && account) visibility = account.profileVisibility;
        const me = account?.username.toLowerCase();
        names = Object.fromEntries(users.map((u) => [u.username, nameOf(u)]));
        pictures = Object.fromEntries(users.map((u) => [u.username, u.avatarUrl ?? null]));
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
  // Preparing the link is the end of finishing up.
  const stepIndex = $derived(
    job?.status === "done"
      ? 3
      : job?.status === "preparing"
        ? 2
        : Math.max(0, STEPS.findIndex((s) => s.status === job?.status)),
  );

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!trimmedTitle) return;
    started = true;
    const high = isAdmin && clip.kind === "clip" && quality === "high";
    void startUpload(clip.id, trimmedTitle, visibility, tagged, replaceable && replace, false, high, albums);
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

<Modal title={job?.status === "done" ? (job.replaced ? "Video replaced" : "Clip uploaded") : "Upload clip"} {onclose} closeOnBackdrop={false}>
  {#if !job}
    <form id="upload-form" onsubmit={submit}>
      {#if replaceable}
        <fieldset>
          <legend>This clip is already online</legend>
          <label class="option" class:selected={replace}>
            <input type="radio" name="replace" value={true} bind:group={replace} />
            <span class="opt-icon"><Icon name="retry" size={18} /></span>
            <span class="opt-text">
              <strong>Replace the video</strong>
              <small>Same link: anyone who opens it sees the new version. Views and stars stay.</small>
            </span>
            <span class="ring" aria-hidden="true"></span>
          </label>
          <label class="option" class:selected={!replace}>
            <input type="radio" name="replace" value={false} bind:group={replace} />
            <span class="opt-icon"><Icon name="link" size={18} /></span>
            <span class="opt-text">
              <strong>Upload as a new clip</strong>
              <small>Gets its own link. The old clip stays online.</small>
            </span>
            <span class="ring" aria-hidden="true"></span>
          </label>
        </fieldset>
      {/if}
      <label class="field">
        <span>Title</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={title} maxlength="100" autofocus />
      </label>
      <fieldset>
        <legend>Who can see it</legend>
        {#each VISIBILITY as v (v.value)}
          <label class="option" class:selected={visibility === v.value}>
            <input type="radio" name="visibility" value={v.value} bind:group={visibility} />
            <span class="opt-icon"><Icon name={v.icon} size={18} /></span>
            <span class="opt-text">
              <strong>{v.label}</strong>
              <small>{v.hint}</small>
            </span>
            <span class="ring" aria-hidden="true"></span>
          </label>
        {/each}
      </fieldset>
      {#if isAdmin && clip.kind === "clip"}
        <fieldset>
          <legend>Quality <span class="optional">admins only</span></legend>
          <label class="option" class:selected={quality === "standard"}>
            <input type="radio" name="quality" value="standard" bind:group={quality} />
            <span class="opt-icon"><Icon name="film" size={18} /></span>
            <span class="opt-text">
              <strong>Standard</strong>
              <small>The server compresses it to save space. Fine for most clips.</small>
            </span>
            <span class="ring" aria-hidden="true"></span>
          </label>
          <label class="option" class:selected={quality === "high"}>
            <input type="radio" name="quality" value="high" bind:group={quality} />
            <span class="opt-icon"><Icon name="star" size={18} /></span>
            <span class="opt-text">
              <strong>High quality</strong>
              <small>Kept as uploaded, never compressed. For edits and compilations. Bigger file, slower upload.</small>
            </span>
            <span class="ring" aria-hidden="true"></span>
          </label>
        </fieldset>
      {/if}
      {#if friends && friends.length > 0}
        <fieldset>
          <legend>
            Who's in it
            <span class="optional">{replaceable && replace ? "leave empty to keep who's tagged now" : "optional"}</span>
          </legend>
          <div class="friends">
            {#each friends as name (name)}
              <button
                type="button"
                class="friend"
                class:on={tagged.includes(name)}
                aria-pressed={tagged.includes(name)}
                onclick={() => toggleTag(name)}
              >
                {#if tagged.includes(name)}<Icon name="check" size={13} />{:else}<Avatar name={names[name] ?? name} url={pictures[name]} size={18} />{/if}
                {names[name] ?? name}
              </button>
            {/each}
          </div>
        </fieldset>
      {/if}
      <AlbumPicker bind:selected={albums} />
    </form>
  {:else if job.status === "done"}
    <div class="done">
      <div class="done-icon"><Icon name="check" size={28} /></div>
      <p>
        {#if job.replaced}
          The video at your link is replaced. Discord messages that already show it keep the old
          preview until the link is posted again.
        {:else}
          {job.copied ? "The link is copied — paste it anywhere to share." : "Your clip is online."}
        {/if}
      </p>
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
      {#if transferLabel(job)}<p class="muted small transfer">{transferLabel(job)}</p>{/if}
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
    padding: 0.3rem 0.75rem 0.3rem 0.4rem;
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
    border-color: color-mix(in srgb, var(--accent) 70%, transparent);
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

  .transfer {
    font-variant-numeric: tabular-nums;
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

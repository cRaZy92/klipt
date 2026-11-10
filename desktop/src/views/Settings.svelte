<script lang="ts">
  import { onMount, tick } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    errorCode,
    errorFields,
    errorMessage,
    type Account,
    type AudioDevice,
    type Display,
    type HotkeyStatus,
    type Quality,
    type Resolution,
    type Settings,
  } from "../lib/api";
  import { bufferLabel, hotkeyLabel } from "../lib/format";
  import { acceleratorFromKeydown } from "../lib/hotkey";
  import {
    BITRATE_KBPS_MAX,
    BITRATE_KBPS_MIN,
    BUFFER_SECONDS_MAX,
    BUFFER_SECONDS_MIN,
    PRESET_KBPS,
    bitrateKbps,
    estimateRamMb,
    sameSettings,
    validateSettings,
  } from "../lib/settings";
  import { toast } from "../lib/toasts.svelte";
  import Icon from "../components/Icon.svelte";

  let {
    settings,
    account,
    hotkeyStatus,
    onsaved,
    onlogout,
    onclose,
    dirty = $bindable(false),
  }: {
    /** The saved settings. */
    settings: Settings;
    account: Account;
    /** The shortcut as the system reports it, or why it isn't working. */
    hotkeyStatus: HotkeyStatus;
    /** After a save: the new settings and, if the shortcut was rebound, how the system shows it. */
    onsaved: (settings: Settings, hotkeyDisplay: string | null) => void;
    /** Log out (the app asks about unsaved changes first). */
    onlogout: () => void;
    /** Back to the library (the app asks about unsaved changes). */
    onclose: () => void;
    /** Whether there are unsaved changes, for the app's "Discard changes?" guard. */
    dirty?: boolean;
  } = $props();

  const RESTARTED = "Recording restarted with new settings — the buffer was cleared.";
  const TAKEN = "That shortcut is already used by another app. Pick a different one.";

  const RESOLUTIONS: { value: Resolution; label: string }[] = [
    { value: "p720", label: "720p (1280 × 720)" },
    { value: "p1080", label: "1080p (1920 × 1080)" },
    { value: "p1440", label: "1440p (2560 × 1440)" },
    { value: "native", label: "Same as the display" },
  ];

  const QUALITIES: { value: Quality; label: string }[] = [
    { value: "low", label: `Low — ${PRESET_KBPS.low / 1000} Mbps` },
    { value: "medium", label: `Medium — ${PRESET_KBPS.medium / 1000} Mbps` },
    { value: "high", label: `High — ${PRESET_KBPS.high / 1000} Mbps` },
    { value: "custom", label: "Custom" },
  ];

  // svelte-ignore state_referenced_locally
  let draft = $state<Settings>({ ...settings });
  let saving = $state(false);
  /** Problems from the last failed save, with the draft they were found in. */
  // svelte-ignore state_referenced_locally
  let saveErrors = $state<{ errors: Record<string, string>; draft: Settings }>({ errors: {}, draft: settings });
  /** The problems still showing: one disappears as soon as its field is edited. */
  const fieldErrors = $derived(
    Object.fromEntries(
      Object.entries(saveErrors.errors).filter(
        ([field]) => draft[field as keyof Settings] === saveErrors.draft[field as keyof Settings],
      ),
    ) as Record<string, string>,
  );
  function setErrors(errors: Record<string, string>) {
    saveErrors = { errors, draft: $state.snapshot(draft) };
    // Show the first tab that needs fixing, once; the user can still move between tabs.
    const first = Object.keys(errors)[0];
    if (first && !Object.keys(errors).some((f) => tabOf(f) === tab)) tab = tabOf(first);
  }
  /** A shortcut problem from the last save (e.g. taken), shown in the Hotkey section. */
  let hotkeyProblem = $state<string | null>(null);

  let displays = $state<Display[] | null>(null);
  let displaysFailed = $state(false);
  let mics = $state<AudioDevice[] | null>(null);
  let micsFailed = $state(false);

  let capturing = $state(false);
  let captureHint = $state<string | null>(null);
  let captureBox = $state<HTMLDivElement>();

  $effect(() => {
    // The server address isn't edited here (logging in changes it).
    dirty = !sameSettings({ ...draft, serverUrl: settings.serverUrl }, settings);
  });

  const ramMb = $derived(estimateRamMb(bitrateKbps(draft), draft.bufferSeconds || 0));

  /** The shortcut as shown: what the system reports, unless the user picked a new one. */
  const hotkeyChanged = $derived(draft.hotkey !== settings.hotkey);
  const hotkeyShown = $derived(
    hotkeyChanged ? hotkeyLabel(draft.hotkey) : (hotkeyStatus.display ?? hotkeyLabel(settings.hotkey)),
  );
  const hotkeyStatusError = $derived(
    hotkeyChanged || hotkeyStatus.display
      ? null
      : hotkeyStatus.errorCode === "hotkey_taken"
        ? TAKEN
        : hotkeyStatus.error,
  );

  /** The saved display, even when it isn't connected now, so the choice isn't silently lost. */
  const missingDisplay = $derived(
    draft.displayId && displays && !displays.some((d) => d.id === draft.displayId) ? draft.displayId : null,
  );
  const missingMic = $derived(
    draft.micDeviceId && mics && !mics.some((m) => m.id === draft.micDeviceId) ? draft.micDeviceId : null,
  );

  onMount(() => {
    api
      .listDisplays()
      .then((d) => (displays = d))
      .catch(() => {
        displays = [];
        displaysFailed = true;
      });
    api
      .listAudioDevices()
      .then((m) => (mics = m))
      .catch(() => {
        mics = [];
        micsFailed = true;
      });
  });

  function displayLabel(d: Display): string {
    return `${d.name} — ${d.width} × ${d.height}${d.primary ? " (main)" : ""}`;
  }

  async function save() {
    const errors = validateSettings(draft);
    setErrors(Object.fromEntries(errors.map((e) => [e.field, e.message])));
    if (errors.length > 0) return;
    saving = true;
    hotkeyProblem = null;
    try {
      const result = await api.setSettings($state.snapshot(draft));
      onsaved(result.settings, result.hotkeyDisplay);
      draft = { ...result.settings };
      setErrors({});
      toast(result.recorderRestarted ? RESTARTED : "Settings saved", "success");
    } catch (e) {
      const code = errorCode(e);
      if (code === "invalid_settings") {
        setErrors(Object.fromEntries(errorFields(e).map((f) => [f.field, f.message])));
      } else if (code === "hotkey_taken" || code === "hotkey_unavailable") {
        // Nothing was saved and the old shortcut still works. Keep the other changes.
        hotkeyProblem = code === "hotkey_taken" ? TAKEN : errorMessage(e);
        toast(hotkeyProblem, "error");
        draft.hotkey = settings.hotkey;
      } else {
        toast(errorMessage(e), "error");
      }
    } finally {
      saving = false;
    }
  }

  function discard() {
    draft = { ...settings };
    setErrors({});
  }

  async function startCapture() {
    capturing = true;
    captureHint = null;
    hotkeyProblem = null;
    await tick();
    captureBox?.focus();
  }

  function stopCapture() {
    capturing = false;
    captureHint = null;
  }

  function oncapturekey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    const result = acceleratorFromKeydown(e);
    switch (result.kind) {
      case "accel":
        draft.hotkey = result.accel;
        stopCapture();
        break;
      case "cancel":
        stopCapture();
        break;
      case "needsModifier":
        captureHint = "Hold Ctrl, Alt, Shift or the Windows/Super key together with it.";
        break;
      case "unsupported":
        captureHint = "That key can't be used. Try a letter, a number, an F-key or the key under Esc.";
        break;
      case "modifiersOnly":
        break;
    }
  }

  async function chooseFolder() {
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: draft.outputDir || undefined,
        title: "Choose a folder for your clips",
      });
      if (typeof picked === "string" && picked) {
        draft.outputDir = picked;
      }
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  const TABS = [
    { id: "recording", label: "Recording" },
    { id: "audio", label: "Audio" },
    { id: "hotkey", label: "Hotkey" },
    { id: "clips", label: "Clips folder" },
    { id: "general", label: "General" },
    { id: "account", label: "Account" },
  ] as const;
  type Tab = (typeof TABS)[number]["id"];
  let tab = $state<Tab>("recording");

  const FIELD_TAB: Record<string, Tab> = {
    micEnabled: "audio",
    micDeviceId: "audio",
    hotkey: "hotkey",
    outputDir: "clips",
    startWithOs: "general",
    saveSound: "general",
    saveNotification: "general",
  };
  const tabOf = (field: string): Tab => FIELD_TAB[field] ?? "recording";
  const tabsWithErrors = $derived(new Set(Object.keys(fieldErrors).map(tabOf)));

  function onTabKey(e: KeyboardEvent, i: number) {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    tab = TABS[(i + step + TABS.length) % TABS.length].id;
    document.getElementById(`tab-${tab}`)?.focus();
  }

  async function openFolder() {
    try {
      await api.openClipsFolder();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }
</script>

<div class="settings">
  <div class="head"><div class="inner">
  <header class="bar">
    <button class="btn btn-ghost back" onclick={onclose}><Icon name="back" size={18} /> Clips</button>
    <h1>Settings</h1>
    {#if dirty}
      <span class="unsaved">Unsaved changes</span>
      <div class="actions">
        <button class="btn" onclick={discard} disabled={saving}>Discard</button>
        <button class="btn btn-primary" onclick={save} disabled={saving}>
          {#if saving}<span class="spinner"></span> Saving…{:else}Save changes{/if}
        </button>
      </div>
    {/if}
  </header>

  {#if Object.keys(fieldErrors).length > 0}
    <p class="form-error top-error" role="alert">Some settings need fixing before they can be saved.</p>
  {/if}

  <div class="tabs" role="tablist" aria-label="Settings sections">
    {#each TABS as t, i (t.id)}
      <button
        id="tab-{t.id}"
        role="tab"
        class="tab"
        class:active={tab === t.id}
        aria-selected={tab === t.id}
        tabindex={tab === t.id ? 0 : -1}
        onclick={() => (tab = t.id)}
        onkeydown={(e) => onTabKey(e, i)}
      >
        {t.label}
        {#if tabsWithErrors.has(t.id)}<span class="tab-dot" aria-label="needs fixing"></span>{/if}
      </button>
    {/each}
  </div>
  </div></div>

  <div class="scroll"><div class="inner">
  <div class="sections" role="tabpanel" aria-labelledby="tab-{tab}">
    {#if tab === "recording"}
    <section>
      <h2>Recording</h2>
      <p class="intro muted">
        Klipt records in the background and keeps the last moments in memory. Press the shortcut to save them as
        a clip.
      </p>

      <fieldset class="row">
        <legend>When to record</legend>
        <div class="choices">
          <label class="choice">
            <input type="radio" name="mode" value="always" bind:group={draft.recordingMode} />
            <span><strong>Always</strong><small>Record whenever Klipt is running.</small></span>
          </label>
          <label class="choice">
            <input type="radio" name="mode" value="whileGameRunning" bind:group={draft.recordingMode} />
            <span><strong>Only while a game is running</strong><small>Saves power when you're not gaming.</small></span>
          </label>
        </div>
      </fieldset>

      <label class="row">
        <span class="label">Display</span>
        <select class="input" bind:value={draft.displayId} disabled={displays === null}>
          <option value={null}>Automatic (main display)</option>
          {#each displays ?? [] as d (d.id)}
            <option value={d.id}>{displayLabel(d)}</option>
          {/each}
          {#if missingDisplay}
            <option value={missingDisplay}>{missingDisplay} (not connected)</option>
          {/if}
        </select>
        {#if displays === null}
          <small class="help">Looking for displays…</small>
        {:else if displaysFailed}
          <small class="help">Klipt couldn't list your displays, so it records the main one.</small>
        {:else}
          <small class="help">Which screen to record.</small>
        {/if}
      </label>

      <div class="grid">
        <label class="row">
          <span class="label">Resolution</span>
          <select class="input" bind:value={draft.resolution}>
            {#each RESOLUTIONS as r (r.value)}
              <option value={r.value}>{r.label}</option>
            {/each}
          </select>
          <small class="help">Clips are scaled to fit this size.</small>
        </label>

        <fieldset class="row">
          <legend>Frame rate</legend>
          <div class="segmented" role="radiogroup" aria-label="Frame rate">
            {#each [30, 60] as fps (fps)}
              <label class:active={draft.fps === fps}>
                <input type="radio" name="fps" value={fps} bind:group={draft.fps} />
                {fps} fps
              </label>
            {/each}
          </div>
          {#if fieldErrors.fps}<small class="error">{fieldErrors.fps}</small>{/if}
        </fieldset>
      </div>

      <div class="grid">
        <label class="row">
          <span class="label">Quality</span>
          <select class="input" bind:value={draft.quality}>
            {#each QUALITIES as q (q.value)}
              <option value={q.value}>{q.label}</option>
            {/each}
          </select>
          <small class="help">Higher quality looks sharper but makes bigger files.</small>
        </label>

        {#if draft.quality === "custom"}
          <label class="row">
            <span class="label">Bitrate (kbps)</span>
            <input
              class="input"
              type="number"
              min={BITRATE_KBPS_MIN}
              max={BITRATE_KBPS_MAX}
              step="500"
              bind:value={draft.customBitrateKbps}
              aria-invalid={!!fieldErrors.customBitrateKbps}
            />
            {#if fieldErrors.customBitrateKbps}
              <small class="error">{fieldErrors.customBitrateKbps}</small>
            {:else}
              <small class="help">Between 2,000 and 80,000. 20,000 is a good start for 1080p.</small>
            {/if}
          </label>
        {/if}
      </div>

      <div class="row">
        <label class="label" for="buffer-seconds">Clip length (buffer)</label>
        <div class="buffer">
          <input
            type="range"
            min={BUFFER_SECONDS_MIN}
            max={BUFFER_SECONDS_MAX}
            step="5"
            bind:value={draft.bufferSeconds}
            aria-label="Clip length in seconds"
          />
          <input
            id="buffer-seconds"
            class="input seconds"
            type="number"
            min={BUFFER_SECONDS_MIN}
            max={BUFFER_SECONDS_MAX}
            bind:value={draft.bufferSeconds}
            aria-invalid={!!fieldErrors.bufferSeconds}
          />
          <span class="muted">seconds</span>
        </div>
        {#if fieldErrors.bufferSeconds}
          <small class="error">{fieldErrors.bufferSeconds}</small>
        {:else}
          <small class="help">
            A saved clip covers the last {bufferLabel(draft.bufferSeconds || 0)}. Klipt keeps that much video in
            memory: <strong>≈ {ramMb.toLocaleString("en-US")} MB of RAM</strong>.
          </small>
        {/if}
      </div>
    </section>

    <section>
      <h2>Buffer location</h2>
      <fieldset class="row">
        <legend class="sr-only">Buffer location</legend>
        <div class="choices">
          <label class="choice">
            <input type="radio" name="location" value="ram" bind:group={draft.bufferLocation} />
            <span><strong>Memory (RAM)</strong><small>Fast, and doesn't wear out your drive.</small></span>
          </label>
          <label class="choice disabled" title="Not supported yet">
            <input type="radio" name="location" value="disk" disabled />
            <span><strong>Disk</strong><small>Not supported yet.</small></span>
          </label>
        </div>
        {#if fieldErrors.bufferLocation}<small class="error">{fieldErrors.bufferLocation}</small>{/if}
      </fieldset>
    </section>

    {:else if tab === "audio"}
    <section>
      <h2>Audio</h2>
      <p class="intro muted">Game and desktop sound are always recorded.</p>
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.micEnabled} />
        <span class="switch" aria-hidden="true"></span>
        <span><strong>Record my microphone</strong><small>On its own track, so you can turn it down later.</small></span>
      </label>
      <label class="row">
        <span class="label">Microphone</span>
        <select class="input" bind:value={draft.micDeviceId} disabled={!draft.micEnabled || mics === null}>
          <option value={null}>Default microphone</option>
          {#each mics ?? [] as m (m.id)}
            <option value={m.id}>{m.name}</option>
          {/each}
          {#if missingMic}
            <option value={missingMic}>Unavailable microphone</option>
          {/if}
        </select>
        {#if micsFailed}
          <small class="help">Klipt couldn't list your microphones, so it uses the default one.</small>
        {/if}
      </label>
    </section>

    {:else if tab === "hotkey"}
    <section>
      <h2>Hotkey</h2>
      <p class="intro muted">Press this shortcut anytime, even in a game, to save a clip.</p>
      <div class="hotkey">
        {#if capturing}
          <div
            class="capture"
            tabindex="0"
            role="textbox"
            aria-label="Press the new shortcut"
            bind:this={captureBox}
            onkeydown={oncapturekey}
            onblur={stopCapture}
          >
            Press the new shortcut…
          </div>
          <button class="btn" onmousedown={(e) => e.preventDefault()} onclick={stopCapture}>Cancel</button>
        {:else}
          <kbd class="keycap">{hotkeyShown}</kbd>
          <button class="btn" onclick={startCapture}>Change</button>
        {/if}
      </div>
      {#if capturing}
        <small class={captureHint ? "error" : "help"}>
          {captureHint ?? "Hold Ctrl, Alt, Shift or Super and press a key. Esc cancels."}
        </small>
      {:else if hotkeyProblem}
        <small class="error">{hotkeyProblem}</small>
      {:else if fieldErrors.hotkey}
        <small class="error">{fieldErrors.hotkey}</small>
      {:else if hotkeyChanged}
        <small class="help">The new shortcut takes effect when you save.</small>
      {:else if hotkeyStatusError}
        <small class="error">{hotkeyStatusError}</small>
      {/if}
    </section>

    {:else if tab === "clips"}
    <section>
      <h2>Clips folder</h2>
      <p class="intro muted">Where Klipt saves your clips.</p>
      <div class="folder">
        <code class="path" title={draft.outputDir}>{draft.outputDir}</code>
        <button class="btn" onclick={chooseFolder}>Change…</button>
        <button class="btn btn-ghost" onclick={openFolder} disabled={draft.outputDir !== settings.outputDir}>
          <Icon name="folder" size={16} /> Open folder
        </button>
      </div>
      {#if fieldErrors.outputDir}<small class="error">{fieldErrors.outputDir}</small>{/if}
    </section>

    {:else if tab === "general"}
    <section>
      <h2>General</h2>
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.startWithOs} />
        <span class="switch" aria-hidden="true"></span>
        <span><strong>Start Klipt when I log in</strong><small>It starts quietly in the tray.</small></span>
      </label>
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.saveSound} />
        <span class="switch" aria-hidden="true"></span>
        <span><strong>Play a sound when a clip is saved</strong></span>
      </label>
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.saveNotification} />
        <span class="switch" aria-hidden="true"></span>
        <span><strong>Show a notification when a clip is saved</strong></span>
      </label>
    </section>

    {:else if tab === "account"}
    <section>
      <h2>Account</h2>
      <dl class="account">
        <dt>Server</dt>
        <dd><code>{account.serverUrl}</code></dd>
        <dt>Username</dt>
        <dd>{account.username}</dd>
      </dl>
      <div>
        <button class="btn" onclick={onlogout}><Icon name="logout" size={16} /> Log out</button>
      </div>
    </section>
    {/if}
  </div>
</div></div>
</div>

<style>
  /* The header and tabs stay put; only the content below them scrolls. */
  .settings {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .head {
    flex: none;
    background: var(--bg);
  }

  .scroll {
    /* Contains the hidden screen-reader text, which would otherwise poke out below. */
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .inner {
    max-width: 46rem;
    margin: 0 auto;
    padding: 0 1.75rem;
  }

  .scroll .inner {
    padding-bottom: 2.5rem;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-height: 3.75rem;
    background: var(--bg);
  }

  .back {
    padding-left: 0.5rem;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    margin-bottom: 1.25rem;
    border-bottom: 1px solid var(--border, rgba(255, 255, 255, 0.08));
    background: var(--bg);
  }

  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.7rem 0.9rem;
    border: 0;
    background: none;
    color: var(--text-muted, #9aa0aa);
    font: inherit;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text, #fff);
  }

  .tab.active {
    color: var(--text, #fff);
    box-shadow: inset 0 -2px 0 var(--accent, #7c5cff);
  }

  .tab-dot {
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 50%;
    background: var(--danger, #ef4444);
  }

  h1 {
    font-size: 1.25rem;
    letter-spacing: -0.01em;
  }

  .unsaved {
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
    background: rgba(255, 181, 71, 0.14);
    color: var(--warning);
    font-size: 0.8rem;
    font-weight: 650;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    margin-left: auto;
  }

  .top-error {
    margin-bottom: 1rem;
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.25rem 1.4rem 1.4rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  h2 {
    font-size: 1.02rem;
  }

  .intro {
    margin-top: -0.6rem;
    font-size: 0.9rem;
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: none;
    min-width: 0;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .label,
  legend {
    padding: 0;
    margin-bottom: 0.4rem;
    font-size: 0.87rem;
    font-weight: 600;
    color: var(--text-muted);
  }

  .row > .label {
    margin-bottom: 0;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  select.input {
    padding-right: 0.5rem;
  }

  .input:disabled {
    opacity: 0.55;
  }

  .input[aria-invalid="true"] {
    border-color: var(--danger);
  }

  .help {
    color: var(--text-faint);
    font-size: 0.84rem;
  }

  .help strong {
    color: var(--text-muted);
  }

  .error {
    color: #ffb3ba;
    font-size: 0.84rem;
  }

  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    cursor: pointer;
  }

  .choice:has(input:checked) {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .choice.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .choice input {
    margin: 0.2rem 0 0;
    accent-color: var(--accent);
  }

  .choice span,
  .toggle > span:last-child {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .choice strong,
  .toggle strong {
    font-weight: 600;
  }

  .choice small,
  .toggle small {
    color: var(--text-faint);
    font-size: 0.84rem;
  }

  .segmented {
    display: inline-flex;
    padding: 0.2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
  }

  .segmented label {
    flex: 1;
    padding: 0.42rem 0.9rem;
    border-radius: 6px;
    text-align: center;
    font-weight: 600;
    color: var(--text-muted);
    cursor: pointer;
  }

  .segmented label.active {
    background: var(--surface-3);
    color: var(--text);
  }

  .segmented label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
  }

  .segmented input,
  .toggle input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .buffer {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .buffer input[type="range"] {
    flex: 1;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .seconds {
    width: 5.5rem;
  }

  .toggle {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.85rem;
    cursor: pointer;
  }

  .switch {
    position: relative;
    flex: none;
    width: 2.5rem;
    height: 1.4rem;
    border-radius: 999px;
    background: var(--surface-3);
    transition: background 0.15s;
  }

  .switch::after {
    content: "";
    position: absolute;
    top: 0.2rem;
    left: 0.2rem;
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    background: var(--text-muted);
    transition:
      transform 0.15s,
      background 0.15s;
  }

  .toggle input:checked + .switch {
    background: var(--accent);
  }

  .toggle input:checked + .switch::after {
    transform: translateX(1.1rem);
    background: #fff;
  }

  .toggle input:focus-visible + .switch {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .hotkey {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .keycap {
    min-width: 6rem;
    padding: 0.45rem 0.9rem;
    border: 1px solid var(--border);
    border-bottom-width: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-family: inherit;
    font-size: 1rem;
    font-weight: 700;
    text-align: center;
  }

  .capture {
    flex: 1;
    max-width: 20rem;
    padding: 0.55rem 0.9rem;
    border: 1px dashed var(--accent);
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--text);
    outline: none;
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      border-color: transparent;
    }
  }

  .folder {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    font-size: 0.88rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .account {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.5rem 1.5rem;
    margin: 0;
  }

  .account dt {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .account dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import { cached, refresh } from "../lib/cache";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    errorCode,
    errorFields,
    errorMessage,
    type Account,
    type FeedbackKind,
    type AudioDevice,
    type Display,
    type HotkeyStatus,
    type MyStorage,
    type Quality,
    type Resolution,
    type Settings,
    type SettingsSaved,
    type UpdateCheck,
  } from "../lib/api";
  import { bufferLabel, formatSize, hotkeyLabel } from "../lib/format";
  import { acceleratorFromKeydown } from "../lib/hotkey";
  import {
    BITRATE_KBPS_MAX,
    BITRATE_KBPS_MIN,
    BUFFER_SECONDS_MAX,
    BUFFER_SECONDS_MIN,
    CLEANUP_DAYS_MAX,
    CLEANUP_DAYS_MIN,
    EDITOR_CACHE_GB_MAX,
    EDITOR_CACHE_GB_MIN,
    PRESET_KBPS,
    bitrateKbps,
    parseAppList,
    estimateRamMb,
    sameSettings,
    validateSettings,
  } from "../lib/settings";
  import { toast } from "../lib/toasts.svelte";
  import Icon from "../components/Icon.svelte";
  import Keys from "../components/Keys.svelte";
  import ResourcesPanel from "../components/ResourcesPanel.svelte";
  import FeedbackDialog from "../components/FeedbackDialog.svelte";

  let {
    active = true,
    settings,
    account,
    hotkeyStatus,
    onsaved,
    onlogout,
    onaccount,
    dirty = $bindable(false),
  }: {
    /** This is the screen shown (it's kept alive, hidden, while another one is). */
    active?: boolean;
    /** The saved settings. */
    settings: Settings;
    account: Account;
    /** The shortcut as the system reports it, or why it isn't working. */
    hotkeyStatus: HotkeyStatus;
    /** After a save: the new settings and, if the shortcut was rebound, how the system shows it. */
    onsaved: (settings: Settings, result: SettingsSaved) => void;
    /** Log out (the app asks about unsaved changes first). */
    onlogout: () => void;
    /** The account changed (a new display name). */
    onaccount: (account: Account) => void;
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

  // What the system and server report, shown at once from the last answer (see cache.ts) and
  // refreshed quietly whenever this screen is shown. A "failed" note shows only with no answer at all.
  type AppInfo = Awaited<ReturnType<typeof api.appInfo>>;
  let info = $state(cached<AppInfo>("appInfo"));
  let displays = $state<Display[] | null>(cached<Display[]>("displays") ?? null);
  let displaysFailed = $state(false);
  let mics = $state<AudioDevice[] | null>(cached<AudioDevice[]>("mics") ?? null);
  let micsFailed = $state(false);
  let folderSize = $state(cached<number>("folderSize"));
  let folderSizeFailed = $state(false);
  let storage = $state(cached<MyStorage>("storage"));
  let storageFailed = $state(false);

  /** Which shortcut is being recorded. */
  let capturing = $state<"hotkey" | "screenshotHotkey" | "recordHotkey" | null>(null);
  let captureHint = $state<string | null>(null);
  let captureBox = $state<HTMLDivElement>();

  $effect(() => {
    // The server address isn't edited here (logging in changes it).
    dirty = !sameSettings({ ...draft, serverUrl: settings.serverUrl }, settings);
  });

  const ramMb = $derived(estimateRamMb(bitrateKbps(draft), draft.bufferSeconds || 0));
  /** How far along its track the buffer slider is, 0–100. */
  const bufferFill = $derived(
    Math.min(100, Math.max(0, (((draft.bufferSeconds || 0) - BUFFER_SECONDS_MIN) / (BUFFER_SECONDS_MAX - BUFFER_SECONDS_MIN)) * 100)),
  );

  /** The shortcut as shown: what the system reports, unless the user picked a new one. */
  const hotkeyChanged = $derived(draft.hotkey !== settings.hotkey);
  const hotkeyShown = $derived(
    hotkeyChanged ? hotkeyLabel(draft.hotkey) : (hotkeyStatus.display ?? hotkeyLabel(settings.hotkey)),
  );
  const screenshotShown = $derived(
    draft.screenshotHotkey !== settings.screenshotHotkey
      ? hotkeyLabel(draft.screenshotHotkey)
      : (hotkeyStatus.screenshotDisplay ?? hotkeyLabel(draft.screenshotHotkey)),
  );
  const recordShown = $derived(
    draft.recordHotkey !== settings.recordHotkey
      ? hotkeyLabel(draft.recordHotkey)
      : (hotkeyStatus.recordDisplay ?? hotkeyLabel(draft.recordHotkey)),
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

  // Settings → Account: the name friends see, and the password.
  // svelte-ignore state_referenced_locally
  let nameDraft = $state(account.customName ?? "");
  let savingName = $state(false);
  let nameError = $state<string | null>(null);
  const nameChanged = $derived(nameDraft.trim() !== (account.customName ?? ""));

  async function saveName(e: SubmitEvent) {
    e.preventDefault();
    savingName = true;
    nameError = null;
    try {
      const next = await api.updateProfile(nameDraft);
      nameDraft = next.customName ?? "";
      onaccount(next);
      toast(next.customName ? `Friends now see you as ${next.displayName}` : "Friends see your username again", "success");
    } catch (err) {
      nameError = errorMessage(err);
    } finally {
      savingName = false;
    }
  }

  let pw = $state({ current: "", next: "", again: "" });
  let changingPassword = $state(false);
  let passwordError = $state<string | null>(null);

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    passwordError = null;
    if (!pw.current || !pw.next) {
      passwordError = "Fill in your current and new password.";
      return;
    }
    if (pw.next.length < 8) {
      passwordError = "The new password needs at least 8 characters.";
      return;
    }
    if (pw.next !== pw.again) {
      passwordError = "The two new passwords aren't the same.";
      return;
    }
    changingPassword = true;
    try {
      await api.changePassword(pw.current, pw.next);
      pw = { current: "", next: "", again: "" };
      toast("Password changed. Your other devices are signed out.", "success");
    } catch (err) {
      passwordError = errorMessage(err);
    } finally {
      changingPassword = false;
    }
  }

  let feedbackKind = $state<FeedbackKind | null>(null);

  function openLogs() {
    api.openLogsFolder().catch((e) => toast(errorMessage(e), "error"));
  }

  function refreshAll() {
    if (!info) refresh("appInfo", api.appInfo).then((i) => (info = i)).catch(() => {});
    refresh("displays", api.listDisplays)
      .then((d) => ((displays = d), (displaysFailed = false)))
      .catch(() => {
        if (displays === null) (displays = []), (displaysFailed = true);
      });
    refresh("mics", api.listAudioDevices)
      .then((m) => ((mics = m), (micsFailed = false)))
      .catch(() => {
        if (mics === null) (mics = []), (micsFailed = true);
      });
    refresh("storage", api.myStorage)
      .then((st) => ((storage = st), (storageFailed = false)))
      .catch(() => (storageFailed = storage === undefined));
    refreshFolderSize();
  }

  function refreshFolderSize() {
    refresh("folderSize", api.clipsFolderSize)
      .then((b) => ((folderSize = b), (folderSizeFailed = false)))
      .catch(() => (folderSizeFailed = folderSize === undefined));
  }

  $effect(() => {
    if (active) untrack(refreshAll);
  });

  // A save may have moved the clips folder: measure the new one.
  // svelte-ignore state_referenced_locally
  let measuredDir = settings.outputDir;
  $effect(() => {
    if (settings.outputDir !== measuredDir) {
      measuredDir = settings.outputDir;
      untrack(refreshFolderSize);
    }
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
      onsaved(result.settings, result);
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

  async function startCapture(target: "hotkey" | "screenshotHotkey" | "recordHotkey" = "hotkey") {
    capturing = target;
    captureHint = null;
    hotkeyProblem = null;
    await tick();
    captureBox?.focus();
  }

  function stopCapture() {
    capturing = null;
    captureHint = null;
  }

  function oncapturekey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    const result = acceleratorFromKeydown(e);
    switch (result.kind) {
      case "accel":
        if (capturing) draft[capturing] = result.accel;
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
    { id: "recording", label: "Recording", icon: "record" },
    { id: "audio", label: "Audio", icon: "mic" },
    { id: "hotkey", label: "Hotkey", icon: "keyboard" },
    { id: "clips", label: "Clips folder", icon: "folder" },
    { id: "general", label: "General", icon: "sliders" },
    { id: "resources", label: "Resources", icon: "pulse" },
    { id: "account", label: "Account", icon: "user" },
  ] as const;
  type Tab = (typeof TABS)[number]["id"];
  let tab = $state<Tab>("recording");

  // Settings → About: "Check for updates".
  let updateCheck = $state<{ checking: boolean; result: UpdateCheck | null; error: string | null; installing: boolean }>({
    checking: false,
    result: null,
    error: null,
    installing: false,
  });

  async function checkForUpdate() {
    Object.assign(updateCheck, { checking: true, result: null, error: null });
    try {
      updateCheck.result = await api.checkForUpdate();
    } catch (e) {
      updateCheck.error = errorMessage(e);
    } finally {
      updateCheck.checking = false;
    }
  }

  async function installFoundUpdate() {
    updateCheck.installing = true;
    try {
      await api.installUpdate(); // restarts Klipt when it succeeds
    } catch (e) {
      updateCheck.installing = false;
      toast(errorMessage(e), "error");
    }
  }

  async function copyFoundCommand(command: string) {
    try {
      await api.copyText(command);
      toast("Command copied. Paste it into a terminal.", "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  const FIELD_TAB: Record<string, Tab> = {
    micEnabled: "audio",
    micDeviceId: "audio",
    voiceApps: "audio",
    hotkey: "hotkey",
    screenshotHotkey: "hotkey",
    recordHotkey: "hotkey",
    outputDir: "clips",
    cleanupDays: "clips",
    editorCacheGb: "clips",
    startWithOs: "general",
    saveSound: "general",
    saveNotification: "general",
    friendNotifications: "general",
  };
  const tabOf = (field: string): Tab => FIELD_TAB[field] ?? "recording";
  const tabsWithErrors = $derived(new Set(Object.keys(fieldErrors).map(tabOf)));

  function onTabKey(e: KeyboardEvent, i: number) {
    const step = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
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
  <header class="bar">
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

  <!-- The header and section list stay put; only the section's content scrolls. -->
  <div class="body">
  <div class="tabs" role="tablist" aria-label="Settings sections" aria-orientation="vertical">
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
        <Icon name={t.icon} size={18} />
        {t.label}
        {#if tabsWithErrors.has(t.id)}<span class="tab-dot" aria-label="needs fixing"></span>{/if}
      </button>
    {/each}
  </div>

  <div class="scroll"><div class="inner">
  {#if Object.keys(fieldErrors).length > 0}
    <p class="form-error top-error" role="alert">Some settings need fixing before they can be saved.</p>
  {/if}
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
            class="range"
            type="range"
            min={BUFFER_SECONDS_MIN}
            max={BUFFER_SECONDS_MAX}
            step="5"
            bind:value={draft.bufferSeconds}
            style:--fill="{bufferFill}%"
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
      {#if info}
        {#if info.platform === "linux"}
          <label class="row">
            <span class="label">Voice chat apps</span>
            <input
              class="input"
              value={draft.voiceApps.join(", ")}
              onchange={(e) => (draft.voiceApps = parseAppList(e.currentTarget.value))}
              placeholder="discord, vesktop"
              aria-invalid={!!fieldErrors.voiceApps}
            />
            {#if fieldErrors.voiceApps}
              <small class="error">{fieldErrors.voiceApps}</small>
            {:else}
              <small class="help">
                Recorded on their own track, so you can turn your friends' voices down before sharing. Leave it
                empty to record voice chat with the game sound.
              </small>
            {/if}
          </label>
        {/if}
      {/if}
    </section>

    {:else if tab === "hotkey"}
    <section>
      <h2>Hotkey</h2>
      <p class="intro muted">Press this shortcut anytime, even in a game, to save a clip.</p>
      <div class="hotkey">
        {#if capturing === "hotkey"}
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
          <span class="keycaps"><Keys shortcut={hotkeyShown} size="lg" /></span>
          <button class="btn" onclick={() => startCapture("hotkey")} disabled={!!capturing}>Change</button>
        {/if}
      </div>
      {#if capturing === "hotkey"}
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

    <section>
      <h2>Screenshot</h2>
      <p class="intro muted">Saves a picture of the screen you record, next to your clips.</p>
      <div class="hotkey">
        {#if capturing === "screenshotHotkey"}
          <div
            class="capture"
            tabindex="0"
            role="textbox"
            aria-label="Press the new screenshot shortcut"
            bind:this={captureBox}
            onkeydown={oncapturekey}
            onblur={stopCapture}
          >
            Press the new shortcut…
          </div>
          <button class="btn" onmousedown={(e) => e.preventDefault()} onclick={stopCapture}>Cancel</button>
        {:else}
          {#if draft.screenshotHotkey}
            <span class="keycaps"><Keys shortcut={screenshotShown} size="lg" /></span>
          {:else}
            <span class="muted">No shortcut</span>
          {/if}
          <button class="btn" onclick={() => startCapture("screenshotHotkey")} disabled={!!capturing}>
            {draft.screenshotHotkey ? "Change" : "Set shortcut"}
          </button>
          {#if draft.screenshotHotkey}
            <button class="btn btn-ghost" onclick={() => (draft.screenshotHotkey = "")}>Turn off</button>
          {/if}
        {/if}
      </div>
      {#if capturing === "screenshotHotkey"}
        <small class={captureHint ? "error" : "help"}>
          {captureHint ?? "Hold Ctrl, Alt, Shift or Super and press a key. Esc cancels."}
        </small>
      {:else if fieldErrors.screenshotHotkey}
        <small class="error">{fieldErrors.screenshotHotkey}</small>
      {:else if draft.screenshotHotkey !== settings.screenshotHotkey}
        <small class="help">The new shortcut takes effect when you save.</small>
      {:else if hotkeyStatus.screenshotError}
        <small class="error">{hotkeyStatus.screenshotError}</small>
      {/if}
    </section>

    <section>
      <h2>Recording</h2>
      <p class="intro muted">Starts a recording of any length, and stops it on the next press. The replay buffer keeps running.</p>
      <div class="hotkey">
        {#if capturing === "recordHotkey"}
          <div
            class="capture"
            tabindex="0"
            role="textbox"
            aria-label="Press the new recording shortcut"
            bind:this={captureBox}
            onkeydown={oncapturekey}
            onblur={stopCapture}
          >
            Press the new shortcut…
          </div>
          <button class="btn" onmousedown={(e) => e.preventDefault()} onclick={stopCapture}>Cancel</button>
        {:else}
          {#if draft.recordHotkey}
            <span class="keycaps"><Keys shortcut={recordShown} size="lg" /></span>
          {:else}
            <span class="muted">No shortcut</span>
          {/if}
          <button class="btn" onclick={() => startCapture("recordHotkey")} disabled={!!capturing}>
            {draft.recordHotkey ? "Change" : "Set shortcut"}
          </button>
          {#if draft.recordHotkey}
            <button class="btn btn-ghost" onclick={() => (draft.recordHotkey = "")}>Turn off</button>
          {/if}
        {/if}
      </div>
      {#if capturing === "recordHotkey"}
        <small class={captureHint ? "error" : "help"}>
          {captureHint ?? "Hold Ctrl, Alt, Shift or Super and press a key. Esc cancels."}
        </small>
      {:else if fieldErrors.recordHotkey}
        <small class="error">{fieldErrors.recordHotkey}</small>
      {:else if draft.recordHotkey !== settings.recordHotkey}
        <small class="help">The new shortcut takes effect when you save.</small>
      {:else if hotkeyStatus.recordError}
        <small class="error">{hotkeyStatus.recordError}</small>
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
      {#if draft.outputDir === settings.outputDir}
        <p class="muted folder-size">
          {#if folderSize !== undefined}
            The folder takes <strong>{formatSize(folderSize)}</strong>.
          {:else if folderSizeFailed}
            Can't measure the folder.
          {:else}
            Measuring the folder…
          {/if}
        </p>
      {/if}
    </section>

    <section>
      <h2>Cleanup</h2>
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.cleanupEnabled} />
        <span class="switch" aria-hidden="true"></span>
        <span>
          <strong>Delete old clips from this PC</strong>
          <small>Uploaded and favorite clips are always kept. Deleted clips can't be recovered.</small>
        </span>
      </label>
      <div class="row">
        <label class="label" for="cleanup-days">Delete clips older than</label>
        <div class="days">
          <input
            id="cleanup-days"
            class="input seconds"
            type="number"
            min={CLEANUP_DAYS_MIN}
            max={CLEANUP_DAYS_MAX}
            bind:value={draft.cleanupDays}
            aria-invalid={!!fieldErrors.cleanupDays}
          />
          <span class="muted">days</span>
        </div>
        {#if fieldErrors.cleanupDays}<small class="error">{fieldErrors.cleanupDays}</small>{/if}
      </div>
    </section>

    <section>
      <h2>Editor</h2>
      <div class="row">
        <label class="label" for="editor-cache">Editor preview cache</label>
        <div class="days">
          <input
            id="editor-cache"
            class="input seconds"
            type="number"
            step="0.5"
            min={EDITOR_CACHE_GB_MIN}
            max={EDITOR_CACHE_GB_MAX}
            bind:value={draft.editorCacheGb}
            aria-invalid={!!fieldErrors.editorCacheGb}
          />
          <span class="muted">GB</span>
        </div>
        <small class="muted">Pictures for scrubbing in the editor. The ones you opened longest ago are deleted first.</small>
        {#if fieldErrors.editorCacheGb}<small class="error">{fieldErrors.editorCacheGb}</small>{/if}
      </div>
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
      <label class="toggle">
        <input type="checkbox" role="switch" bind:checked={draft.friendNotifications} />
        <span class="switch" aria-hidden="true"></span>
        <span><strong>Notify me when a friend shares a clip</strong><small>Klipt checks the feed every few minutes.</small></span>
      </label>
    </section>

    {:else if tab === "resources"}
    <ResourcesPanel />

    {:else if tab === "account"}
    <section>
      <h2>Account</h2>
      <dl class="account">
        <dt>Server</dt>
        <dd><code>{account.serverUrl}</code></dd>
        <dt>Username</dt>
        <dd>{account.username}</dd>
        <dt>Your clips on the server</dt>
        <dd>
          {#if storage}
            {storage.clips === 1 ? "1 clip" : `${storage.clips} clips`} · {formatSize(storage.bytes)}
          {:else if storageFailed}
            <span class="muted">Can't reach the server</span>
          {:else}
            …
          {/if}
        </dd>
      </dl>
      <div>
        <button class="btn" onclick={onlogout}><Icon name="logout" size={16} /> Log out</button>
      </div>
    </section>

    <section>
      <h2>Name</h2>
      <p class="intro muted">Friends see this name on your clips. You still log in as {account.username}.</p>
      <form class="inline-form" onsubmit={saveName}>
        <input
          class="input"
          bind:value={nameDraft}
          placeholder={account.username}
          maxlength="32"
          aria-label="Display name"
          aria-invalid={!!nameError}
        />
        <button class="btn btn-primary" type="submit" disabled={savingName || !nameChanged}>
          {#if savingName}<span class="spinner"></span> Saving…{:else}Save name{/if}
        </button>
      </form>
      {#if nameError}<small class="error">{nameError}</small>{/if}
    </section>

    <section>
      <h2>Password</h2>
      <p class="intro muted">Changing it signs you out on your other devices. This one stays logged in.</p>
      <form class="password-form" onsubmit={changePassword}>
        <input type="text" class="sr-only" autocomplete="username" value={account.username} tabindex="-1" aria-hidden="true" readonly />
        <label class="row">
          <span class="label">Current password</span>
          <input class="input" type="password" autocomplete="current-password" bind:value={pw.current} />
        </label>
        <div class="grid">
          <label class="row">
            <span class="label">New password</span>
            <input class="input" type="password" autocomplete="new-password" bind:value={pw.next} />
          </label>
          <label class="row">
            <span class="label">New password again</span>
            <input class="input" type="password" autocomplete="new-password" bind:value={pw.again} />
          </label>
        </div>
        {#if passwordError}<small class="error">{passwordError}</small>{/if}
        <div>
          <button class="btn" type="submit" disabled={changingPassword}>
            {#if changingPassword}<span class="spinner"></span> Changing…{:else}Change password{/if}
          </button>
        </div>
      </form>
      <small class="help">Forgot it? Your admin can send you a link to set a new one.</small>
    </section>

    <section>
      <h2>Help</h2>
      <p class="intro muted">Something not working, or an idea? It goes straight to your admin.</p>
      <div class="help-actions">
        <button class="btn" onclick={() => (feedbackKind = "bug")}><Icon name="alert" size={16} /> Report a problem</button>
        <button class="btn" onclick={() => (feedbackKind = "idea")}><Icon name="bulb" size={16} /> Suggest a feature</button>
        <button class="btn btn-ghost" onclick={openLogs}><Icon name="folder" size={16} /> Open logs folder</button>
      </div>
    </section>

    <section>
      <h2>About</h2>
      {#if info}
        <p class="about">
          {#if info.dev}
            Klipt · development build
          {:else}
            Klipt <strong>{info.version}</strong>
            {#if info.commit}
              <span class="muted">· build <code>{info.commit}</code></span>
            {/if}
          {/if}
        </p>
      {/if}
      <div class="update-check">
        <button class="btn" onclick={checkForUpdate} disabled={updateCheck.checking || updateCheck.installing}>
          {#if updateCheck.checking}<span class="spinner"></span> Checking…{:else}<Icon name="retry" size={16} /> Check for updates{/if}
        </button>
        {#if updateCheck.result?.status === "upToDate"}
          <span class="update-result ok"><Icon name="check" size={16} /> You have the latest version.</span>
        {:else if updateCheck.result?.status === "unsupported"}
          <span class="update-result muted">{updateCheck.result.reason}</span>
        {:else if updateCheck.error}
          <span class="update-result error"><Icon name="alert" size={16} /> {updateCheck.error}</span>
        {/if}
      </div>
      {#if updateCheck.result?.status === "available"}
        {@const found = updateCheck.result}
        <div class="update-found">
          <span><Icon name="download" size={16} /> <strong>Klipt {found.version}</strong> is available.</span>
          {#if found.command}
            <span class="muted">To install it, run this in a terminal:</span>
            <code title={found.command}>{found.command}</code>
            <div><button class="btn btn-sm btn-primary" onclick={() => copyFoundCommand(found.command!)}>Copy command</button></div>
          {:else}
            <div>
              <button class="btn btn-sm btn-primary" onclick={installFoundUpdate} disabled={updateCheck.installing}>
                {#if updateCheck.installing}<span class="spinner"></span> Installing…{:else}Restart to install{/if}
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </section>
    {/if}
  </div>
</div></div>
</div>
</div>

{#if feedbackKind}
  <FeedbackDialog kind={feedbackKind} onclose={() => (feedbackKind = null)} />
{/if}

<style>
  .settings {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .bar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 0.85rem;
    min-height: 2.6rem;
    margin: 1.75rem 2rem 1.25rem;
  }

  h1 {
    font-size: 1.85rem;
    font-weight: 750;
    line-height: 1.1;
  }

  .unsaved {
    padding: 0.2rem 0.65rem;
    border-radius: 999px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 0.8rem;
    font-weight: 650;
    animation: rise 0.25s var(--ease-out);
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    margin-left: auto;
    animation: rise 0.25s var(--ease-out);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .tabs {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 0.2rem;
    width: 13.5rem;
    padding: 0 0.75rem 1.5rem 2rem;
    overflow-y: auto;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.6rem 0.75rem;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .tab:hover {
    background: var(--surface);
    color: var(--text);
  }

  .tab.active {
    background: var(--accent-soft);
    color: var(--text);
  }

  .tab.active :global(svg) {
    color: var(--accent-text);
  }

  .tab-dot {
    width: 0.5rem;
    height: 0.5rem;
    margin-left: auto;
    border-radius: 50%;
    background: var(--danger);
    box-shadow: 0 0 8px var(--danger);
  }

  .scroll {
    /* Contains the hidden screen-reader text, which would otherwise poke out below. */
    position: relative;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }

  .inner {
    max-width: 46rem;
    padding: 0 2rem 3rem 1rem;
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
    gap: 1.1rem;
    padding: 1.4rem 1.5rem 1.5rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: linear-gradient(180deg, var(--surface), rgba(21, 20, 29, 0.55));
    animation: rise 0.3s var(--ease-out) backwards;
  }

  .about {
    color: var(--text-muted);
  }

  .about strong {
    color: var(--text);
  }

  .update-check {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.9rem;
    margin-top: 0.75rem;
  }

  .update-result {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.9rem;
  }

  .update-result.ok {
    color: var(--success);
  }

  .update-result.error {
    color: var(--danger, #ff5c70);
  }

  .update-result.muted,
  .update-found .muted {
    color: var(--text-muted);
  }

  .update-found {
    display: grid;
    gap: 0.5rem;
    margin-top: 0.8rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--accent-soft);
  }

  .update-found > span:first-child {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .update-found code {
    overflow-x: auto;
    padding: 0.4rem 0.55rem;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    font-size: 0.8rem;
    white-space: nowrap;
  }

  h2 {
    font-size: 1.2rem;
    font-weight: 700;
  }

  .intro {
    margin-top: -0.65rem;
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
    color: var(--danger-text);
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
    gap: 0.7rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg);
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s,
      box-shadow 0.15s;
  }

  .choice:hover:not(.disabled) {
    border-color: var(--border-strong);
  }

  .choice:has(input:checked) {
    border-color: rgba(124, 92, 255, 0.7);
    background: var(--accent-softer);
    box-shadow: 0 0 0 3px var(--accent-softer);
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
    padding: 0.22rem;
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
    transition:
      background 0.15s,
      color 0.15s;
  }

  .segmented label:hover {
    color: var(--text);
  }

  .segmented label.active {
    background: var(--accent);
    box-shadow: 0 4px 12px -4px var(--accent-glow);
    color: #fff;
  }

  .segmented label:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
    gap: 0.9rem;
  }

  .buffer .range {
    flex: 1;
  }

  .seconds {
    width: 5.5rem;
  }

  .toggle {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.2rem 0;
    cursor: pointer;
  }

  .switch {
    position: relative;
    flex: none;
    width: 2.6rem;
    height: 1.5rem;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface-3);
    transition:
      background 0.2s,
      border-color 0.2s,
      box-shadow 0.2s;
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
      transform 0.2s var(--ease-out),
      background 0.2s;
  }

  .toggle input:checked + .switch {
    border-color: transparent;
    background: var(--accent);
    box-shadow: 0 0 14px -2px var(--accent-glow);
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
    gap: 1.25rem;
    min-height: 4rem;
  }

  .keycaps {
    font-size: 0.85rem;
  }

  .capture {
    flex: 1;
    max-width: 20rem;
    padding: 0.7rem 1rem;
    border: 1px dashed var(--accent);
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
    outline: none;
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      border-color: transparent;
    }
  }

  .folder-size {
    margin: 0.75rem 0 0;
    font-size: 0.9rem;
  }

  .days {
    display: flex;
    align-items: center;
    gap: 0.6rem;
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
    padding: 0.6rem 0.8rem;
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
    gap: 0.6rem 1.5rem;
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

  .inline-form {
    display: flex;
    gap: 0.6rem;
    max-width: 30rem;
  }

  .inline-form .input {
    flex: 1;
  }

  .password-form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    max-width: 30rem;
  }

  .help-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>

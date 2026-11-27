<script lang="ts">
  import { tick } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errorCode, errorFields, errorMessage, type HotkeyStatus, type Settings, type SettingsSaved } from "../lib/api";
  import { bufferLabel, hotkeyLabel } from "../lib/format";
  import { acceleratorFromKeydown } from "../lib/hotkey";
  import { bitrateKbps, estimateRamMb } from "../lib/settings";
  import Icon from "../components/Icon.svelte";
  import Keys from "../components/Keys.svelte";

  // The first-start setup: the save-clip shortcut, the clip length and the clips folder. Each step
  // starts on the current (default) value; Skip keeps it, Skip setup keeps all of them.
  let {
    settings,
    hotkeyStatus,
    ondone,
  }: {
    settings: Settings;
    hotkeyStatus: HotkeyStatus;
    ondone: (saved: Settings, result: SettingsSaved) => void;
  } = $props();

  const LENGTHS = [15, 30, 60, 120, 300];
  const TAKEN =
    "That shortcut is already used by another app (often an NVIDIA, AMD, Discord or Steam overlay). Pick a different one.";

  // svelte-ignore state_referenced_locally
  const initial = { hotkey: settings.hotkey, bufferSeconds: settings.bufferSeconds, outputDir: settings.outputDir };
  let hotkey = $state(initial.hotkey);
  let bufferSeconds = $state(initial.bufferSeconds);
  let outputDir = $state(initial.outputDir);

  let step = $state(0);
  let saving = $state(false);
  let capturing = $state(false);
  let captureHint = $state<string | null>(null);
  let captureBox = $state<HTMLElement>();
  /** A problem from saving, shown on the step it belongs to. */
  let problem = $state<{ step: number; message: string } | null>(null);

  const steps = ["Shortcut", "Clip length", "Folder"];
  const last = steps.length - 1;
  // Unchanged, show what the system actually bound (KDE may have let the user pick another key).
  const hotkeyShown = $derived(hotkey === initial.hotkey ? (hotkeyStatus.display ?? hotkeyLabel(hotkey)) : hotkeyLabel(hotkey));
  const ramMb = $derived(estimateRamMb(bitrateKbps(settings), bufferSeconds));
  const lengths = $derived(LENGTHS.includes(bufferSeconds) ? LENGTHS : [...LENGTHS, bufferSeconds].sort((a, b) => a - b));

  function shortLength(seconds: number): string {
    return seconds >= 60 && seconds % 60 === 0 ? `${seconds / 60} min` : `${seconds} s`;
  }

  async function startCapture() {
    capturing = true;
    captureHint = null;
    problem = null;
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
        hotkey = result.accel;
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
    problem = null;
    try {
      const picked = await openDialog({ directory: true, multiple: false, defaultPath: outputDir || undefined, title: "Choose a folder for your clips" });
      if (typeof picked === "string" && picked) outputDir = picked;
    } catch (e) {
      problem = { step: 2, message: errorMessage(e) };
    }
  }

  /** Skip this step: keep its default. */
  function skip() {
    if (step === 0) (hotkey = initial.hotkey), stopCapture();
    if (step === 1) bufferSeconds = initial.bufferSeconds;
    if (step === 2) outputDir = initial.outputDir;
    next();
  }

  function next() {
    problem = null;
    if (step < last) step += 1;
    else void finish();
  }

  async function finish(all = { hotkey, bufferSeconds, outputDir }) {
    saving = true;
    problem = null;
    try {
      const result = await api.setSettings({ ...$state.snapshot(settings), ...all, onboarded: true });
      ondone(result.settings, result);
    } catch (e) {
      const code = errorCode(e);
      if (code === "hotkey_taken" || code === "hotkey_unavailable") {
        step = 0;
        problem = { step: 0, message: code === "hotkey_taken" ? TAKEN : errorMessage(e) };
      } else if (code === "invalid_settings") {
        const field = errorFields(e)[0];
        const at = field?.field === "hotkey" ? 0 : field?.field === "bufferSeconds" ? 1 : 2;
        step = at;
        problem = { step: at, message: field?.message ?? errorMessage(e) };
      } else {
        problem = { step, message: errorMessage(e) };
      }
    } finally {
      saving = false;
    }
  }

  /** Skip setup: every default, saved so the setup doesn't come back. */
  function skipAll() {
    stopCapture();
    void finish({ ...initial });
  }
</script>

<div class="screen">
  <div class="card">
    <header>
      <div class="brand"><span class="logo"><Icon name="logo" size={22} /></span> Set up Klipt</div>
      <button class="btn btn-ghost btn-sm" onclick={skipAll} disabled={saving}>Skip setup</button>
    </header>

    <ol class="steps" aria-label="Setup steps">
      {#each steps as name, i (name)}
        <li class:done={i < step} class:current={i === step} aria-current={i === step ? "step" : undefined}>
          <span class="num">{i + 1}</span>{name}
        </li>
      {/each}
    </ol>

    <section class="body">
      {#if step === 0}
        <h1>Your clip shortcut</h1>
        <p class="muted">Press it anytime, even in a game, to save what just happened.</p>
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
            <span class="keycaps"><Keys shortcut={hotkeyShown} size="lg" /></span>
            <button class="btn" onclick={startCapture}>Change</button>
          {/if}
        </div>
        {#if capturing}
          <small class={captureHint ? "error" : "help"}>{captureHint ?? "Hold Ctrl, Alt, Shift or Super and press a key. Esc cancels."}</small>
        {:else if problem?.step === 0}
          <small class="error">{problem.message}</small>
        {:else if hotkey === initial.hotkey}
          <small class="help">This is the default. You can change it later in Settings.</small>
        {/if}
      {:else if step === 1}
        <h1>How long is a clip?</h1>
        <p class="muted">When you press the shortcut, Klipt saves this much of what just happened.</p>
        <div class="choices" role="radiogroup" aria-label="Clip length">
          {#each lengths as s (s)}
            <button class="chip" class:on={bufferSeconds === s} role="radio" aria-checked={bufferSeconds === s} onclick={() => (bufferSeconds = s)}>
              {shortLength(s)}
            </button>
          {/each}
        </div>
        {#if problem?.step === 1}
          <small class="error">{problem.message}</small>
        {:else}
          <small class="help">
            The last {bufferLabel(bufferSeconds)}, kept in memory: <strong>≈ {ramMb.toLocaleString("en-US")} MB of RAM</strong>.
            You can trim clips later.
          </small>
        {/if}
      {:else}
        <h1>Where should clips go?</h1>
        <p class="muted">Klipt saves your clips and screenshots in this folder.</p>
        <div class="folder">
          <code class="path" title={outputDir}>{outputDir}</code>
          <button class="btn" onclick={chooseFolder}>Change…</button>
        </div>
        {#if problem?.step === 2}
          <small class="error">{problem.message}</small>
        {:else if outputDir === initial.outputDir}
          <small class="help">This is the default. You can change it later in Settings.</small>
        {/if}
      {/if}
    </section>

    <footer>
      <button class="btn btn-ghost" onclick={() => ((problem = null), (step -= 1), stopCapture())} disabled={step === 0 || saving}>
        Back
      </button>
      <span class="spacer"></span>
      <button class="btn" onclick={skip} disabled={saving}>Skip</button>
      <button class="btn btn-primary" onclick={next} disabled={saving || capturing}>
        {#if saving}<span class="spinner"></span> Saving…{:else if step === last}Finish{:else}Next{/if}
      </button>
    </footer>
  </div>
</div>

<style>
  .screen {
    display: grid;
    place-items: center;
    min-height: 100%;
    padding: 2rem;
    background:
      radial-gradient(36rem 28rem at 0% 0%, color-mix(in srgb, var(--accent) 30%, transparent), transparent 65%),
      var(--bg);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    width: min(34rem, 100%);
    padding: 1.75rem;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: var(--surface);
    box-shadow: 0 24px 60px -24px rgba(0, 0, 0, 0.6);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-family: var(--font-display);
    font-weight: 700;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 10px;
    background: linear-gradient(145deg, var(--accent-hover), color-mix(in srgb, var(--accent) 88%, #000));
    color: #fff;
  }

  .steps {
    display: flex;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 0.45rem;
    padding-top: 0.6rem;
    border-top: 3px solid var(--surface-3);
    color: var(--text-faint);
    font-size: 0.85rem;
  }

  .steps li.done,
  .steps li.current {
    border-top-color: var(--accent);
    color: var(--text);
  }

  .num {
    display: grid;
    place-items: center;
    width: 1.3rem;
    height: 1.3rem;
    border-radius: 50%;
    background: var(--surface-3);
    font-size: 0.75rem;
    font-weight: 700;
  }

  .current .num,
  .done .num {
    background: var(--accent);
    color: #fff;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-height: 12rem;
  }

  h1 {
    font-size: 1.5rem;
    font-weight: 750;
  }

  .body p {
    margin: 0 0 0.5rem;
  }

  .hotkey {
    display: flex;
    align-items: center;
    gap: 1.25rem;
    min-height: 4rem;
  }

  .capture {
    flex: 1;
    padding: 0.7rem 1rem;
    border: 1px dashed var(--accent);
    border-radius: 12px;
    background: var(--accent-soft);
    font-weight: 600;
    outline: none;
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      border-color: transparent;
    }
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .chip {
    padding: 0.55rem 1rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .chip:hover {
    background: var(--surface-3);
  }

  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-text);
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

  .help {
    color: var(--text-muted);
  }

  .error {
    color: var(--danger);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }
</style>

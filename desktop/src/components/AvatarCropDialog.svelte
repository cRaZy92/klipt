<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Account } from "../lib/api";
  import { cropSquare, MAX_ZOOM, MIN_ZOOM, panBy, zoomTo, type Point, type Size } from "../lib/avatarCrop";
  import Modal from "./Modal.svelte";

  // Crops a picture to a square for the profile picture (drag to move, zoom with the slider or
  // the wheel, round preview), then sends it as a 512×512 JPEG.
  let { src, onclose, onsaved }: { src: string; onclose: () => void; onsaved: (account: Account) => void } = $props();

  const VIEW = 280;
  const OUT = 512;
  let canvas = $state<HTMLCanvasElement>();
  let img = $state.raw<HTMLImageElement | null>(null);
  let size = $state<Size>({ w: 1, h: 1 });
  let zoom = $state(1);
  let center = $state<Point>({ x: 0, y: 0 });
  let drag: Point | null = null;
  let saving = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    const picture = new Image();
    picture.onload = () => {
      img = picture;
      size = { w: picture.naturalWidth, h: picture.naturalHeight };
      center = { x: size.w / 2, y: size.h / 2 };
    };
    picture.onerror = () => (error = "Klipt can't read that file as a picture.");
    picture.src = src;
  });

  function draw(target: HTMLCanvasElement, px: number) {
    const ctx = target.getContext("2d");
    if (!ctx || !img) return;
    const { sx, sy, size: side } = cropSquare(size, zoom, center);
    ctx.clearRect(0, 0, px, px);
    ctx.drawImage(img, sx, sy, side, side, 0, 0, px, px);
  }

  $effect(() => {
    void [zoom, center, size];
    if (!canvas) return;
    const px = Math.round(VIEW * (window.devicePixelRatio || 1));
    canvas.width = canvas.height = px;
    draw(canvas, px);
  });

  function setZoom(z: number) {
    ({ zoom, center } = zoomTo(size, z, center));
  }

  function pointerdown(e: PointerEvent) {
    drag = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function pointermove(e: PointerEvent) {
    if (!drag) return;
    center = panBy(size, zoom, center, e.clientX - drag.x, e.clientY - drag.y, (e.currentTarget as HTMLElement).clientWidth);
    drag = { x: e.clientX, y: e.clientY };
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    setZoom(zoom * (e.deltaY < 0 ? 1.08 : 1 / 1.08));
  }

  async function save() {
    if (!img) return;
    saving = true;
    error = null;
    try {
      const out = document.createElement("canvas");
      out.width = out.height = OUT;
      draw(out, OUT);
      const blob = await new Promise<Blob | null>((resolve) => out.toBlob(resolve, "image/jpeg", 0.9));
      if (!blob) throw new Error("Klipt couldn't make the picture.");
      onsaved(await api.setAvatar(new Uint8Array(await blob.arrayBuffer())));
    } catch (e) {
      error = errorMessage(e);
      saving = false;
    }
  }
</script>

<Modal title="Crop your picture" {onclose} width="22rem">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="stage" onpointerdown={pointerdown} onpointermove={pointermove} onpointerup={() => (drag = null)} onwheel={wheel}>
    <canvas bind:this={canvas}></canvas>
  </div>
  <label class="zoom">
    Zoom
    <input type="range" min={MIN_ZOOM} max={MAX_ZOOM} step="0.01" value={zoom} oninput={(e) => setZoom(Number(e.currentTarget.value))} />
  </label>
  <p class="hint">Drag to move the picture.</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" onclick={save} disabled={saving || !img}>
      {#if saving}<span class="spinner"></span> Saving…{:else}Save picture{/if}
    </button>
  {/snippet}
</Modal>

<style>
  .stage {
    width: 280px;
    height: 280px;
    margin: 0 auto;
    border-radius: 50%;
    overflow: hidden;
    background: var(--bg);
    box-shadow: 0 0 0 1px var(--border-strong);
    touch-action: none;
    cursor: grab;
  }
  .stage:active {
    cursor: grabbing;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 1rem;
    color: var(--text-muted);
  }
  .zoom input {
    flex: 1;
  }
  .hint {
    margin: 0.4rem 0 0;
    color: var(--text-faint);
    font-size: 0.85rem;
  }
  .error {
    color: var(--danger-text);
  }
</style>

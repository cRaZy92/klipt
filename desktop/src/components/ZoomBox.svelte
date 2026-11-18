<script lang="ts">
  // The picked zoom's area over the video: drag it to move it, drag a corner to resize it (it
  // keeps the picture's shape). Outside it the picture is dimmed.
  import type { ZoomRange } from "../lib/api";
  import { boxFromRect, boxRect, MAX_ZOOM, MIN_ZOOM } from "../lib/zoom";

  let {
    zoom,
    rect,
    onchange,
    ongesture,
  }: {
    zoom: ZoomRange;
    /** Where the picture shows on the stage, in stage pixels. */
    rect: { left: number; top: number; width: number; height: number };
    onchange: (box: { zoom: number; x: number; y: number }) => void;
    /** A drag started (true) or ended (false): one drag is one undo step. */
    ongesture: (active: boolean) => void;
  } = $props();

  type Corner = "nw" | "ne" | "sw" | "se";
  const box = $derived(boxRect(zoom));

  /** The drag in progress: what it grabbed and the box when it started (fractions of the frame). */
  let drag: { what: "move" | Corner; x: number; y: number; start: { left: number; top: number; width: number } } | null =
    null;

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  /** The pointer as fractions of the picture. */
  function at(e: PointerEvent): { x: number; y: number } {
    const stage = (e.currentTarget as HTMLElement).closest(".zoom-layer")!.getBoundingClientRect();
    return { x: (e.clientX - stage.left) / stage.width, y: (e.clientY - stage.top) / stage.height };
  }

  function down(e: PointerEvent, what: "move" | Corner) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const p = at(e);
    drag = { what, x: p.x, y: p.y, start: { left: box.left, top: box.top, width: box.width } };
    ongesture(true);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const p = at(e);
    const s = drag.start;
    if (drag.what === "move") {
      const left = clamp(s.left + p.x - drag.x, 0, 1 - s.width);
      const top = clamp(s.top + p.y - drag.y, 0, 1 - s.width);
      onchange(boxFromRect({ left, top, width: s.width }));
      return;
    }
    // A corner: the opposite corner stays put and the box keeps its shape.
    const west = drag.what.endsWith("w");
    const north = drag.what.startsWith("n");
    const ax = west ? s.left + s.width : s.left;
    const ay = north ? s.top + s.width : s.top;
    const fits = Math.min(west ? ax : 1 - ax, north ? ay : 1 - ay);
    const wanted = Math.max(west ? ax - p.x : p.x - ax, north ? ay - p.y : p.y - ay);
    const size = clamp(Math.min(wanted, fits), 1 / MAX_ZOOM, 1 / MIN_ZOOM);
    const left = clamp(west ? ax - size : ax, 0, 1 - size);
    const top = clamp(north ? ay - size : ay, 0, 1 - size);
    onchange(boxFromRect({ left, top, width: size }));
  }

  function up() {
    if (!drag) return;
    drag = null;
    ongesture(false);
  }
</script>

<div
  class="zoom-layer"
  style:left="{rect.left}px"
  style:top="{rect.top}px"
  style:width="{rect.width}px"
  style:height="{rect.height}px"
>
  <!-- Moved with the pointer; the zoom's time is set on the Zoom lane. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="box"
    style:left="{box.left * 100}%"
    style:top="{box.top * 100}%"
    style:width="{box.width * 100}%"
    style:height="{box.height * 100}%"
    title="Drag to move the zoom, drag a corner to zoom further in or out"
    onpointerdown={(e) => down(e, "move")}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onlostpointercapture={up}
  >
    <span class="factor">{zoom.zoom.toFixed(1)}×</span>
    {#each ["nw", "ne", "sw", "se"] as const as c (c)}
      <span
        class="corner {c}"
        onpointerdown={(e) => down(e, c)}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
        onlostpointercapture={up}
      ></span>
    {/each}
  </div>
</div>

<style>
  .zoom-layer {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }

  .box {
    position: absolute;
    box-sizing: border-box;
    border: 2px solid var(--accent);
    border-radius: 2px;
    /* Dims the picture outside the box (the layer clips it to the picture). */
    box-shadow: 0 0 0 100vmax rgba(5, 4, 10, 0.55);
    cursor: move;
    pointer-events: auto;
    touch-action: none;
  }

  .factor {
    position: absolute;
    top: 0.35rem;
    left: 0.35rem;
    padding: 0.1rem 0.4rem;
    border-radius: 6px;
    background: rgba(5, 4, 10, 0.7);
    color: #fff;
    font-size: 0.75rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .corner {
    position: absolute;
    width: 14px;
    height: 14px;
    border: 2px solid var(--accent);
    border-radius: 3px;
    background: #fff;
  }
  .corner.nw {
    top: -8px;
    left: -8px;
    cursor: nwse-resize;
  }
  .corner.ne {
    top: -8px;
    right: -8px;
    cursor: nesw-resize;
  }
  .corner.sw {
    bottom: -8px;
    left: -8px;
    cursor: nesw-resize;
  }
  .corner.se {
    right: -8px;
    bottom: -8px;
    cursor: nwse-resize;
  }
</style>

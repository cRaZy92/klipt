<script lang="ts" module>
  // Inline SVG icons (24×24, stroked), so there's no icon font to load.
  const PATHS = {
    upload: "M12 16V4m0 0-5 5m5-5 5 5M4 17v2a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-2",
    link: "M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1",
    check: "M5 12.5l4.5 4.5L19 7.5",
    cloud: "M7 19a5 5 0 0 1-.9-9.92A6.5 6.5 0 0 1 18.6 8.6 4.6 4.6 0 0 1 17.5 19z",
    cloudCheck: "M7 19a5 5 0 0 1-.9-9.92A6.5 6.5 0 0 1 18.6 8.6 4.6 4.6 0 0 1 17.5 19zM9.2 13.4l2 2 3.6-3.6",
    pc: "M3 5a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1zM8 20h8M12 16v4",
    eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    more: "M5 12h.01M12 12h.01M19 12h.01",
    bulb: "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.5 10.9c.6.4 1 1.1 1 1.8V16h5v-.3c0-.7.4-1.4 1-1.8A6 6 0 0 0 12 3z",
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3",
    tag: "M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9zM7.5 7.5h.01",
    rename: "M4 7V5h16v2M12 5v14M9 19h6",
    combine: "M3 7h6v10H3zM15 7h6v10h-6zM9 12h6",
    album: "M4 6h16M4 12h10M4 18h7M18 14v6M15 17h6",
    arrowUp: "M12 19V5m0 0-6 6m6-6 6 6",
    arrowDown: "M12 5v14m0 0-6-6m6 6 6-6",
    scissors: "M6 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm0 12a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12",
    settings:
      "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
    record: "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z",
    sort: "M7 4v16m0 0-3-3m3 3 3-3M17 20V4m0 0-3 3m3-3 3 3",
    film: "M3 6a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM10 9l5 3-5 3z",
    alert: "M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
    close: "M6 6l12 12M18 6 6 18",
    plus: "M12 5v14M5 12h14",
    back: "M15 18l-6-6 6-6",
    forward: "M9 18l6-6-6-6",
    chevronDown: "M6 9l6 6 6-6",
    image: "M4 5h16v14H4zM4 15.5l4.5-4.5 4 4 3-3 4.5 4.5M15.5 9h.01",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
    zoom: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4M11 8v6M8 11h6",
    zoomOut: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4M8 11h6",
    subtitles: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM7 15h4M13 15h4M7 11h10",
    star: "M12 3.6l2.6 5.2 5.8.9-4.2 4.1 1 5.7L12 16.8l-5.2 2.7 1-5.7-4.2-4.1 5.8-.9z",
    starFilled: "M12 3.6l2.6 5.2 5.8.9-4.2 4.1 1 5.7L12 16.8l-5.2 2.7 1-5.7-4.2-4.1 5.8-.9z",
    retry: "M4 12a8 8 0 0 1 14-5.3L20 9M20 4v5h-5M20 12a8 8 0 0 1-14 5.3L4 15m0 5v-5h5",
    logout: "M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l-5-5 5-5M5 12h12",
    server: "M4 5h16v6H4zM4 13h16v6H4zM8 8h.01M8 16h.01",
    users:
      "M16 20v-1.5a3.5 3.5 0 0 0-3.5-3.5h-5A3.5 3.5 0 0 0 4 18.5V20M10 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM21 20v-1.5a3.5 3.5 0 0 0-2.6-3.4M15.5 4.2a3.5 3.5 0 0 1 0 6.6",
    user: "M19 20v-1.5a3.5 3.5 0 0 0-3.5-3.5h-7A3.5 3.5 0 0 0 5 18.5V20M12 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z",
    mic: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM18.5 11a6.5 6.5 0 0 1-13 0M12 17.5V21M8.5 21h7",
    keyboard: "M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM7 9h.01M11 9h.01M15 9h.01M7 12.5h.01M11 12.5h.01M15 12.5h.01M17 9h.01M17 12.5h.01M8 16h8",
    volume: "M11 5 6.5 9H3v6h3.5l4.5 4zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13",
    sliders: "M5 20v-6M5 10V4M12 20v-8M12 8V4M19 20v-4M19 12V4M3 14h4M10 8h4M17 16h4",
    external: "M18 13.5V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5.5M15 3h6v6M10 14 21 3",
    download: "M12 4v12m0 0-5-5m5 5 5-5M4 17v2a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-2",
    globe:
      "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3.5 9h17M3.5 15h17M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18",
    play: "",
    pause: "",
    volumeOff: "M11 5 6.5 9H3v6h3.5l4.5 4zM16 9.5l5 5M21 9.5l-5 5",
    fullscreen: "M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5",
    fullscreenExit: "M9 4v5H4M20 9h-5V4M15 20v-5h5M4 15h5v5",
    pulse: "M3 12h4l3-7 4 14 3-7h4",
    info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5M12 8h.01",
    clock: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2",
    spinner: "M12 3a9 9 0 1 0 9 9",
    lock: "M6.5 11h11a1.5 1.5 0 0 1 1.5 1.5v7a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19.5v-7A1.5 1.5 0 0 1 6.5 11zM8 11V7.5a4 4 0 0 1 8 0V11M12 15v2",
    trophy:
      "M7 4h10v5a5 5 0 0 1-10 0zM7 6H4.5a2.5 2.5 0 0 0 2.6 4.5M17 6h2.5a2.5 2.5 0 0 1-2.6 4.5M12 14v3.5M8 21h8M9 21c0-2 1.3-3.5 3-3.5s3 1.5 3 3.5",
    palette:
      "M12 21a9 9 0 1 1 9-9c0 2-1.6 3.2-3.6 3.2h-1.9a1.8 1.8 0 0 0-1.3 3.1A1.7 1.7 0 0 1 12 21zM7.5 11.5h.01M9.5 7.5h.01M14.5 7.5h.01M16.5 11.5h.01",
    // The app icon's glyph (src-tauri/icons/source.svg scaled 1/32 about its centre): frame brackets
    // here, the play triangle in FILLS. Put it on an accent tile, sized ~5/6 of the tile, to match.
    logo: "M5.375 5.375H3.5v13.25h1.875M18.625 5.375H20.5v13.25h-1.875",
  } as const;

  export type IconName = keyof typeof PATHS;

  // Solid shapes drawn over an icon's strokes.
  const FILLS: Partial<Record<IconName, string>> = {
    logo: "M9.75 7.875v8.25L16.625 12z",
    starFilled: "M12 3.6l2.6 5.2 5.8.9-4.2 4.1 1 5.7L12 16.8l-5.2 2.7 1-5.7-4.2-4.1 5.8-.9z",
    play: "M8 5.5v13a.8.8 0 0 0 1.2.7l10.4-6.5a.8.8 0 0 0 0-1.4L9.2 4.8A.8.8 0 0 0 8 5.5z",
    pause: "M6.5 5h3.5v14H6.5zM14 5h3.5v14H14z",
  };
</script>

<script lang="ts">
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
  {#if FILLS[name]}<path d={FILLS[name]} fill="currentColor" stroke="none" />{/if}
</svg>

# Editor subtitles — design (short)

Approved by the owner 2026-11-20.

## Intent
Clips often need to show what people said (voice chat, mic) so they make sense with the sound off.
Speech-to-text was considered and deferred; this version is typed subtitles. Stored timed text
blocks are the same shape a later transcription feature would produce.

## Goals
- A Subtitles lane in the editor, under the Zooms lane. **T** adds a 2 s block at the playhead and
  opens a text input; Enter commits, Esc cancels (an empty new block is dropped).
- Drag edges to retime, drag the middle to move, double-click to edit text, Delete removes the
  picked block. Blocks never overlap. Undo/redo and autosave like zooms/cuts.
- One fixed style: white bold text, black outline, centred near the bottom, wraps to a second line.
- Preview draws the subtitle over the video with the same bundled font the export uses.
- Export (and therefore upload, the website and Discord embeds) burns the subtitles into the
  picture; any subtitle in the trim forces a re-encode. Timing follows trims and cuts
  (`joined_time`), and a block cut in half keeps only its kept part(s).
- Saved in the clip's edit like `zoom_ranges`.

## Non-goals
Style, colour or position choices; speech-to-text; sidecar `.srt`/soft subtitles; server changes.

## Approaches
1. **Lane + burned-in at export, like zooms (picked).** Works everywhere a clip goes (Discord,
   site, downloads) and follows trims/cuts.
2. Soft subtitle track / `.srt` uploaded beside the video — rejected: Discord embeds and most
   players ignore it, and the server would need changes.
3. Separate subtitle editor screen — rejected: the editor is already where every clip passes.

## How
- Rust: `SubtitleRange { start_ms, end_ms, text }` in `library`, stored with the edit (migration
  for a JSON column, same as zooms). `Edit.subtitles`; `reencodes` true when a subtitle overlaps
  the trim.
- Export writes a temporary `.ass` file (fixed style, PlayRes = video size, times mapped into the
  export's timeline) and adds `subtitles=<file>:fontsdir=<bundled fonts dir>` after the zoom filter,
  so zoom does not scale the text. ASS text must escape `{`, `}`, `\` and newlines.
- Font: a bold open-licence sans with full Latin Extended (Slovak/Czech), bundled as a Tauri
  resource; the same file is loaded with `@font-face` for the preview.
- Builder's calls: exact font, outline width and size (relative to video height), lane UI details.

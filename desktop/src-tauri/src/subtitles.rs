//! Subtitles drawn onto exports: the ASS file ffmpeg's `subtitles` filter (libass) renders, the
//! font it uses, and escaping paths into a filter graph.

use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::library::SubtitleRange;

/// The subtitles' font, bundled so every machine draws the same letters (Slovak and Czech too).
const FONT: &[u8] = include_bytes!("../../src/assets/fonts/NotoSans-Bold.ttf");
const FONT_FILE: &str = "NotoSans-Bold.ttf";
/// The font's family name, as the ASS style names it.
const FONT_NAME: &str = "Noto Sans";
/// Emoji, in one colour: libass can't draw colour emoji fonts, and its fallback would pick one.
const EMOJI_FONT: &[u8] = include_bytes!("../../src/assets/fonts/NotoEmoji.ttf");
const EMOJI_FONT_FILE: &str = "NotoEmoji.ttf";
const EMOJI_FONT_NAME: &str = "Noto Emoji";

/// Whether `c` is an emoji picture (drawn with the emoji font).
fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x2300..=0x23FF)
}

/// Whether `c` belongs to the emoji before it: a zero-width joiner, a variation selector or a keycap.
fn joins_emoji(c: char) -> bool {
    matches!(c, '\u{200D}' | '\u{FE0F}' | '\u{20E3}' | '\u{E0020}'..='\u{E007F}')
}

/// The subtitles as they show in an export of `kept` (the file's kept parts, in order): each
/// subtitle's overlap with each part, moved to the export's time. A subtitle across a removed
/// part shows on both sides of it; one inside a removed part or outside the trim doesn't show.
pub fn events(subs: &[SubtitleRange], kept: &[(u64, u64)]) -> Vec<(u64, u64, String)> {
    let mut out = Vec::new();
    let mut before = 0;
    for &(ks, ke) in kept {
        for s in subs {
            let (start, end) = (s.start_ms.max(ks), s.end_ms.min(ke));
            if end > start {
                out.push((before + start - ks, before + end - ks, s.text.clone()));
            }
        }
        before += ke.saturating_sub(ks);
    }
    out
}

/// `text` as ASS dialogue text that shows exactly as typed: braces would start override tags and
/// `\N`, `\n`, `\h` would be line breaks or spaces. Emoji switch to the emoji font and back.
pub fn ass_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_emoji = false;
    while let Some(c) = chars.next() {
        let emoji = is_emoji(c) || (in_emoji && joins_emoji(c));
        if emoji != in_emoji {
            out.push_str(if emoji { "{\\fn" } else { "{\\fn}" });
            if emoji {
                out.push_str(EMOJI_FONT_NAME);
                out.push('}');
            }
            in_emoji = emoji;
        }
        match c {
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\\' => {
                out.push('\\');
                // A word joiner (invisible) keeps the backslash from escaping the letter.
                if matches!(chars.peek(), Some('N' | 'n' | 'h')) {
                    out.push('\u{2060}');
                }
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    if in_emoji {
        out.push_str("{\\fn}");
    }
    out
}

/// An ASS time: `H:MM:SS.cc`.
fn ass_time(ms: u64) -> String {
    let cs = ms / 10;
    format!("{}:{:02}:{:02}.{:02}", cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100)
}

/// The ASS file for `events` on a `size` picture: one fixed style (white bold text with a black
/// outline, bottom centre, wrapped), sized from the picture's height.
pub fn ass_document(events: &[(u64, u64, String)], (w, h): (u32, u32)) -> String {
    let px = |f: f64| (f * h as f64).round() as u32;
    let size = px(0.055);
    let outline = px(0.0035).max(2);
    let margin_v = px(0.06);
    let margin_lr = (0.05 * w as f64).round() as u32;
    let mut doc = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n\
         [V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, \
         Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, \
         MarginV, Encoding\n\
         Style: Default,{FONT_NAME},{size},&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,{outline},0,2,\
         {margin_lr},{margin_lr},{margin_v},1\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    for (start, end, text) in events {
        doc.push_str(&format!("Dialogue: 0,{},{},Default,,0,0,0,,{}\n", ass_time(*start), ass_time(*end), ass_text(text)));
    }
    doc
}

/// `p` as a filter option value inside a `-filter_complex` graph. The graph is unescaped twice:
/// once into filters (`\ ' [ ] , ;` are special) and once into options (`\ ' :`).
pub fn filter_path(p: &Path) -> String {
    let escape = |s: &str, special: &[char]| {
        s.chars().fold(String::new(), |mut out, c| {
            if special.contains(&c) {
                out.push('\\');
            }
            out.push(c);
            out
        })
    };
    let option = escape(&p.to_string_lossy(), &['\\', '\'', ':']);
    escape(&option, &['\\', '\'', '[', ']', ',', ';'])
}

/// Writes the bundled fonts to `<cache_dir>/fonts/` (unless they're there already) and returns
/// that directory, for the `subtitles` filter's `fontsdir`. Each is written to a temporary file
/// first, so an export running meanwhile never reads half a font.
pub fn write_font(cache_dir: &Path) -> Result<PathBuf> {
    let dir = cache_dir.join("fonts");
    for (name, bytes) in [(FONT_FILE, FONT), (EMOJI_FONT_FILE, EMOJI_FONT)] {
        let file = dir.join(name);
        if fs::metadata(&file).map(|m| m.len() != bytes.len() as u64).unwrap_or(true) {
            fs::create_dir_all(&dir)?;
            let temp = dir.join(format!("{name}.{}.tmp", std::process::id()));
            fs::write(&temp, bytes)?;
            fs::rename(&temp, &file)?;
        }
    }
    Ok(dir)
}

/// The `subtitles` filter that draws the ASS file `ass` with the fonts in `fonts_dir`.
pub fn filter(ass: &Path, fonts_dir: &Path) -> String {
    format!("subtitles=filename={}:fontsdir={}", filter_path(ass), filter_path(fonts_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(start_ms: u64, end_ms: u64, text: &str) -> SubtitleRange {
        SubtitleRange { start_ms, end_ms, text: text.into() }
    }

    #[test]
    fn events_follow_the_trim() {
        assert_eq!(events(&[s(5000, 7000, "a")], &[(4000, 20_000)]), vec![(1000, 3000, "a".into())]);
        assert_eq!(events(&[s(5000, 7000, "a")], &[(4000, u64::MAX)]), vec![(1000, 3000, "a".into())]);
        assert!(events(&[s(1000, 3000, "a")], &[(4000, 20_000)]).is_empty(), "before the trim");
    }

    #[test]
    fn events_split_across_a_cut() {
        let kept = [(0, 4000), (6000, 10_000)];
        assert_eq!(events(&[s(3000, 7000, "a")], &kept), vec![(3000, 4000, "a".into()), (4000, 5000, "a".into())]);
        assert!(events(&[s(4500, 5500, "a")], &kept).is_empty(), "inside the removed part");
    }

    #[test]
    fn ass_text_shows_markup_literally() {
        assert_eq!(ass_text("{\\b0}lol"), "\\{\\b0\\}lol");
        assert!(!ass_text("a\\Nb").contains("\\N"));
        assert!(!ass_text("a\\hb").contains("\\h"));
        assert_eq!(ass_text("C:\\new").replace('\u{2060}', ""), "C:\\new");
        assert_eq!(ass_text("čo to bolo, ľudia"), "čo to bolo, ľudia");
    }

    #[test]
    fn emoji_are_drawn_with_the_emoji_font() {
        assert_eq!(ass_text("gg 💀🔥 ez"), "gg {\\fnNoto Emoji}💀🔥{\\fn} ez");
        // Sequences stay together: ZWJ, a skin tone, the emoji variation selector, a keycap.
        assert_eq!(ass_text("👍🏽"), "{\\fnNoto Emoji}👍🏽{\\fn}");
        assert_eq!(ass_text("❤️ a"), "{\\fnNoto Emoji}❤️{\\fn} a");
        assert_eq!(ass_text("🧑‍💻"), "{\\fnNoto Emoji}🧑‍💻{\\fn}");
        assert_eq!(ass_text("čo ľudia"), "čo ľudia");
    }

    // Review: the preview is sized from this (src/lib/subtitles.ts, `subtitleFontPx`): a capital
    // is 0.055 × 714/1519 of the picture's height, because libass sizes a font by its whole
    // ascent + descent (1519 units for Noto Sans), not by its em (1000 units).
    #[test]
    fn a_capital_is_as_tall_as_the_preview_expects() {
        let tools = crate::ffmpeg::locate().expect("ffmpeg");
        let dir = tempfile::tempdir().unwrap();
        let fonts = write_font(dir.path()).unwrap();
        let ass = dir.path().join("h.ass");
        fs::write(&ass, ass_document(&[(0, 5000, "HHHH".into())], (1920, 1080))).unwrap();
        let out = std::process::Command::new(&tools.ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-f", "lavfi", "-i", "color=black:s=1920x1080:d=1", "-vf"])
            .arg(format!("{},format=gray", filter(&ass, &fonts)))
            .args(["-frames:v", "1", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        // Rows where the letters' white fill (not the black outline) is.
        let rows: Vec<usize> = (0..1080).filter(|y| out.stdout[y * 1920..(y + 1) * 1920].iter().any(|p| *p > 200)).collect();
        let cap = rows.last().unwrap() - rows.first().unwrap() + 1;
        let expected = 0.055 * 1080.0 * 714.0 / 1519.0;
        assert!((cap as f64 - expected).abs() <= 2.0, "capital {cap} px, expected {expected:.1}");
    }

    #[test]
    fn ass_document_uses_the_fixed_style() {
        let doc = ass_document(&[(1000, 3000, "hi".into()), (61_230, 3_723_450, "x".into())], (1920, 1080));
        assert!(doc.contains("PlayResX: 1920\nPlayResY: 1080"));
        assert!(doc.contains("Style: Default,Noto Sans,59,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,4,0,2,96,96,65,1"));
        assert!(doc.contains("Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,hi\n"));
        assert!(doc.contains("Dialogue: 0,0:01:01.23,1:02:03.45,Default,,0,0,0,,x\n"));
        let small = ass_document(&[], (640, 360));
        assert!(small.contains(",1,2,0,2,32,32,22,1"), "the outline is at least 2: {small}");
    }

    #[test]
    fn filter_path_escapes_windows_paths() {
        assert_eq!(filter_path(Path::new("C:\\Users\\Ann's PC\\x.ass")), "C\\\\:\\\\\\\\Users\\\\\\\\Ann\\\\\\'s PC\\\\\\\\x.ass");
        assert_eq!(filter_path(Path::new("/tmp/a,b[c];d")), "/tmp/a\\,b\\[c\\]\\;d");
    }

    #[test]
    fn both_fonts_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let fonts = write_font(dir.path()).unwrap();
        assert!(fonts.join("NotoEmoji.ttf").is_file());
    }

    #[test]
    fn the_font_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        let fonts = write_font(dir.path()).unwrap();
        let file = fonts.join(FONT_FILE);
        assert_eq!(fs::read(&file).unwrap().len(), FONT.len());
        fs::write(&file, b"broken").unwrap();
        write_font(dir.path()).unwrap();
        assert_eq!(fs::read(&file).unwrap().len(), FONT.len(), "a damaged copy is replaced");
    }
}

// The preview column's twelve states and the facts each one states, ui/js/Facts.js and the half of
// ui/js/Kinds.js it reads: one anatomy for all twelve, a frame, the name, and a caption-type table of
// label and value pairs under it. Toolkit free; columnpreview.rs draws it.
use crate::format;
use crate::kinds::{is_markdown, is_pdf, is_tabular};
use crate::wire::{Meta, Row};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    Code,
    Archive,
    Symlink,
    Multi,
    Loading,
    Error,
    Unsupported,
}

// src/backend/rows.rs UNKNOWN_KIND: the Kind a row gets when nothing could describe its type.
pub const UNKNOWN_KIND: &str = "Data";

const CODE_EXTENSIONS: [&str; 38] = [
    ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".c", ".h", ".cpp", ".hpp", ".cc", ".go", ".rb", ".java", ".kt", ".swift", ".lua",
    ".pl", ".php", ".sh", ".bash", ".zsh", ".fish", ".vim", ".qml", ".css", ".scss", ".html", ".xml", ".json", ".toml", ".yaml",
    ".yml", ".ini", ".conf", ".sql", ".mjs", ".cjs",
];

pub fn is_code(name: &str) -> bool {
    let lower = name.to_lowercase();
    CODE_EXTENSIONS.iter().any(|e| lower.len() > e.len() && lower.ends_with(e))
}

// Kinds.kindState: the icon's freedesktop family classifies, and code keeps its own state here,
// where kinds::quick_look folds it into text for the overlay.
pub fn kind_state(icon: &str) -> State {
    match icon {
        "text-x-script" | "text-html" | "application-xml" => State::Code,
        "package-x-generic" => State::Archive,
        _ if icon.starts_with("image-") => State::Image,
        _ if icon.starts_with("video-") => State::Video,
        _ if icon.starts_with("audio-") => State::Audio,
        _ if icon.starts_with("text-") => State::Text,
        _ => State::Unsupported,
    }
}

// Facts.state: a multi-selection, a failure and a preview still arriving outrank what the row is.
pub fn state(row: Option<&Row>, selection: usize, loading: bool, failure: &str, kind_name: &str) -> State {
    if selection > 1 {
        return State::Multi;
    }
    let Some(row) = row else { return State::Unsupported };
    if !failure.is_empty() {
        return State::Error;
    }
    if loading {
        return State::Loading;
    }
    if format::is_symlink(row.p) {
        return State::Symlink;
    }
    // A name nothing recognised falls to text-x-generic on the icon ladder; its Kind says the truth.
    if kind_name == UNKNOWN_KIND {
        return State::Unsupported;
    }
    if is_pdf(&row.n) {
        return State::Pdf;
    }
    if is_markdown(&row.n) || is_tabular(&row.n) {
        return State::Text;
    }
    match kind_state(&row.i) {
        // Only a row the icon already called text is promoted to code; an image named .css is not.
        State::Text if is_code(&row.n) => State::Code,
        s => s,
    }
}

pub type Fact = (&'static str, String);

// HANDOFF rule 19: a label with no value is not drawn.
fn filled(rows: Vec<Fact>) -> Vec<Fact> {
    rows.into_iter().filter(|(_, v)| !v.is_empty()).collect()
}

// Format.permissions: the low nine bits of st_mode, read three at a time.
pub fn permissions(mode: u32) -> String {
    (0..9).map(|i| if mode & (1 << (8 - i)) != 0 { ['r', 'w', 'x'][i % 3] } else { '-' }).collect()
}

pub fn duration(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// A whole number of kilohertz reads without a decimal: 48000 is "48 kHz", 44100 is "44.1 kHz".
pub fn sample_rate(hz: u64) -> String {
    match hz {
        0 => String::new(),
        _ if hz % 1000 == 0 => format!("{} kHz", hz / 1000),
        _ => format!("{:.1} kHz", hz as f64 / 1000.0),
    }
}

// A count the backend stopped early is a floor; a file it could not open has no count at all.
fn line_count(meta: Option<&Meta>) -> String {
    match meta {
        Some(m) if !m.lines_failed => format!("{}{}", if m.partial { "> " } else { "" }, m.lines),
        _ => String::new(),
    }
}

fn pixels(meta: Option<&Meta>) -> String {
    match meta {
        Some(m) if m.w > 0 && m.h > 0 => format!("{} × {}", m.w, m.h),
        _ => String::new(),
    }
}

// The board's "3, 240.8 kB out": Size already says what the archive weighs packed.
fn archive_fact(meta: Option<&Meta>) -> String {
    match meta {
        Some(m) if m.entries > 0 && m.unpacked > 0 => format!("{}, {} out", m.entries, format::size(m.unpacked)),
        Some(m) if m.entries > 0 => m.entries.to_string(),
        _ => String::new(),
    }
}

// Facts.facts: the rows the canvas draws for each state, in its own order, with its own labels.
// `pages` is the PDF's page count, 0 until a page has been read.
pub fn facts(st: State, row: &Row, meta: Option<&Meta>, kind_name: &str, pages: usize) -> Vec<Fact> {
    let kind = || ("Kind", kind_name.to_string());
    let size = || ("Size", format::size(row.s));
    let mode = || ("Mode", permissions(row.p));
    // Every ordinary settled kind opens with the same three, so each answer keeps its line.
    let mut out = vec![kind(), size(), ("Modified", format::date(row.m))];
    match st {
        State::Image => out.push(("Pixels", pixels(meta))),
        State::Video => out.extend([("Duration", meta.filter(|m| m.ms > 0).map(|m| duration(m.ms)).unwrap_or_default()), ("Pixels", pixels(meta))]),
        State::Audio => out.extend([
            ("Duration", meta.filter(|m| m.ms > 0).map(|m| duration(m.ms)).unwrap_or_default()),
            ("Rate", meta.map(|m| sample_rate(m.rate)).unwrap_or_default()),
        ]),
        State::Pdf => out.push(("Pages", if pages > 0 { pages.to_string() } else { String::new() })),
        State::Text => out.push(("Lines", line_count(meta))),
        // Mode stays: the columns view has no permission column, so dropping it here would lose it.
        State::Code => out.extend([("Lines", line_count(meta)), mode()]),
        State::Archive => out.push(("Entries", archive_fact(meta))),
        State::Symlink => {
            out = vec![
                ("Kind", "Symbolic link".into()),
                ("Target", meta.map(|m| m.target.clone()).unwrap_or_default()),
                ("Points at", if meta.is_some_and(|m| m.target_dir) { "Folder" } else { "File" }.into()),
                mode(),
            ]
        }
        State::Loading => out = vec![kind(), size(), ("State", "loading".into())],
        State::Error => out = vec![kind(), size(), mode(), ("Owner", meta.map(|m| m.owner.clone()).unwrap_or_default())],
        // Unsupported, and Multi, which never reaches here: multi_facts summarises a selection.
        State::Unsupported | State::Multi => out.push(mode()),
    }
    filled(out)
}

// Facts.kindWord: one word per kind for the Kinds row.
fn kind_word(icon: &str) -> &'static str {
    match kind_state(icon) {
        State::Image => "image",
        State::Video => "video",
        State::Audio => "audio",
        State::Code => "code file",
        State::Text => "text",
        State::Archive => "archive",
        _ => "file",
    }
}

// Facts.kindGroups: the distinct kinds in the order they were met, each with its first member's
// mark and a count; the frame's stack and the Kinds row read this one pass.
pub fn kind_groups(rows: &[&Row]) -> Vec<(&'static str, &'static str, usize)> {
    let mut out: Vec<(&'static str, &'static str, usize)> = vec![];
    for r in rows {
        let word = kind_word(&r.i);
        match out.iter_mut().find(|g| g.0 == word) {
            Some(g) => g.2 += 1,
            None => out.push((word, crate::marks::glyph_for(&r.i), 1)),
        }
    }
    out
}

// The canvas stacks three marks, front-most the kind the Kinds row names first.
pub const MULTI_MARK_CAP: usize = 3;

pub fn multi_marks(rows: &[&Row]) -> Vec<&'static str> {
    kind_groups(rows).into_iter().take(MULTI_MARK_CAP).map(|g| g.1).collect()
}

// Facts.multiFacts: a summary, never a collage. `selection` is the selection's true size and `rows`
// only what the held window carries; a wider selection cannot be summed without a sweep, so the
// numbers become floors and say so.
pub fn multi_facts(rows: &[&Row], selection: usize) -> Vec<Fact> {
    let floor = if selection > rows.len() { "> " } else { "" };
    let bytes: u64 = rows.iter().map(|r| r.s).sum();
    let newest = rows.iter().map(|r| r.m).filter(|m| *m != 0).max();
    let oldest = rows.iter().map(|r| r.m).filter(|m| *m != 0).min();
    let kinds: Vec<String> = kind_groups(rows)
        .into_iter()
        .map(|(word, _, n)| format!("{floor}{n} {}", if n == 1 || word == "text" { word.to_string() } else { format!("{word}s") }))
        .collect();
    filled(vec![
        ("Kinds", kinds.join(", ")),
        ("Combined", format!("{floor}{}", format::size(bytes))),
        ("Newest", newest.map(|m| format!("{floor}{}", format::date(m))).unwrap_or_default()),
        ("Oldest", oldest.map(|m| format!("{floor}{}", format::date(m))).unwrap_or_default()),
    ])
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod tests;

// The Quick Look overlay's state, ui/Preview.qml, and what its keys do, ui/js/PreviewKeys.js. The
// overlay draws one file over the listing, inset or expanded to fill it. Inset, Up and Down move the
// listing cursor and the preview follows it, settled at 120 ms so a held key decodes nothing in
// between; expanded, they scroll the surface. Left leaves an inset preview; Right and Space expand.
//
// One deliberate reading of the source over the prose: AGENTS.md says Space also brings the inset
// back, but PreviewKeys.act's "preview" case only ever expands, and that code is what this follows.
use crate::kinds::{self, Kind};

pub const FOLLOW_SETTLE_MS: u64 = 120;
// ui/Theme.qml preview.fraction: the inset surface's share of the listing area.
pub const INSET: f32 = 0.82;
// PreviewText.maxBytes: a file over this is not read into the window at all.
pub const TEXT_MAX_BYTES: u64 = 524_288_000;
// PreviewTable.maxRecords for the overlay, which is not the column's compact fourteen.
pub const TABLE_MAX_RECORDS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Archive {
    pub entries: u64,
    pub names: Vec<(String, bool)>,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    // Read the text, run --sheet, render through --preview-image, or ask the backend for the row's
    // archive index: each answer comes back carrying the path it was for.
    LoadText(String),
    LoadSheet(String),
    RenderImage(String),
    AskArchive(usize),
    // Stage 5b: a PDF opens on its first page, fitted; media starts playing.
    LoadPdf(String),
    OpenMedia(String),
    Turn(i32),
    Seek(i64),
    Mute,
    Settle(u64),
    // Move the listing cursor by one, which is the pane's job; the preview then follows.
    MoveCursor(isize),
    Scroll(isize),
    Closed,
}

#[derive(Debug, Default)]
pub struct Preview {
    pub active: bool,
    pub path: String,
    pub icon: String,
    pub size: u64,
    pub kind_name: String,
    pub kind: Option<Kind>,
    pub expanded: bool,
    pub row: usize,
    pub archive: Option<Archive>,
    pub archive_offset: usize,
    settle_gen: u64,
    settling: bool,
    pending: Option<(String, String, u64, String, usize)>,
}

impl Preview {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    // Space or Right on a file row: immediate, and any follow still settling is dropped.
    pub fn open(&mut self, path: String, icon: String, size: u64, kind_name: String, row: usize) -> Vec<Effect> {
        self.settling = false;
        self.pending = None;
        self.load(path, icon, size, kind_name, row)
    }

    // The held-key path: the first move loads at once and opens the window; any move inside it is
    // kept and only the last one loads when the window closes.
    pub fn follow(&mut self, path: String, icon: String, size: u64, kind_name: String, row: usize) -> Vec<Effect> {
        self.settle_gen += 1;
        let settle = Effect::Settle(self.settle_gen);
        if !self.settling {
            self.settling = true;
            self.pending = None;
            let mut out = self.load(path, icon, size, kind_name, row);
            out.push(settle);
            return out;
        }
        self.pending = Some((path, icon, size, kind_name, row));
        vec![settle]
    }

    pub fn settled(&mut self, generation: u64) -> Vec<Effect> {
        if generation != self.settle_gen {
            return vec![];
        }
        self.settling = false;
        match self.pending.take() {
            Some((p, i, s, k, r)) if self.active => self.load(p, i, s, k, r),
            _ => vec![],
        }
    }

    fn load(&mut self, path: String, icon: String, size: u64, kind_name: String, row: usize) -> Vec<Effect> {
        let kind = kinds::quick_look(&icon, &path);
        self.kind = Some(kind);
        self.active = true;
        self.archive = None;
        self.archive_offset = 0;
        self.row = row;
        (self.path, self.icon, self.size, self.kind_name) = (path.clone(), icon, size, kind_name);
        match kind {
            Kind::Text if kinds::is_sheet(&path) => vec![Effect::LoadSheet(path)],
            Kind::Text if size > TEXT_MAX_BYTES => vec![],
            Kind::Text => vec![Effect::LoadText(path)],
            Kind::Image => vec![Effect::RenderImage(path)],
            Kind::Archive => vec![Effect::AskArchive(row)],
            Kind::Pdf => vec![Effect::LoadPdf(path)],
            Kind::Video | Kind::Audio => vec![Effect::OpenMedia(path)],
            Kind::Unsupported => vec![],
        }
    }

    pub fn close(&mut self) -> Vec<Effect> {
        *self = Preview { settle_gen: self.settle_gen, ..Default::default() };
        vec![Effect::Closed]
    }

    // A meta answer is the archive's only when it is for the row still open.
    pub fn archive_answered(&mut self, row: usize, archive: Archive) {
        if self.active && self.kind == Some(Kind::Archive) && row == self.row && self.archive.is_none() {
            self.archive = Some(archive);
        }
    }

    // ui/Preview.qml status: the one sentence a surface with nothing to draw says. `failed` is the
    // surface's own report: a text that could not be read, a PDF poppler refused, media that failed.
    pub fn status(&self, failed: bool) -> Option<(&'static str, bool)> {
        match self.kind? {
            Kind::Unsupported => Some(("This file cannot be previewed.", false)),
            Kind::Archive if self.archive.as_ref().is_some_and(|a| a.failed) => Some(("This archive could not be read.", true)),
            Kind::Text if self.size > TEXT_MAX_BYTES => Some(("This file is too large to preview.", false)),
            Kind::Text if failed => Some(("This file could not be read.", false)),
            Kind::Pdf if failed => Some(("This file could not be read.", true)),
            Kind::Video | Kind::Audio if failed => Some(("This file could not be played.", true)),
            _ => None,
        }
    }

    pub fn scroll_archive(&mut self, steps: isize, shown: usize) {
        let names = self.archive.as_ref().map(|a| a.names.len()).unwrap_or(0);
        let top = names.saturating_sub(shown) as isize;
        self.archive_offset = (self.archive_offset as isize + steps).clamp(0, top.max(0)) as usize;
    }

    // PreviewKeys.act for the keys this stage's kinds answer.
    pub fn key(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Down if self.expanded => vec![Effect::Scroll(1)],
            Action::Up if self.expanded => vec![Effect::Scroll(-1)],
            Action::Down => vec![Effect::MoveCursor(1)],
            Action::Up => vec![Effect::MoveCursor(-1)],
            Action::Expand if !self.expanded => {
                self.expanded = true;
                vec![]
            }
            Action::Back if !self.expanded => self.close(),
            Action::Forward if !self.expanded => {
                self.expanded = true;
                vec![]
            }
            // Expanded, Left and Right move the content: a PDF's page, a media file's playhead.
            Action::Back | Action::Forward => {
                let step = if action == Action::Back { -1 } else { 1 };
                match self.kind {
                    Some(Kind::Pdf) => vec![Effect::Turn(step)],
                    Some(Kind::Video | Kind::Audio) => vec![Effect::Seek(step as i64 * crate::media::SEEK_MS)],
                    _ => vec![],
                }
            }
            // MediaMute rule 4: m mutes while media is open and keeps its listing meaning elsewhere.
            Action::Mute if matches!(self.kind, Some(Kind::Video | Kind::Audio)) => vec![Effect::Mute],
            Action::Escape => self.close(),
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Up,
    Down,
    Back,
    Forward,
    Expand,
    Escape,
    Mute,
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

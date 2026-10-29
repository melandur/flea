// The preview's half of the shell: every load runs off the UI thread and answers with the path it
// was for, so an answer that lands after the cursor moved on is dropped rather than drawn under the
// wrong name. Text goes into a read-only editor, whose buffer shapes only the lines on screen; a
// table is read by delimited.rs; an image is the PNG `flea --preview-image` rendered in its jail,
// because Qt choosing a decoder by content once ran Ghostscript unjailed (AGENTS.md "Module map").
use crate::delimited;
use crate::kinds::{self, Kind};
use crate::preview::{self, Action, Archive};
use crate::wire::{Meta, Request};
use crate::{backend, blocking, sleep, Flea, Message};
use iced::keyboard::{key::Named, Key};
use iced::widget::{image, text_editor};
use iced::Task;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Table {
    pub text: Vec<char>,
    pub delim: char,
    pub starts: Vec<usize>,
    pub widths: Vec<usize>,
    pub numeric: Vec<bool>,
}

impl Table {
    // PreviewTable: the sniffed delimiter (a sheet's CSV is always comma), the record starts, and a
    // width per column from the first records, capped at forty characters and 128 columns.
    pub fn read(body: &str, name: &str, sheet: bool) -> Table {
        let text: Vec<char> = body.chars().collect();
        let delim = if sheet { ',' } else { delimited::sniff(&text, name) };
        let starts = delimited::record_starts(&text, delim, preview::TABLE_MAX_RECORDS);
        let mut widths = delimited::column_chars(&text, &starts, delim, 40);
        widths.truncate(128);
        let numeric = delimited::numeric_columns(&text, &starts, delim);
        Table { text, delim, starts, widths, numeric }
    }

    pub fn records(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    pub fn row(&self, r: usize) -> Vec<String> {
        delimited::fields(&self.text, self.starts[r], self.starts[r + 1], self.delim)
    }
}

#[derive(Debug, Default)]
pub struct Surfaces {
    pub text: Option<(String, text_editor::Content)>,
    pub text_failed: bool,
    pub table: Option<(String, Table)>,
    pub image: Option<(String, Option<image::Handle>)>,
}

fn read_text(path: &str) -> Result<String, ()> {
    std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|_| ())
}

pub(crate) fn run_sheet(path: &str) -> Result<String, ()> {
    let out = std::process::Command::new(backend::binary())
        .args(["--sheet", path, &(preview::TABLE_MAX_RECORDS + 1).to_string()])
        .output()
        .map_err(|_| ())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err(()) }
}

fn render_image(path: &str, size: u32) -> Option<image::Handle> {
    let out = std::process::Command::new(backend::binary()).args(["--preview-image", path, &size.to_string()]).output().ok()?;
    let png = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !png.is_empty()).then(|| image::Handle::from_path(png))
}

impl Flea {
    // PreviewImage.renderSize: the window's larger side rounded up to 512, at most 4096.
    fn render_size(&self) -> u32 {
        let side = self.window_w.max(self.viewport_h).max(1.0);
        (512.0 * (side / 512.0).ceil()).min(4096.0) as u32
    }

    pub(crate) fn run_preview(&mut self, effects: Vec<preview::Effect>) -> Task<Message> {
        let mut tasks = vec![];
        for e in effects {
            tasks.push(match e {
                preview::Effect::LoadText(path) => {
                    self.surfaces = Surfaces::default();
                    self.clear_media();
                    let tabular = kinds::is_delimited(&path);
                    let key = path.clone();
                    Task::perform(
                        blocking(move || read_text(&path).map(|body| if tabular { Ok(Table::read(&body, &path, false)) } else { Err(body) })),
                        move |r| Message::PreviewLoaded(key.clone(), r),
                    )
                }
                preview::Effect::LoadSheet(path) => {
                    self.surfaces = Surfaces::default();
                    self.clear_media();
                    let key = path.clone();
                    Task::perform(blocking(move || run_sheet(&path).map(|csv| Ok(Table::read(&csv, &path, true)))), move |r| Message::PreviewLoaded(key.clone(), r))
                }
                preview::Effect::RenderImage(path) => {
                    self.clear_media();
                    let size = self.render_size();
                    let key = path.clone();
                    Task::perform(blocking(move || render_image(&path, size)), move |h| Message::PreviewImage(key.clone(), h))
                }
                preview::Effect::AskArchive(row) => {
                    self.surfaces = Surfaces::default();
                    self.clear_media();
                    let sent = self.backend.as_ref().map(|b| b.send(&Request::MetaArchive { row }));
                    if let Some(Err(e)) = sent {
                        self.say(e)
                    } else {
                        Task::none()
                    }
                }
                preview::Effect::Settle(generation) => {
                    Task::perform(sleep(Duration::from_millis(preview::FOLLOW_SETTLE_MS)), move |_| Message::PreviewSettled(generation))
                }
                preview::Effect::MoveCursor(delta) => {
                    let effects = self.pane.step(delta);
                    let moved = self.run(effects);
                    let follow = self.follow_cursor();
                    Task::batch([moved, follow])
                }
                preview::Effect::Scroll(steps) => self.scroll_preview(steps),
                preview::Effect::LoadPdf(path) => {
                    self.surfaces = Surfaces::default();
                    self.load_pdf(path)
                }
                preview::Effect::OpenMedia(path) => {
                    self.surfaces = Surfaces::default();
                    self.open_media(&path)
                }
                preview::Effect::Turn(delta) => self.pdf_turn(delta),
                preview::Effect::Seek(delta) => {
                    if let Some(m) = &self.playback.media {
                        m.seek(delta);
                    }
                    Task::none()
                }
                preview::Effect::Mute => {
                    self.toggle_mute();
                    Task::none()
                }
                preview::Effect::Closed => {
                    self.surfaces = Surfaces::default();
                    self.clear_media();
                    Task::none()
                }
            });
        }
        Task::batch(tasks)
    }

    fn cursor_file(&self) -> Option<(String, String, u64, String, usize)> {
        let i = self.pane.cursor;
        let r = self.pane.row_at(i).filter(|r| !r.d)?;
        let kind = self.pane.kinds.get(r.k).cloned().unwrap_or_default();
        Some((crate::pane::join(&self.pane.path, &r.n), r.i.clone(), r.s, kind, i))
    }

    // Space and Right on a file row, and Enter on an archive nobody claimed.
    pub(crate) fn open_preview(&mut self) -> Task<Message> {
        let Some((path, icon, size, kind, row)) = self.cursor_file() else { return Task::none() };
        let effects = self.preview.open(path, icon, size, kind, row);
        self.run_preview(effects)
    }

    fn follow_cursor(&mut self) -> Task<Message> {
        let Some((path, icon, size, kind, row)) = self.cursor_file() else { return Task::none() };
        let effects = self.preview.follow(path, icon, size, kind, row);
        self.run_preview(effects)
    }

    // Nav.openCursor: a claimed ending goes to the opener; an archive otherwise opens Flea's own view.
    pub(crate) fn archive_routes_here(&self, path: &str) -> bool {
        let Some(r) = self.pane.row_at(self.pane.cursor) else { return false };
        kinds::quick_look(&r.i, path) == Kind::Archive && !self.prefs.claimed(&r.n)
    }

    fn scroll_preview(&mut self, steps: isize) -> Task<Message> {
        if let Some((_, content)) = self.surfaces.text.as_mut() {
            content.perform(text_editor::Action::Scroll { lines: steps as i32 * 3 });
        }
        if self.preview.kind == Some(Kind::Archive) {
            let shown = ((self.viewport_h / crate::ROW_H) as usize).max(1);
            self.preview.scroll_archive(steps, shown);
        }
        if self.pdf.is_some() {
            let y = steps as f32 * crate::ROW_H;
            return iced::widget::operation::scroll_by(crate::mediashell::PDF_ID, iced::widget::scrollable::AbsoluteOffset { x: 0.0, y });
        }
        if self.surfaces.table.is_some() {
            let y = steps as f32 * crate::ROW_H * 3.0;
            return iced::widget::operation::scroll_by(crate::previewview::TABLE_ID, iced::widget::scrollable::AbsoluteOffset { x: 0.0, y });
        }
        Task::none()
    }

    pub(crate) fn preview_loaded(&mut self, path: String, result: Result<Result<Table, String>, ()>) -> Task<Message> {
        if !self.preview.active || path != self.preview.path {
            return Task::none();
        }
        match result {
            Err(()) => self.surfaces.text_failed = true,
            Ok(Ok(table)) => self.surfaces.table = Some((path, table)),
            Ok(Err(body)) => self.surfaces.text = Some((path, text_editor::Content::with_text(&body))),
        }
        Task::none()
    }

    pub(crate) fn preview_image(&mut self, path: String, handle: Option<image::Handle>) -> Task<Message> {
        if self.preview.active && path == self.preview.path {
            self.surfaces.image = Some((path, handle));
        }
        Task::none()
    }

    // A meta line with no token is the overlay's archive index; one with a token is the column's.
    pub(crate) fn meta_answered(&mut self, meta: Meta) {
        if meta.token == 0 {
            self.preview.archive_answered(meta.row, Archive { entries: meta.entries, names: meta.names, failed: meta.archive_failed });
        } else {
            self.column_meta(meta);
        }
    }

    // The preview context's keys; everything else is not read while the overlay is up.
    pub(crate) fn preview_key(&mut self, key: &Key) -> Option<Task<Message>> {
        if !self.preview.active {
            return None;
        }
        let action = match key.as_ref() {
            Key::Named(Named::ArrowUp) => Action::Up,
            Key::Named(Named::ArrowDown) => Action::Down,
            Key::Named(Named::ArrowLeft) => Action::Back,
            Key::Named(Named::ArrowRight) => Action::Forward,
            Key::Named(Named::Space) => Action::Expand,
            Key::Named(Named::Escape) => Action::Escape,
            Key::Character("m") => Action::Mute,
            _ => return Some(Task::none()),
        };
        let effects = self.preview.key(action);
        Some(self.run_preview(effects))
    }
}

// The columns view's last pane when the cursor is on a file, ui/PreviewColumn.qml: one anatomy for
// every state, a 16 by 10 frame, the name, and the caption-type table of facts under it. The frame
// is the row's thumbnail decoded at its own size, the file's first lines for text and code, the first
// records for a table, an archive's first names, a stack of marks for a selection, or the kind's mark
// with the one line saying why it stands in. colpreview.rs decides what is loaded; this runs it.
use crate::colpreview::{ColPreview, Out};
use crate::columnsview::Msg;
use crate::facts::{self, State};
use crate::kinds::{is_sheet, is_tabular};
use crate::previewshell::{run_sheet, Table};
use crate::wire::Meta;
use crate::{blocking, marks, sleep, Flea, Message, PAD_X, TEXT_PX};
use iced::widget::{column, container, image, row, stack, svg, text, Column};
use iced::{Border, Color, Element, Length, Task};
use std::time::Duration;

// PreviewLines: a file over a mebibyte is refused rather than truncated, and fourteen lines fill the frame.
const MAX_BYTES: u64 = 1_048_576;
const MAX_LINES: usize = 14;
// Theme: the caption, its line box at lineBoxRatio 1.8, the pane-state mark, the gap.
const CAPTION: f32 = 11.0;
const CAPTION_LINE: f32 = 20.0;
const STATE_MARK: f32 = 40.0;
const GAP: f32 = 8.0;
// FactsTable's labelWidth: the advance of "Points at " in the caption face.
const LABEL_W: f32 = 66.0;
// A freedesktop thumbnail is at most 256 px on its long side, so this is its own size.
const FRAME_PX: u32 = 256;
// LoadingState's hold-off: a local read never flashes the crawl.
const HOLD_OFF_MS: u64 = 150;

#[derive(Debug, Clone)]
pub enum Surface {
    Lines { lines: Vec<String>, empty: bool },
    Table(Table),
    Failed,
}

#[derive(Debug, Clone)]
pub enum Frame {
    Decoding,
    Ready(image::Handle),
    Failed,
}

#[derive(Debug, Default)]
pub struct ColumnShell {
    pub model: ColPreview,
    pub surface: Option<(String, Surface)>,
    pub frame: Option<(String, Frame)>,
    // The token whose meta has been in flight past the hold-off.
    armed: u64,
}

fn read_lines(path: &str, sheet: bool) -> Surface {
    if sheet {
        return run_sheet(path).map(|csv| Surface::Table(Table::read(&csv, path, true))).unwrap_or(Surface::Failed);
    }
    let Ok(bytes) = std::fs::read(path) else { return Surface::Failed };
    let body = String::from_utf8_lossy(&bytes);
    if is_tabular(path) {
        return Surface::Table(Table::read(&body, path, false));
    }
    Surface::Lines { lines: body.split('\n').take(MAX_LINES).map(String::from).collect(), empty: body.is_empty() }
}

impl Flea {
    // The column on screen: the columns view, the setting on, and the cursor on a file.
    fn preview_shown(&self) -> bool {
        self.columns_mode() && self.prefs.preview_column && self.pane.row_at(self.pane.cursor).is_some_and(|r| !r.d)
    }

    // Run after every input with columns_follow: the load, and the frame's decode once the thumb lands.
    pub(crate) fn column_preview_follow(&mut self) -> Task<Message> {
        let readable = self.preview_shown() && !self.pane.list_in_flight();
        let outs = self.colprev.model.follow(&self.pane, readable, self.prefs.preview_automatic);
        let mut tasks = vec![self.column_outs(outs)];
        let file = self.colprev.model.loaded.as_ref().and_then(|l| self.pane.thumb_file(l.index)).map(String::from);
        match file {
            Some(f) if self.colprev.frame.as_ref().map(|(k, _)| k) != Some(&f) => {
                self.colprev.frame = Some((f.clone(), Frame::Decoding));
                let key = f.clone();
                tasks.push(Task::perform(blocking(move || crate::decode(&f, FRAME_PX)), move |h| Message::Column(Msg::Frame(key.clone(), h))));
            }
            None => self.colprev.frame = None,
            _ => {}
        }
        Task::batch(tasks)
    }

    fn column_outs(&mut self, outs: Vec<Out>) -> Task<Message> {
        let mut tasks = vec![];
        for out in outs {
            match out {
                Out::Settle(generation) => {
                    tasks.push(Task::perform(sleep(Duration::from_millis(crate::colpreview::SETTLE_MS)), move |_| Message::Column(Msg::Settled(generation))))
                }
                Out::Send(request) => {
                    if let crate::wire::Request::Meta { token, .. } = request {
                        tasks.push(Task::perform(sleep(Duration::from_millis(HOLD_OFF_MS)), move |_| Message::Column(Msg::Armed(token))));
                    }
                    tasks.push(self.run(vec![crate::pane::Effect::Send(request)]));
                }
                Out::Read(path, sheet) => {
                    let size = self.colprev.model.loaded.as_ref().map_or(0, |l| l.row.s);
                    self.colprev.surface = None;
                    // The sheet reader stops at the rows the frame shows, so only a raw read is gated.
                    if sheet || size <= MAX_BYTES {
                        let key = path.clone();
                        tasks.push(Task::perform(blocking(move || read_lines(&path, sheet)), move |s| Message::Column(Msg::Read(key.clone(), s))));
                    }
                }
            }
        }
        Task::batch(tasks)
    }

    pub(crate) fn column_preview_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Settled(generation) => {
                let outs = self.colprev.model.settled(generation, &mut self.pane, self.prefs.preview_automatic);
                self.column_outs(outs)
            }
            Msg::Read(path, surface) => {
                if self.colprev.model.loaded.as_ref().is_some_and(|l| l.path == path) {
                    self.colprev.surface = Some((path, surface));
                }
                Task::none()
            }
            Msg::Frame(file, handle) => {
                if let Some((_, frame)) = self.colprev.frame.as_mut().filter(|(k, _)| *k == file) {
                    *frame = handle.map_or(Frame::Failed, Frame::Ready);
                }
                Task::none()
            }
            Msg::Armed(token) => {
                self.colprev.armed = token;
                Task::none()
            }
            Msg::Tap(..) | Msg::Menu(..) => Task::none(),
        }
    }

    pub(crate) fn column_meta(&mut self, meta: Meta) {
        self.colprev.model.meta_answered(meta, &self.pane);
    }

    pub(crate) fn preview_column<'a>(&'a self, width: f32) -> Element<'a, Message> {
        let p = self.palette;
        let c = &self.colprev;
        let loaded = c.model.loaded.as_ref();
        let surface = loaded.and_then(|l| c.surface.as_ref().filter(|(k, _)| *k == l.path)).map(|(_, s)| s);
        let row_state = c.model.row_state();
        let lined = matches!(row_state, Some(State::Text | State::Code));
        let too_large = lined && loaded.is_some_and(|l| !is_sheet(&l.row.n) && l.row.s > MAX_BYTES);
        let meta = c.model.meta.as_ref();
        let failure = if lined && matches!(surface, Some(Surface::Failed)) {
            "This file could not be read."
        } else if meta.is_some_and(|m| m.archive_failed) {
            "This archive could not be read."
        } else {
            ""
        };
        let selection = loaded.map_or(0, |l| l.selection);
        let kind_name = loaded.map_or("", |l| l.kind_name.as_str());
        let st = facts::state(loaded.map(|l| &l.row), selection, c.model.busy(), failure, kind_name);
        let selected: Vec<&crate::wire::Row> = loaded.map(|l| l.selected.iter().collect()).unwrap_or_default();
        let fact_rows = match (st, loaded) {
            (State::Multi, _) => facts::multi_facts(&selected, selection),
            (_, Some(l)) => facts::facts(st, &l.row, meta, kind_name, 0),
            _ => vec![],
        };
        // The thumbnail is wanted wherever the state draws a picture of the file itself.
        let wants_thumb = !matches!(st, State::Loading | State::Pdf | State::Multi);
        let thumb = c.frame.as_ref().map(|(_, f)| f);
        let thumb_drawn = matches!(thumb, Some(Frame::Decoding | Frame::Ready(_)));
        let thumb_shown = wants_thumb && thumb_drawn;
        let blank = too_large || matches!(surface, Some(Surface::Lines { empty: true, .. })) || matches!(surface, Some(Surface::Table(t)) if t.records() == 0);
        let multi_marks = if st == State::Multi { facts::multi_marks(&selected) } else { vec![] };
        let glyph_state = match st {
            State::Error | State::Unsupported | State::Loading => false,
            State::Text | State::Code => blank,
            State::Archive => meta.is_none(),
            State::Multi => multi_marks.is_empty(),
            _ => true,
        };
        let pictures = matches!(st, State::Image | State::Video);
        let note = if wants_thumb && thumb.is_some() && !thumb_drawn {
            "thumbnail unavailable"
        } else if wants_thumb && c.model.no_thumb_coming(&self.pane) && pictures && !thumb_drawn {
            "no preview could be made"
        } else if matches!(st, State::Text | State::Code) && too_large {
            "too large to preview"
        } else {
            ""
        };
        let inner = width - 2.0 * PAD_X;
        let frame_h = (inner * 10.0 / 16.0).round();
        let mut layers: Vec<Element<'a, Message>> = vec![];
        if let (true, Some(Frame::Ready(h))) = (thumb_shown, thumb) {
            layers.push(container(image(h.clone()).content_fit(iced::ContentFit::Contain).width(Length::Fill).height(Length::Fill)).padding(1).into());
        }
        if !thumb_shown && glyph_state {
            let mark = match (st, loaded) {
                (State::Symlink, _) => "symlink",
                (State::Multi, _) | (_, None) => "file",
                (_, Some(l)) => marks::glyph_for_row(&l.row.i, l.row.p, &l.row.n, l.row.d, self.pane.file_type_colours),
            };
            let ink = if st == State::Symlink { p.symlink } else { p.muted };
            let mut col = column![svg(marks::handle(mark, Color { a: 0.7, ..ink })).width(STATE_MARK).height(STATE_MARK)].spacing(GAP).align_x(iced::Center);
            if !note.is_empty() {
                col = col.push(text(note).size(CAPTION).color(p.muted));
            }
            layers.push(container(col).center(Length::Fill).into());
        }
        if !multi_marks.is_empty() {
            layers.push(container(kind_stack(&multi_marks, p.muted)).center(Length::Fill).into());
        }
        if matches!(st, State::Text | State::Code) {
            if let Some(s) = surface {
                layers.push(lines_layer(s, st == State::Code, p));
            }
        }
        if let (State::Archive, Some(m)) = (st, meta) {
            layers.push(archive_layer(m, frame_h - 2.0 * GAP, p));
        }
        if st == State::Loading && c.armed == c.model.pending() && c.armed != 0 {
            layers.push(container(text("reading").size(CAPTION).color(p.muted)).center(Length::Fill).into());
        }
        if matches!(st, State::Error | State::Unsupported) {
            let (mark, ink, words, words_ink) =
                if st == State::Error { ("alert", p.error, failure, p.foreground) } else { ("file", p.muted, "no preview", p.muted) };
            layers.push(
                container(column![svg(marks::handle(mark, ink)).width(STATE_MARK).height(STATE_MARK), text(words).size(CAPTION).color(words_ink)].spacing(GAP).align_x(iced::Center))
                    .center(Length::Fill)
                    .into(),
            );
        }
        let frame = container(stack(layers).width(Length::Fill).height(Length::Fill)).width(Length::Fill).height(frame_h).clip(true).style(move |_| {
            container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() })
        });
        let name = match (st, loaded) {
            (State::Multi, _) => format!("{selection} {}", if selection == 1 { "item selected" } else { "items selected" }),
            (_, Some(l)) => l.row.n.clone(),
            _ => String::new(),
        };
        let mut table = Column::new().spacing(3);
        for (label, value) in fact_rows {
            table = table.push(
                row![text(label).size(CAPTION).color(p.muted).width(LABEL_W), text(value).size(CAPTION).color(p.foreground).wrapping(text::Wrapping::None)]
                    .height((CAPTION * 1.5).round()),
            );
        }
        container(column![frame, text(name).size(TEXT_PX).color(p.foreground).wrapping(text::Wrapping::None), table].spacing(GAP))
            .padding(PAD_X)
            .width(width)
            .height(Length::Fill)
            .clip(true)
            .into()
    }
}

// KindStack: one mark per kind, front-most first, each a quarter of the mark across and an eleventh
// down from the one in front of it, and a step fainter.
fn kind_stack<'a>(marks_: &[&'static str], ink: Color) -> Element<'a, Message> {
    let steps = marks_.len().saturating_sub(1) as f32;
    let (sx, sy) = ((STATE_MARK / 4.0).round(), (STATE_MARK / 11.0).round());
    // Painted back to front, so the front mark comes last.
    let layers: Vec<Element<'a, Message>> = marks_
        .iter()
        .enumerate()
        .rev()
        .map(|(i, m)| {
            let ink = Color { a: 1.0 - i as f32 * 0.3, ..ink };
            container(svg(marks::handle(m, ink)).width(STATE_MARK).height(STATE_MARK))
                .padding(iced::Padding { left: (steps - i as f32) * sx, top: i as f32 * sy, ..Default::default() })
                .into()
        })
        .collect();
    stack(layers).width(STATE_MARK + steps * sx).height(STATE_MARK + steps * sy).into()
}

// PreviewLines: verbatim for text, a muted number gutter for code, the first records for a table.
fn lines_layer<'a>(s: &Surface, numbered: bool, p: crate::theme::Palette) -> Element<'a, Message> {
    let mut col = Column::new();
    match s {
        Surface::Lines { lines, .. } => {
            for (i, l) in lines.iter().enumerate() {
                let mut line = row![];
                if numbered {
                    line = line.push(text((i + 1).to_string()).size(CAPTION).color(Color { a: 0.6, ..p.muted }).width(16.0).align_x(iced::Right));
                }
                line = line.push(text(format!("  {l}")).size(CAPTION).color(p.foreground).wrapping(text::Wrapping::None));
                col = col.push(line);
            }
        }
        Surface::Table(t) => {
            for r in 0..t.records().min(MAX_LINES) {
                let mut line = row![].spacing(GAP);
                for (c, cell) in t.row(r).into_iter().enumerate().take(t.widths.len().max(1)) {
                    let w = t.widths.get(c).copied().unwrap_or(1).clamp(1, 40) as f32 * CAPTION * 0.6;
                    line = line.push(text(cell).size(CAPTION).color(if r == 0 { p.muted } else { p.foreground }).width(w).wrapping(text::Wrapping::None));
                }
                col = col.push(line);
            }
        }
        Surface::Failed => {}
    }
    container(col).padding(GAP).width(Length::Fill).height(Length::Fill).clip(true).into()
}

// PreviewArchive: as many of the wire's names as the frame has room for, and the count it could not name.
fn archive_layer<'a>(m: &Meta, height: f32, p: crate::theme::Palette) -> Element<'a, Message> {
    let shown = ((height / CAPTION_LINE).floor() as usize).saturating_sub(1);
    let mut col = Column::new();
    for (n, d) in m.names.iter().take(shown) {
        col = col.push(
            row![svg(marks::handle(if *d { "folder" } else { "file" }, p.muted)).width(CAPTION).height(CAPTION), text(n.clone()).size(CAPTION).color(p.muted).wrapping(text::Wrapping::None)]
                .spacing(GAP)
                .height(CAPTION_LINE)
                .align_y(iced::Center),
        );
    }
    let more = m.entries.saturating_sub(m.names.len().min(shown) as u64);
    if more > 0 {
        col = col.push(text(format!("+ {more} more")).size(CAPTION).color(Color { a: 0.6, ..p.muted }).height(CAPTION_LINE));
    }
    container(col).padding(GAP).width(Length::Fill).height(Length::Fill).clip(true).into()
}

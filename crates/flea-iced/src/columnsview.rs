// The columns view as ui/ColumnsArea.qml, ui/ColumnPane.qml and ui/ColumnRow.qml draw it: a third
// of the pane for the parent, a third for the pane's own listing, and the rest for the cursor row's
// folder. The parent's row for the folder on screen is the trail, lifted and never accented; rows of
// an ancestor off the trail read muted; only the middle column takes the accent, draws a size, and is
// the pane's own scrollable, so its viewport is the one thumbnails and the held window follow.
use crate::columns::{child_path, parent_path, tapped_neighbour};
use crate::view::PaneView;
use crate::wire::Row;
use crate::{format, list_id, marks, Flea, Message, MARK, PAD_X, RENAME_ID, TEXT_PX};
use iced::widget::{column, container, mouse_area, row, scrollable, space, svg, text, text_input, Column};
use iced::{Color, Element, Length, Task};

#[derive(Debug, Clone)]
pub enum Msg {
    // A tap on a neighbour's row: 0 the parent, 2 the child; the row's index in that column.
    Tap(u8, usize),
    Menu(u8, usize),
    // The preview column's own: its settle, its frame's reader and decode, and the loading hold-off.
    Settled(u64),
    Read(String, crate::columnpreview::Surface),
    Frame(String, Option<iced::widget::image::Handle>),
    Armed(u64),
    // columnmedia.rs: the PDF's page and toolbar, and the media strip.
    PdfRendered(Result<crate::pdf::Page, ()>),
    PdfTurn(i32),
    PdfZoom(i32),
    PdfExpand,
    Play,
    Scrub(f32),
    ScrubDone,
    Mute,
}

impl Flea {
    pub(crate) fn columns_mode(&self) -> bool {
        self.other.is_none() && self.dual_from == "columns"
    }

    // Run after every input: both neighbours asked, and the neighbour menu raised once its row landed.
    pub(crate) fn columns_follow(&mut self) -> Task<Message> {
        let mut tasks = vec![];
        if self.columns_mode() {
            let asks = self.columns.follow(&self.pane);
            tasks.push(self.run(asks.into_iter().map(crate::pane::Effect::Send).collect()));
        }
        // menuOnNeighbour: Nav.applyPendingSelect opens the menu once the cursor is on its row.
        let ready = self.pending_menu.as_ref().is_some_and(|(dir, name)| {
            self.pane.path == *dir && !self.pane.list_in_flight() && self.pane.row_at(self.pane.cursor).is_some_and(|r| r.n == *name)
        });
        if ready {
            self.pending_menu = None;
            tasks.push(self.open_cursor_menu());
        }
        tasks.push(self.column_preview_follow());
        Task::batch(tasks)
    }

    pub(crate) fn column_message(&mut self, msg: Msg) -> Task<Message> {
        let (which, index) = match msg {
            Msg::Tap(w, i) | Msg::Menu(w, i) => (w, i),
            _ => return self.column_preview_message(msg),
        };
        let base = if which == 0 { parent_path(&self.pane.path) } else { child_path(&self.pane) };
        let Some(r) = self.columns.rows(&base).get(index).cloned() else { return Task::none() };
        if let Msg::Menu(..) = msg {
            self.pending_menu = Some((base.clone(), r.n.clone()));
            let effects = self.pane.open(base, Some((r.n, 0)));
            return self.run(effects);
        }
        // The pointer's tap count, taken the way keys.rs takes it for the listing's rows.
        let now = std::time::Instant::now();
        let key = 1_000_000 * (which as usize + 1) + index;
        let taps = match self.last_tap {
            Some((i, at)) if i == key && now.duration_since(at) < crate::DOUBLE_TAP => 2,
            _ => 1,
        };
        self.last_tap = if taps == 1 { Some((key, now)) } else { None };
        match tapped_neighbour(&base, &r, taps) {
            Some(effect) => self.run(vec![effect]),
            None => Task::none(),
        }
    }

    pub(crate) fn columns_view<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        let width = self.pane_width();
        let third = (width / 3.0).floor();
        let parent = parent_path(&v.pane.path);
        let child = child_path(v.pane);
        let trail = v.pane.path.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
        let left = self.peek_column(0, &parent, Some(trail), third);
        let cursor_file = v.pane.row_at(v.pane.cursor).is_some_and(|r| !r.d);
        let right: Element<'a, Message> = if !child.is_empty() {
            self.peek_column(2, &child, None, width - 2.0 * third)
        } else if cursor_file && self.prefs.preview_column {
            self.preview_column(width - 2.0 * third)
        } else {
            space().width(Length::Fill).into()
        };
        row![left, container(self.middle_column(v)).width(third), right].height(Length::Fill).into()
    }

    fn peek_column<'a>(&'a self, which: u8, path: &str, trail: Option<String>, width: f32) -> Element<'a, Message> {
        let p = self.palette;
        if self.columns.draws_empty(path) {
            return container(text("This folder is empty.").color(p.muted)).center(Length::Fill).width(width).into();
        }
        let mut col = Column::new();
        for (i, r) in self.columns.rows(path).iter().enumerate() {
            let lifted = trail.as_deref() == Some(r.n.as_str());
            // An ancestor column that is not on the trail reads back, so its text drops to muted.
            let ink = if which == 0 && !lifted { p.muted } else { p.name_ink(r.d, r.p) };
            let line = self.column_line(r, ink, lifted, false, None, false);
            col = col.push(mouse_area(line).on_press(Message::Column(Msg::Tap(which, i))).on_right_press(Message::Column(Msg::Menu(which, i))));
        }
        container(scrollable(col.width(Length::Fill)).height(Length::Fill)).width(width).into()
    }

    // ui/ColumnRow.qml: the mark, the name, a size in the pane's own column, a chevron on a folder.
    fn column_line<'a>(&'a self, r: &Row, ink: Color, lifted: bool, cursor: bool, size: Option<String>, selected: bool) -> Element<'a, Message> {
        let p = self.palette;
        let mark_ink = if cursor { p.accent } else { p.chosen_mark(r.d, r.p).unwrap_or(p.muted) };
        let mut line = row![
            svg(marks::handle(marks::glyph_for_row(&r.i, r.p, &r.n, r.d, false), mark_ink)).width(MARK).height(MARK),
            text(r.n.clone()).color(ink).width(Length::Fill).wrapping(text::Wrapping::None),
        ]
        .spacing(8)
        .align_y(iced::Center);
        if let Some(s) = size {
            line = line.push(text(s).size(11.0).color(ink));
        }
        if r.d {
            line = line.push(svg(marks::handle("chevron-right", mark_ink)).width(11.0).height(11.0));
        }
        container(line).padding([0.0, PAD_X]).height(self.file_h()).width(Length::Fill).align_y(iced::Center).clip(true).style(move |_| {
            if cursor {
                container::Style::default().background(Color { a: 0.18, ..p.accent })
            } else if selected {
                container::Style::default().background(p.selection)
            } else if lifted {
                container::Style::default().background(Color { a: 0.08, ..p.foreground })
            } else {
                container::Style::default()
            }
        })
        .into()
    }

    // The pane's own listing, drawn the way the list draws it so rule 2 and the viewport hold here too.
    fn middle_column<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        let p = self.palette;
        let pane = v.pane;
        let total = pane.view_total();
        let first = ((v.scroll_y / self.file_h()).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / self.file_h()).ceil() as usize + 1).min(total - first);
        let mut col = column![space().height(first as f32 * self.file_h())];
        for at in first..first + shown {
            let Some(i) = pane.at_view(at) else { continue };
            let Some(r) = pane.row_at(i) else {
                col = col.push(space().height(self.file_h()));
                continue;
            };
            let cursor = i == pane.cursor && v.focused;
            let line: Element<'a, Message> = match pane.renaming.as_ref().filter(|e| e.index == i) {
                Some(e) => container(text_input("Name", &e.draft).id(iced::widget::Id::new(RENAME_ID)).on_input_maybe(e.pending.is_none().then_some(Message::RenameInput)).on_submit(Message::RenameSubmit).size(TEXT_PX))
                    .padding([0.0, PAD_X])
                    .height(self.file_h())
                    .align_y(iced::Center)
                    .into(),
                None => {
                    // ColumnRow.sizeText: a link says so, a folder's own size is not walked here.
                    let size = if format::is_symlink(r.p) { "link".to_string() } else if r.d { String::new() } else { format::size(r.s) };
                    let ink = if cursor { p.accent } else { p.name_ink(r.d, r.p) };
                    // ColumnRow's drop target: the frame, and the board's words where the size sits.
                    match v.drop.filter(|d| d.0 == i) {
                        Some((_, copy)) => {
                            let line = self.column_line(r, ink, false, cursor, Some(crate::dragdrop::label(copy).to_string()), false);
                            container(line).style(move |_| crate::dragshell::drop_frame(p.accent)).into()
                        }
                        None => self.column_line(r, ink, false, cursor, Some(size), pane.selection.contains(&i) || (i == pane.cursor && !v.focused)),
                    }
                }
            };
            let area = mouse_area(line).on_press(Message::RowPressed(v.side, i)).on_release(Message::RowReleased(v.side, i)).on_right_press(Message::Menu(crate::menushell::Msg::Row(v.side, i)));
            col = col.push(self.drag_target(area, crate::dragdrop::Target::Row(v.side, i)));
        }
        col = col.push(space().height((total - first - shown) as f32 * self.file_h()));
        let side = v.side;
        let list = scrollable(col.width(Length::Fill)).id(list_id(side)).on_scroll(move |vp| Message::Scrolled(side, vp)).height(Length::Fill);
        // Only the middle column owns this directory, so it alone is the floor a drop lands on.
        let layer = iced::widget::stack![list, self.band_over(v, 1, None, self.file_h(), false)];
        self.drag_target(mouse_area(layer), crate::dragdrop::Target::Floor(v.side)).into()
    }
}

// What the window draws: the chrome, the rail, one or two panes of header and listing, and the
// status bar. The shell's state and its update live elsewhere; nothing here changes either.
use crate::pane::Pane;
use crate::listcols::{self, Cols};
use crate::{contrast, format, list_id, marks, wire, Flea, Message, DATE_W, ICON_SLOT, ITEMS_W, KIND_W, MARK, MODE_W, PAD_X, RENAME_ID, ROW_H, SIZE_W, TEXT_PX};
use iced::widget::{column, container, image, mouse_area, row, scrollable, space, stack, svg, text, text_input, Id};
use iced::{Background, Border, Color, Element, Length};

// One pane as the view draws it: which side it sits on, where its listing is scrolled, and whether
// it holds the keyboard, which is what decides between the accent and the plain cursor fill.
#[derive(Clone, Copy)]
pub(crate) struct PaneView<'a> {
    pub(crate) pane: &'a Pane,
    pub(crate) side: u8,
    pub(crate) scroll_y: f32,
    pub(crate) focused: bool,
    // The row of this pane a drag in flight would drop on, and whether it would copy there.
    pub(crate) drop: Option<(usize, bool)>,
}

impl Flea {
    // Theme.fileRowHeight at the stored density: the list's, the columns view's and the Trash's rows.
    pub(crate) fn file_h(&self) -> f32 {
        listcols::row_height(&self.prefs.density)
    }

    // The columns a row of this pane draws; a search keeps Size and drops the others, as Row.qml does.
    pub(crate) fn list_cols(&self, searching: bool) -> Cols {
        let hidden = listcols::hidden(&self.prefs.columns);
        let width = self.pane_width();
        let mut cols = if self.other.is_some() { listcols::dual_set(width, &hidden) } else { listcols::set(width, &hidden) };
        if searching {
            cols = Cols { size: cols.size, ..Cols::default() };
        }
        cols
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let drop = self.drop_row();
        let on = |side: u8| drop.filter(|d| d.0 == side).map(|d| (d.1, d.2));
        let mine = PaneView { pane: &self.pane, side: self.side, scroll_y: self.scroll_y, focused: true, drop: on(self.side) };
        let column_of_mine = self.pane_column(mine);
        let panes: Element<'_, Message> = match &self.other {
            None => column_of_mine,
            Some(o) => {
                let theirs = self.pane_column(PaneView { pane: &o.pane, side: o.side, scroll_y: o.scroll_y, focused: false, drop: on(o.side) });
                // The primary is always the left one, whichever holds the keyboard.
                let (left, right) = if self.side == 0 { (column_of_mine, theirs) } else { (theirs, column_of_mine) };
                let rule = container(space().width(1).height(Length::Fill)).style(move |_| container::Style::default().background(p.surface));
                row![container(left).width(Length::FillPortion(1)), rule, container(right).width(Length::FillPortion(1))].into()
            }
        };
        let main: Element<'_, Message> = if self.rail.shown { row![self.rail_view(), panes].into() } else { panes };
        let mut stacked = column![self.chrome_view()];
        if let Some(bar) = self.tab_bar() {
            stacked = stacked.push(bar);
        }
        let window = container(stacked.push(main).push(self.status_view()))
            .style(move |_| container::Style::default().background(p.background).color(p.foreground))
            .width(Length::Fill)
            .height(Length::Fill);
        self.dialog_layer(self.sheet_layer(self.panel_layer(self.job_layer(self.action_layer(self.menu_layer(window.into()))))))
    }

    fn status_view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let mut count = if self.pane.total == 1 { "1 item".to_string() } else { format!("{} items", format::count(self.pane.total)) };
        if !self.pane.selection.is_empty() {
            count += &format!(" · {} selected", format::count(self.pane.selection.len()));
        }
        // A transient message wins the slot; the running transfer's sticky line is under it.
        let (note, ink) = match (&self.message, self.drag_line()) {
            (Some((t, _, alert)), _) => (t.clone(), if *alert { p.error } else { p.foreground }),
            // FileDrag.say: the drag's own line while it is up, under a transient one.
            (None, Some(line)) => (line, p.foreground),
            (None, None) if self.pane.search_listed() && self.pane.search.mode == crate::search::Mode::Results => (self.pane.search_status(), p.foreground),
            (None, None) => (self.pane.sticky.clone(), p.foreground),
        };
        // The query line takes the path's place while it is being typed in; a filter's stays while it stands.
        let left: Element<'_, Message> = if self.pane.filter_typing || !self.pane.filter_query.is_empty() {
            let caret = if self.pane.filter_typing { "▏" } else { "" };
            let hint = if self.pane.filter_typing { "" } else { "   esc clears" };
            row![
                text("Filter").color(p.muted),
                text(format!("{}{caret}", self.pane.filter_query)).color(p.foreground),
                text(self.pane.filter_summary()).color(p.muted),
                text(hint).color(p.muted),
            ]
            .spacing(10)
            .width(Length::Fill)
            .into()
        } else if self.pane.search.mode == crate::search::Mode::Typing {
            let s = &self.pane.search;
            let scope = format!("Search {}{}", self.pane.path, if s.deep { " and subfolders" } else { "" });
            row![text(scope).color(p.muted), text(format!("{}▏", s.query)).color(p.foreground)].spacing(10).width(Length::Fill).into()
        } else {
            text(self.pane.path.clone()).width(Length::Fill).wrapping(text::Wrapping::None).into()
        };
        let status = row![left, text(note).color(ink), text(count)].spacing(16).padding([0.0, PAD_X]).height(ROW_H).align_y(iced::Center);
        container(status).style(move |_| container::Style::default().color(p.muted).background(p.surface)).into()
    }

    fn pane_column<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        // The Trash view covers the focused pane whole, header and listing alike.
        if v.focused && self.trash.opened {
            return self.trash_view();
        }
        let p = self.palette;
        // ui/Header.qml: a click sorts by that column, and the order in force carries its mark.
        let order = &v.pane.order;
        let side = v.side;
        let head = |label: &'static str, key: &'static str| {
            let mark = if order.by == key { if order.desc { " ▼" } else { " ▲" } } else { "" };
            mouse_area(text(format!("{label}{mark}"))).on_press(Message::SortColumn(side, key))
        };
        // The header heads exactly the columns its rows draw, from the same width; Mode sorts nothing.
        let cols = self.list_cols(v.pane.search_listed());
        let mut header = row![container(head("Name", "name")).width(Length::Fill)];
        if cols.mode {
            header = header.push(container(text("Mode")).width(MODE_W));
        }
        if cols.size {
            header = header.push(container(head("Size", "size")).width(SIZE_W));
        }
        // Nothing sorts by a count, so Items is the one head with no press (Header.qml).
        if cols.items {
            header = header.push(container(text("Items")).width(ITEMS_W));
        }
        if cols.date {
            header = header.push(container(head("Modified", "mtime")).width(DATE_W));
        }
        if cols.kind {
            header = header.push(container(head("Kind", "kind")).width(KIND_W));
        }
        let header = header.spacing(listcols::GAP).padding([0.0, PAD_X]).height(ROW_H).align_y(iced::Center);
        // A right click on the band is the header menu: the columns to show and the dotfile toggle.
        let header = mouse_area(container(header).style(move |_| container::Style::default().color(p.muted)).width(Length::Fill))
            .on_right_press(Message::Menu(crate::menushell::Msg::Header(side)));
        let body: Element<'a, Message> = match (&self.share_browser, v.focused) {
            (Some(b), true) => self.share_browser_view(b),
            _ if v.pane.error.is_some() => container(text(v.pane.error.clone().unwrap_or_default()).color(p.muted)).center(Length::Fill).into(),
            // PaneStates' tip under an empty folder while key hints are on. QML's own sentence names
            // Ctrl+Shift+N, which nothing has bound since New Folder moved to Ctrl+N on 2026-09-18.
            _ if v.pane.listed_once && v.pane.total == 0 => {
                let mut tile = column![text("This folder is empty.").color(p.muted)].spacing(6).align_x(iced::Center);
                if self.prefs.key_hints {
                    tile = tile.push(text("Press Ctrl+N for a new folder.").size(11.0).color(p.muted));
                }
                self.background(v, self.floor_only(v, container(tile).center(Length::Fill).into()))
            }
            // StateMessage's nothingMatched: the query named back, and how many rows it tested.
            _ if v.pane.filtering() && v.pane.view_total() == 0 => {
                let tile = column![
                    text(format!("Nothing matched {}", v.pane.filter_query)).color(p.foreground),
                    text(crate::filter::no_match(v.pane.rows.len())).size(11.0).color(p.muted),
                ]
                .spacing(6)
                .align_x(iced::Center);
                self.background(v, self.floor_only(v, container(tile).center(Length::Fill).into()))
            }
            _ if self.grid_mode() => self.background(v, self.grid_listing(v)),
            _ if self.columns_mode() => mouse_area(self.columns_view(v)).on_right_press(Message::Menu(crate::menushell::Msg::Background(v.side))).into(),
            _ => self.background(v, self.listing(v)),
        };
        let body = if v.focused { self.preview_layer(body) } else { body };
        // The grid draws no column header: its tiles have no columns to title.
        if self.grid_mode() || self.columns_mode() { column![body].width(Length::Fill).into() } else { column![header, body].width(Length::Fill).into() }
    }

    // A right click a row did not take is the background menu's: the empty space below the rows.
    fn background<'a>(&self, v: PaneView<'a>, body: Element<'a, Message>) -> Element<'a, Message> {
        let area = mouse_area(body).on_right_press(Message::Menu(crate::menushell::Msg::Background(v.side)));
        self.drag_target(area, crate::dragdrop::Target::Floor(v.side)).into()
    }

    // Rule 2: a spacer above, the rows on screen, a spacer below. Nothing else is ever built.
    // Under a filter these are view positions, each drawing the listing row the filter kept there.
    fn listing<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        let total = v.pane.view_total();
        let first = ((v.scroll_y / self.file_h()).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / self.file_h()).ceil() as usize + 1).min(total - first);
        let mut rows = column![space().height(first as f32 * self.file_h())];
        for at in first..first + shown {
            rows = rows.push(match v.pane.at_view(at) {
                Some(i) => self.row(v, i),
                None => space().height(self.file_h()).into(),
            });
        }
        rows = rows.push(space().height((total - first - shown) as f32 * self.file_h()));
        let side = v.side;
        let list = scrollable(rows.width(Length::Fill))
            .id(list_id(side))
            .on_scroll(move |vp| Message::Scrolled(side, vp))
            .height(Length::Fill)
            .width(Length::Fill);
        stack![list, self.band_over(v, 1, None, self.file_h(), true)].into()
    }

    // List.qml's and GridArea.qml's SelectionBand, enabled as it is: no listing in flight, no rename
    // open, a listing that is ready.
    // A listing with no rows drawn is still a floor a drop lands on: the layer records where it is and
    // takes no press.
    fn floor_only<'a>(&self, v: PaneView<'a>, body: Element<'a, Message>) -> Element<'a, Message> {
        stack![body, crate::band::band(v.side, 1, None, self.file_h(), 0, 0.0, v.pane.band.epoch, false, false, self.palette.accent)].into()
    }

    // `ground` is false for the columns view's middle column, which lifts rows and draws no band.
    pub(crate) fn band_over<'a>(&self, v: PaneView<'a>, columns: usize, cell_w: Option<f32>, cell_h: f32, ground: bool) -> Element<'a, Message> {
        let pane = v.pane;
        let enabled = pane.listed_once && !pane.list_in_flight() && pane.renaming.is_none() && pane.error.is_none();
        crate::band::band(v.side, columns, cell_w, cell_h, pane.view_total(), v.scroll_y, pane.band.epoch, enabled, ground, self.palette.accent)
    }

    fn row<'a>(&'a self, v: PaneView<'a>, i: usize) -> Element<'a, Message> {
        let p = self.palette;
        let pane = v.pane;
        let cursor = i == pane.cursor;
        let selected = pane.selection.contains(&i);
        let editing = pane.renaming.as_ref().filter(|r| r.index == i && v.focused);
        let cells: Element<'a, Message> = match pane.row_at(i) {
            Some(_) if editing.is_some() => {
                let r = editing.unwrap();
                let field = text_input("Name", &r.draft)
                    .id(Id::new(RENAME_ID))
                    .on_input_maybe(r.pending.is_none().then_some(Message::RenameInput))
                    .on_submit(Message::RenameSubmit)
                    .size(TEXT_PX)
                    .width(Length::Fill);
                row![field, text(r.error.clone()).color(p.error)].spacing(12).align_y(iced::Center).into()
            }
            Some(r) => {
                let link = format::is_symlink(r.p);
                let ink = p.row_ink(r);
                // A search result is a path under the walked folder: the leaf is its name and the rest
                // its location, drawn muted beside it, the split the client makes (protocol "search").
                let (leafname, location) = match (pane.search_listed(), r.n.rsplit_once('/')) {
                    (true, Some((dir, leaf))) => (leaf.to_string(), dir.to_string()),
                    _ => (r.n.clone(), String::new()),
                };
                let name = match &r.l {
                    Some(target) => format!("{leafname} -> {target}"),
                    None if !location.is_empty() => format!("{leafname}   {location}"),
                    None => leafname,
                };
                // p 0 is a failed stat, and it condemns s and m with it; see docs/protocol.md "rows".
                let size = if link {
                    "link".into()
                } else if r.d {
                    crate::dirsizes::size_text(pane.dirs.get(i), self.prefs.folder_sizes)
                } else if r.p == 0 {
                    "--".into()
                } else {
                    format::size(r.s)
                };
                let date = if r.p == 0 { "--".into() } else { format::date(r.m) };
                // Row.qml's cells, right of the name in the header's order, each drawn only when it fits.
                // A drop target draws the board's words in the columns' place (Row.qml).
                let dropping = v.drop.filter(|d| d.0 == i).map(|d| d.1);
                let cols = if dropping.is_some() { listcols::Cols::default() } else { self.list_cols(pane.search_listed()) };
                let mut line = row![self.icon(pane, i, r), text(name).color(ink).width(Length::Fill).wrapping(text::Wrapping::None)];
                if cols.mode {
                    line = line.push(text(crate::facts::permissions(r.p)).color(p.muted).width(MODE_W).wrapping(text::Wrapping::None));
                }
                if cols.size {
                    line = line.push(text(size).color(p.muted).width(SIZE_W));
                }
                if cols.items {
                    line = line.push(text(crate::dirsizes::items_text(r.d, link, pane.dirs.get(i))).color(p.muted).width(ITEMS_W));
                }
                if cols.date {
                    line = line.push(text(date).color(p.muted).width(DATE_W));
                }
                if cols.kind {
                    let kind = pane.kinds.get(r.k).cloned().unwrap_or_default();
                    line = line.push(text(kind).color(p.muted).width(KIND_W).wrapping(text::Wrapping::None));
                }
                if let Some(copy) = dropping {
                    line = line.push(text(crate::dragdrop::label(copy)).size(11.0).color(p.accent));
                }
                line.spacing(listcols::GAP).into()
            }
            // Outside the held window: the row's slot stands until its window arrives.
            None => row![text("").width(Length::Fill)].into(),
        };
        let focused = v.focused;
        let dropping = v.drop.is_some_and(|d| d.0 == i);
        let band = container(cells).padding([0.0, PAD_X]).height(self.file_h()).width(Length::Fill).align_y(iced::Center).style(move |_| {
            // The focused pane's cursor draws the accent fill; the other pane's cursor, and a marked
            // row that is not the cursor, the plain one (Row.qml in dual mode). A drop target takes
            // the board's hairline of accent over the hover rung's wash.
            if dropping {
                crate::dragshell::drop_frame(p.accent)
            } else if cursor && focused {
                container::Style::default()
                    .background(Background::Color(Color { a: 0.18, ..p.accent }))
                    .border(Border { color: p.accent, width: 0.0, radius: 0.0.into() })
            } else if selected || cursor {
                container::Style::default().background(p.selection)
            } else {
                container::Style::default()
            }
        });
        let area = mouse_area(band)
            .on_press(Message::RowPressed(v.side, i))
            .on_release(Message::RowReleased(v.side, i))
            .on_right_press(Message::Menu(crate::menushell::Msg::Row(v.side, i)));
        self.drag_target(area, crate::dragdrop::Target::Row(v.side, i)).into()
    }

    // The icon slot: the thumbnail when the pane holds a decoded one, the row's mark otherwise. One
    // slot either way, so the row's geometry never depends on which of the two it draws.
    fn icon<'a>(&'a self, pane: &'a Pane, i: usize, r: &wire::Row) -> Element<'a, Message> {
        let slot = container(match pane.thumb_file(i).and_then(|f| self.decoded.get(f)) {
            Some(h) => Element::from(image(h.clone()).width(ICON_SLOT).height(ICON_SLOT)),
            None => {
                let p = self.palette;
                let tier = pane.file_type_colours;
                let mark = marks::glyph_for_row(&r.i, r.p, &r.n, r.d, tier);
                // Theme.markInk: a vendored colour lifted to 3:1 on the background, else the dim ink.
                let ink = p.row_mark(r).or_else(|| tier
                    .then(|| marks::row_colour(&r.n, r.p, r.d))
                    .flatten()
                    .map(|c| contrast::ensure_ratio(c, p.background, 3.0)))
                    .unwrap_or(p.dim);
                let size = MARK.min(ICON_SLOT);
                Element::from(svg(marks::handle(mark, ink)).width(size).height(size))
            }
        });
        slot.width(ICON_SLOT).height(ICON_SLOT).center_x(ICON_SLOT).center_y(ICON_SLOT).into()
    }

    // ui/ShareBrowser.qml: the server's shares over the listing, one row each, Enter or a click opens.
    fn share_browser_view(&self, b: &'_ crate::netshell::ShareBrowser) -> Element<'_, Message> {
        let p = self.palette;
        let mut rows = column![];
        for (i, name) in b.names.iter().enumerate() {
            let picked = i == b.cursor;
            let line = row![svg(marks::handle("server", p.dim)).width(MARK).height(MARK), text(name.clone()).color(p.foreground)]
                .spacing(10)
                .align_y(iced::Center);
            let band = container(line).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Center).style(move |_| {
                if picked { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() }
            });
            rows = rows.push(mouse_area(band).on_press(Message::ShareChosen(i)));
        }
        container(scrollable(rows)).height(Length::Fill).width(Length::Fill).into()
    }
}

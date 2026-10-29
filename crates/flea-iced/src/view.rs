// What the window draws: the chrome, the rail, one or two panes of header and listing, and the
// status bar. The shell's state and its update live elsewhere; nothing here changes either.
use crate::pane::Pane;
use crate::{contrast, format, list_id, marks, wire, Flea, Message, DATE_W, ICON_SLOT, MARK, PAD_X, RENAME_ID, ROW_H, SIZE_W, TEXT_PX};
use iced::widget::{column, container, image, mouse_area, row, scrollable, space, svg, text, text_input, Id};
use iced::{Background, Border, Color, Element, Length};

// One pane as the view draws it: which side it sits on, where its listing is scrolled, and whether
// it holds the keyboard, which is what decides between the accent and the plain cursor fill.
#[derive(Clone, Copy)]
pub(crate) struct PaneView<'a> {
    pub(crate) pane: &'a Pane,
    pub(crate) side: u8,
    pub(crate) scroll_y: f32,
    pub(crate) focused: bool,
}

impl Flea {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let mine = PaneView { pane: &self.pane, side: self.side, scroll_y: self.scroll_y, focused: true };
        let column_of_mine = self.pane_column(mine);
        let panes: Element<'_, Message> = match &self.other {
            None => column_of_mine,
            Some(o) => {
                let theirs = self.pane_column(PaneView { pane: &o.pane, side: o.side, scroll_y: o.scroll_y, focused: false });
                // The primary is always the left one, whichever holds the keyboard.
                let (left, right) = if self.side == 0 { (column_of_mine, theirs) } else { (theirs, column_of_mine) };
                let rule = container(space().width(1).height(Length::Fill)).style(move |_| container::Style::default().background(p.surface));
                row![container(left).width(Length::FillPortion(1)), rule, container(right).width(Length::FillPortion(1))].into()
            }
        };
        let main: Element<'_, Message> = if self.rail.shown { row![self.rail_view(), panes].into() } else { panes };
        let window = container(column![self.chrome_view(), main, self.status_view()])
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
        let (note, ink) = match &self.message {
            Some((t, _, alert)) => (t.clone(), if *alert { p.error } else { p.foreground }),
            None if self.pane.search_listed() && self.pane.search.mode == crate::search::Mode::Results => (self.pane.search_status(), p.foreground),
            None => (self.pane.sticky.clone(), p.foreground),
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
        let p = self.palette;
        // ui/Header.qml: a click sorts by that column, and the order in force carries its mark.
        let order = &v.pane.order;
        let side = v.side;
        let head = |label: &'static str, key: &'static str| {
            let mark = if order.by == key { if order.desc { " ▼" } else { " ▲" } } else { "" };
            mouse_area(text(format!("{label}{mark}"))).on_press(Message::SortColumn(side, key))
        };
        let header = row![
            container(head("Name", "name")).width(Length::Fill),
            container(head("Size", "size")).width(SIZE_W),
            container(head("Date Modified", "mtime")).width(DATE_W),
        ]
        .spacing(8)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let header = container(header).style(move |_| container::Style::default().color(p.muted)).width(Length::Fill);
        let body: Element<'a, Message> = match (&self.share_browser, v.focused) {
            (Some(b), true) => self.share_browser_view(b),
            _ if v.pane.error.is_some() => container(text(v.pane.error.clone().unwrap_or_default()).color(p.muted)).center(Length::Fill).into(),
            _ if v.pane.listed_once && v.pane.total == 0 => self.background(v, container(text("This folder is empty.").color(p.muted)).center(Length::Fill).into()),
            // StateMessage's nothingMatched: the query named back, and how many rows it tested.
            _ if v.pane.filtering() && v.pane.view_total() == 0 => {
                let tile = column![
                    text(format!("Nothing matched {}", v.pane.filter_query)).color(p.foreground),
                    text(crate::filter::no_match(v.pane.rows.len())).size(11.0).color(p.muted),
                ]
                .spacing(6)
                .align_x(iced::Center);
                self.background(v, container(tile).center(Length::Fill).into())
            }
            _ if self.grid_mode() => self.background(v, self.grid_listing(v)),
            _ => self.background(v, self.listing(v)),
        };
        let body = if v.focused { self.preview_layer(body) } else { body };
        // The grid draws no column header: its tiles have no columns to title.
        if self.grid_mode() { column![body].width(Length::Fill).into() } else { column![header, body].width(Length::Fill).into() }
    }

    // A right click a row did not take is the background menu's: the empty space below the rows.
    fn background<'a>(&self, v: PaneView<'a>, body: Element<'a, Message>) -> Element<'a, Message> {
        mouse_area(body).on_right_press(Message::Menu(crate::menushell::Msg::Background(v.side))).into()
    }

    // Rule 2: a spacer above, the rows on screen, a spacer below. Nothing else is ever built.
    // Under a filter these are view positions, each drawing the listing row the filter kept there.
    fn listing<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        let total = v.pane.view_total();
        let first = ((v.scroll_y / ROW_H).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / ROW_H).ceil() as usize + 1).min(total - first);
        let mut rows = column![space().height(first as f32 * ROW_H)];
        for at in first..first + shown {
            rows = rows.push(match v.pane.at_view(at) {
                Some(i) => self.row(v, i),
                None => space().height(ROW_H).into(),
            });
        }
        rows = rows.push(space().height((total - first - shown) as f32 * ROW_H));
        let side = v.side;
        scrollable(rows.width(Length::Fill))
            .id(list_id(side))
            .on_scroll(move |vp| Message::Scrolled(side, vp))
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
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
                let ink = if link {
                    p.symlink
                } else if r.d {
                    p.foreground
                } else if format::is_executable(r.p) {
                    p.executable
                } else {
                    p.foreground
                };
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
                let size = if r.d { String::new() } else if link { "link".into() } else if r.p == 0 { "--".into() } else { format::size(r.s) };
                let date = if r.p == 0 { "--".into() } else { format::date(r.m) };
                row![
                    self.icon(pane, i, r),
                    text(name).color(ink).width(Length::Fill).wrapping(text::Wrapping::None),
                    text(size).color(p.muted).width(SIZE_W),
                    text(date).color(p.muted).width(DATE_W),
                ]
                .spacing(8)
                .into()
            }
            // Outside the held window: the row's slot stands until its window arrives.
            None => row![text("").width(Length::Fill)].into(),
        };
        let focused = v.focused;
        let band = container(cells).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Center).style(move |_| {
            // The focused pane's cursor draws the accent fill; the other pane's cursor, and a marked
            // row that is not the cursor, the plain one (Row.qml in dual mode).
            if cursor && focused {
                container::Style::default()
                    .background(Background::Color(Color { a: 0.18, ..p.accent }))
                    .border(Border { color: p.accent, width: 0.0, radius: 0.0.into() })
            } else if selected || cursor {
                container::Style::default().background(p.selection)
            } else {
                container::Style::default()
            }
        });
        mouse_area(band)
            .on_press(Message::RowPressed(v.side, i))
            .on_right_press(Message::Menu(crate::menushell::Msg::Row(v.side, i)))
            .into()
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
                let ink = tier
                    .then(|| marks::row_colour(&r.n, r.p, r.d))
                    .flatten()
                    .map(|c| contrast::ensure_ratio(c, p.background, 3.0))
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

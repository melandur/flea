// What the window draws: the header, the listing's visible rows, the icon slot and the status bar.
// The shell's state and its update live in main.rs; nothing here changes either.
use crate::{contrast, format, marks, wire, Flea, Message, DATE_W, ICON_SLOT, LIST_ID, MARK, PAD_X, RENAME_ID, ROW_H, SIZE_W, TEXT_PX};
use iced::widget::{column, container, image, mouse_area, row, scrollable, space, svg, text, text_input, Id};
use iced::{Background, Border, Color, Element, Length};

impl Flea {
    pub(crate) fn view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let header = row![
            text("Name").width(Length::Fill),
            text("Size").width(SIZE_W),
            text("Date Modified").width(DATE_W),
        ]
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let header = container(header.spacing(8))
            .style(move |_| container::Style::default().color(p.muted))
            .width(Length::Fill);

        let body: Element<'_, Message> = if let Some(b) = &self.share_browser {
            self.share_browser_view(b)
        } else if let Some(err) = &self.pane.error {
            container(text(err.clone()).color(p.muted)).center(Length::Fill).into()
        } else if self.pane.listed_once && self.pane.total == 0 {
            container(text("This folder is empty.").color(p.muted)).center(Length::Fill).into()
        } else {
            self.listing()
        };

        let mut count = if self.pane.total == 1 { "1 item".to_string() } else { format!("{} items", format::count(self.pane.total)) };
        if !self.pane.selection.is_empty() {
            count += &format!(" · {} selected", format::count(self.pane.selection.len()));
        }
        // A transient message wins the slot; the running transfer's sticky line is under it.
        let (note, ink) = match &self.message {
            Some((t, _, alert)) => (t.clone(), if *alert { p.error } else { p.foreground }),
            None => (self.pane.sticky.clone(), p.foreground),
        };
        let status = row![
            text(self.pane.path.clone()).width(Length::Fill).wrapping(text::Wrapping::None),
            text(note).color(ink),
            text(count),
        ]
        .spacing(16)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let status = container(status).style(move |_| container::Style::default().color(p.muted).background(p.surface));

        let listing = column![header, body].width(Length::Fill);
        let main: Element<'_, Message> = if self.rail.shown { row![self.rail_view(), listing].into() } else { listing.into() };
        let window = container(column![main, status])
            .style(move |_| container::Style::default().background(p.background).color(p.foreground))
            .width(Length::Fill)
            .height(Length::Fill);
        self.dialog_layer(window.into())
    }

    // Rule 2: a spacer above, the rows on screen, a spacer below. Nothing else is ever built.
    fn listing(&self) -> Element<'_, Message> {
        let total = self.pane.total;
        let first = ((self.scroll_y / ROW_H).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / ROW_H).ceil() as usize + 1).min(total - first);
        let mut rows = column![space().height(first as f32 * ROW_H)];
        for i in first..first + shown {
            rows = rows.push(self.row(i));
        }
        rows = rows.push(space().height((total - first - shown) as f32 * ROW_H));
        scrollable(rows.width(Length::Fill))
            .id(Id::new(LIST_ID))
            .on_scroll(Message::Scrolled)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn row(&self, i: usize) -> Element<'_, Message> {
        let p = self.palette;
        let cursor = i == self.pane.cursor;
        let selected = self.pane.selection.contains(&i);
        let editing = self.pane.renaming.as_ref().filter(|r| r.index == i);
        let cells: Element<'_, Message> = match self.pane.row_at(i) {
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
                let name = match &r.l {
                    Some(target) => format!("{} -> {}", r.n, target),
                    None => r.n.clone(),
                };
                // p 0 is a failed stat, and it condemns s and m with it; see docs/protocol.md "rows".
                let size = if r.d { String::new() } else if link { "link".into() } else if r.p == 0 { "--".into() } else { format::size(r.s) };
                let date = if r.p == 0 { "--".into() } else { format::date(r.m) };
                row![
                    self.icon(i, r),
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
        let band = container(cells)
            .padding([0.0, PAD_X])
            .height(ROW_H)
            .width(Length::Fill)
            .align_y(iced::Center)
            .style(move |_| {
                // The cursor draws the accent fill, a marked row that is not the cursor the plain one.
                if cursor {
                    container::Style::default()
                        .background(Background::Color(Color { a: 0.18, ..p.accent }))
                        .border(Border { color: p.accent, width: 0.0, radius: 0.0.into() })
                } else if selected {
                    container::Style::default().background(p.selection)
                } else {
                    container::Style::default()
                }
            });
        mouse_area(band).on_press(Message::RowPressed(i)).into()
    }

    // The icon slot: the thumbnail when the pane holds a decoded one, the row's mark otherwise. One
    // slot either way, so the row's geometry never depends on which of the two it draws.
    fn icon(&self, i: usize, r: &wire::Row) -> Element<'_, Message> {
        let slot = container(match self.pane.thumb_file(i).and_then(|f| self.decoded.get(f)) {
            Some(h) => Element::from(image(h.clone()).width(ICON_SLOT).height(ICON_SLOT)),
            None => {
                let p = self.palette;
                let tier = self.pane.file_type_colours;
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
            let line = row![
                svg(marks::handle("server", p.dim)).width(MARK).height(MARK),
                text(name.clone()).color(p.foreground),
            ]
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

// What the chooser draws, after SendPicker.html: the chrome strip with Back, Parent, the location and
// the chips, the rail beside the list, the save strip, and the footer saying what is checked and
// which keys act on it.
use crate::picker::{self, RECENT_LABEL};
use crate::picker::app::{Focus, PMsg, Picking, Place, LIST_ID, SAVE_ID};
use crate::picker::state::Listing;
use crate::{format, marks, wire, ICON_SLOT, MARK, PAD_X, ROW_H, SIZE_W, TEXT_PX};
use iced::widget::{button, column, container, mouse_area, row, scrollable, space, svg, text, text_input, tooltip, Column, Id, Row};
use iced::{Border, Color, Element, Length};

const RAIL_W: f32 = 172.0;
const DATE_W: f32 = 80.0;

impl Picking {
    pub fn view(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        let mut body = column![self.chrome(), row![self.rail(), self.list()].height(Length::Fill)];
        if self.c.req.saving() {
            body = body.push(self.save_strip());
        }
        body = body.push(self.footer());
        container(body).width(Length::Fill).height(Length::Fill).style(move |_| container::Style::default().background(p.background).color(p.foreground)).into()
    }

    fn chrome(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        let busy = self.c.submitting() || self.c.backend_unavailable;
        let back = self.icon_button("arrow-left", "Back".into(), None, false, (!busy && !self.c.history.is_empty()).then_some(PMsg::Back));
        let up = self.icon_button("arrow-up", "Parent folder".into(), None, false, (!busy && !self.c.recent() && self.c.path != "/").then_some(PMsg::Up));
        let location = if self.c.recent() { RECENT_LABEL.to_string() } else { self.c.path.clone() };
        let mut heading = column![text(self.title()).size(TEXT_PX + 2.0)];
        let sub = picker::subtitle(&self.c.req);
        if !sub.is_empty() {
            heading = heading.push(text(sub).size(11.0).color(p.muted));
        }
        let mut strip = column![
            row![heading, space().width(Length::Fill), self.icon_button("x", "Cancel".into(), None, false, Some(PMsg::Cancel)),
                self.icon_button("check", picker::accept_label(&self.c.req, self.c.marks.len()), (self.c.marks.len() > 1).then(|| self.c.marks.len().to_string()), true, self.c.can_accept().then_some(PMsg::Accept))]
            .spacing(8)
            .align_y(iced::Center),
            row![back, up, text(location).color(p.muted).wrapping(text::Wrapping::None)].spacing(8).align_y(iced::Center),
        ]
        .spacing(6);
        let chips = picker::chips(&self.c.req);
        if !chips.is_empty() {
            let mut line = Row::new().spacing(6);
            for (label, index) in chips {
                let on = index == self.c.filter_index;
                line = line.push(button(text(label).size(11.0)).on_press(PMsg::Chip(index)).style(move |t, s| {
                    let base = button::secondary(t, s);
                    if on { button::Style { border: Border { color: p.accent, width: 1.0, radius: 4.0.into() }, ..base } } else { base }
                }));
            }
            strip = strip.push(line);
        }
        container(strip).padding([8.0, PAD_X]).width(Length::Fill).style(move |_| container::Style::default().background(p.surface)).into()
    }

    // A glyph button with its action named in a tooltip; the primary one is filled with the accent and
    // carries the marked count beside its check.
    fn icon_button(&self, glyph: &'static str, tip: String, count: Option<String>, primary: bool, press: Option<PMsg>) -> Element<'_, PMsg> {
        let p = self.palette;
        let enabled = press.is_some();
        let ink = match (primary, enabled) {
            (_, false) => Color { a: 0.35, ..p.muted },
            (true, true) => p.background,
            (false, true) => p.foreground,
        };
        let mut face = row![svg(marks::handle(glyph, ink)).width(MARK).height(MARK)].spacing(4).align_y(iced::Center);
        if let Some(n) = count {
            face = face.push(text(n).size(11.0).color(ink));
        }
        let b = button(face).padding([4.0, 8.0]).on_press_maybe(press).style(move |_, status| {
            let fill = match (primary && enabled, status) {
                (true, button::Status::Hovered) => Some(Color { a: 0.85, ..p.accent }),
                (true, _) => Some(p.accent),
                (false, button::Status::Hovered) if enabled => Some(Color { a: 0.18, ..p.accent }),
                _ => None,
            };
            button::Style { background: fill.map(Into::into), border: Border { radius: 4.0.into(), ..Border::default() }, ..button::Style::default() }
        });
        let label = container(text(tip).size(11.0)).padding([2.0, 6.0]).style(move |_| {
            container::Style::default().background(p.surface).color(p.foreground).border(Border { color: p.muted, width: 1.0, radius: 3.0.into() })
        });
        tooltip(b, label, tooltip::Position::Bottom).into()
    }

    fn rail(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        let mut list = Column::new();
        for (i, place) in self.places.iter().enumerate() {
            let (label, glyph, path) = match place {
                Place::Recent => (RECENT_LABEL.to_string(), "history", picker::RECENT.to_string()),
                Place::Entry(e) => (e.label.clone(), e.glyph, e.path.clone()),
            };
            // The cursor while the rail holds the keyboard, the open place otherwise.
            let here = if self.focus == Focus::Rail { i == self.rail_cursor } else { path == self.c.path };
            let line = row![svg(marks::handle(glyph, p.dim)).width(MARK).height(MARK), text(label).wrapping(text::Wrapping::None)]
                .spacing(10)
                .align_y(iced::Center);
            let band = container(line).padding([0.0, 10.0]).height(ROW_H).width(Length::Fill).align_y(iced::Center).clip(true).style(move |_| {
                if here { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() }
            });
            list = list.push(mouse_area(band).on_press(PMsg::Place(i)));
        }
        container(scrollable(list)).width(RAIL_W).height(Length::Fill).into()
    }

    // Rule 2: a spacer above, the rows on screen, a spacer below. Nothing else is ever built.
    fn list(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        match self.c.state {
            Listing::Empty => return container(text("Nothing here.").color(p.muted)).center(Length::Fill).into(),
            Listing::Loading if self.c.total == 0 => return container(text("").color(p.muted)).center(Length::Fill).into(),
            _ => {}
        }
        let total = self.c.total;
        let first = ((self.scroll_y / ROW_H).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / ROW_H).ceil() as usize + 1).min(total - first);
        let mut rows = column![space().height(first as f32 * ROW_H)];
        for i in first..first + shown {
            rows = rows.push(self.row(i));
        }
        rows = rows.push(space().height((total - first - shown) as f32 * ROW_H));
        scrollable(rows.width(Length::Fill)).id(Id::new(LIST_ID)).on_scroll(PMsg::Scrolled).height(Length::Fill).width(Length::Fill).into()
    }

    fn row(&self, i: usize) -> Element<'_, PMsg> {
        let p = self.palette;
        let cursor = i == self.c.cursor && self.focus == Focus::List;
        let Some(r) = self.c.row_for(i) else { return space().height(ROW_H).into() };
        // The board draws no check at all on the rows the caller cannot receive.
        let check: Element<'_, PMsg> = if self.c.markable(r) {
            let on = self.c.marked(r);
            let frame = container(if on { Element::from(svg(marks::handle("check", p.accent)).width(12).height(12)) } else { space().into() })
                .center_x(16)
                .center_y(16)
                .style(move |_| container::Style::default().border(Border { color: if on { p.accent } else { p.muted }, width: 1.0, radius: 2.0.into() }));
            mouse_area(container(frame).center_y(ROW_H).width(24)).on_press(PMsg::Check(i)).into()
        } else {
            space().width(24).into()
        };
        // A Recent row is its whole path under "/", drawn by its own leaf.
        let name = if self.c.recent() { r.n.rsplit('/').next().unwrap_or(&r.n).to_string() } else { r.n.clone() };
        let ink = if format::is_symlink(r.p) { p.symlink } else { p.foreground };
        let size = if r.d { String::new() } else if r.p == 0 { "--".into() } else { format::size(r.s) };
        let date = if r.p == 0 { "--".into() } else { format::date(r.m).split(' ').next().unwrap_or("").to_string() };
        let cells = row![
            self.mark(r),
            text(name).color(ink).width(Length::Fill).wrapping(text::Wrapping::None),
            text(size).color(p.muted).width(SIZE_W.min(70.0)),
            text(date).color(p.muted).width(DATE_W),
        ]
        .spacing(8)
        .align_y(iced::Center);
        let band = container(cells).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Center).clip(true).style(move |_| {
            if cursor { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() }
        });
        row![check, mouse_area(band).on_press(PMsg::Row(i))].align_y(iced::Center).into()
    }

    fn mark(&self, r: &wire::Row) -> Element<'_, PMsg> {
        let glyph = marks::glyph_for_row(&r.i, r.p, &r.n, r.d, false);
        let size = MARK.min(ICON_SLOT);
        container(svg(marks::handle(glyph, self.palette.dim)).width(size).height(size)).center_x(ICON_SLOT).center_y(ICON_SLOT).into()
    }

    fn save_strip(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        let asked = if self.c.save_name.is_empty() { self.c.req.name.clone() } else { self.c.save_name.clone() };
        let out = picker::file_uri(&picker::join(&self.c.path, &self.c.save_name));
        let field = text_input("Filename", &self.c.save_name).id(Id::new(SAVE_ID)).on_input(PMsg::SaveName).on_submit(PMsg::Accept).size(TEXT_PX);
        let mut strip = column![
            row![text("Filename").size(11.0).width(70), field].spacing(8).align_y(iced::Center),
            row![text("Output").size(11.0).width(70), text(out).size(11.0).wrapping(text::Wrapping::None)].spacing(8).align_y(iced::Center),
        ]
        .spacing(6);
        let warning = if !asked.is_empty() && !picker::valid_name(&asked) {
            format!("Refused · {asked} · {}", picker::NAME_REFUSED)
        } else if !self.c.save_error.is_empty() {
            self.c.save_error.clone()
        } else if self.c.save_collision() {
            format!("{} already exists here · review before continuing", self.c.save_name)
        } else {
            String::new()
        };
        if !warning.is_empty() {
            strip = strip.push(text(warning).size(11.0).color(p.error));
        }
        if self.c.save_collision() {
            let ring = |on: bool| move |t: &iced::Theme, s| {
                let base = button::secondary(t, s);
                if on { button::Style { border: Border { color: p.accent, width: 2.0, radius: 4.0.into() }, ..base } } else { base }
            };
            strip = strip.push(row![
                space().width(Length::Fill),
                button(text("Cancel")).on_press(PMsg::Cancel).style(ring(self.focus == Focus::KeepFile)),
                button(text("Use this location").color(p.error)).on_press(PMsg::UseLocation).style(ring(self.focus == Focus::UseLocation)),
            ].spacing(8));
            strip = strip.push(text("This confirms the shown name and folder. Cancel leaves the existing file untouched.").size(11.0));
        }
        container(strip).padding([8.0, PAD_X]).width(Length::Fill).style(move |_| container::Style::default().background(p.surface)).into()
    }

    fn footer(&self) -> Element<'_, PMsg> {
        let p = self.palette;
        let (left, error) = match &self.c.message {
            Some((m, e)) => (m.clone(), *e),
            None => (picker::status_line(self.c.marks.len(), picker::total_bytes(&self.c.marks)), false),
        };
        let hints = if self.c.backend_unavailable { "Esc cancel".to_string() } else { picker::hints(&self.c.req) };
        let line = row![
            text(left).size(11.0).color(if error { p.error } else { p.foreground }).width(Length::Fill).wrapping(text::Wrapping::None),
            text(hints).size(11.0),
        ]
        .spacing(8)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        container(line).width(Length::Fill).style(move |_| container::Style::default().background(p.surface)).into()
    }
}

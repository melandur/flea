// The Convert and Open with cards as ConvertDialog.qml and OpenWithDialog.qml draw them, over the same
// backdrop the other cards use. The focused control carries the accent, since neither card hands
// iced a focusable widget: the keyboard is jobkeys.rs's.
use crate::convertcard::{label, Convert, FORMATS};
use crate::jobkeys::{Msg, EYEBROW_H, OPEN_WITH_LIST, PICK_H, VIEWPORT_ROWS};
use crate::openwith::{self, OpenWith};
use crate::{Flea, Message, TEXT_PX};
use iced::widget::{checkbox, column, container, mouse_area, opaque, row, scrollable, space, stack, text, Column};
use iced::{Border, Color, Element, Length};

impl Flea {
    pub(crate) fn job_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let (card, close) = match (&self.jobs.convert, &self.jobs.open_with) {
            (Some(c), _) => (self.convert_card(c), Msg::ConvertClose),
            (None, Some(c)) => (self.open_with_card(c), Msg::OpenWithClose),
            _ => return base,
        };
        let p = self.palette;
        let backdrop = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| {
            container::Style::default().background(Color { a: 0.5, ..p.background })
        }))
        .on_press(Message::Job(close.clone()))
        .on_right_press(Message::Job(close));
        stack![base, opaque(backdrop), container(opaque(card)).center(Length::Fill)].into()
    }

    // A row of a list a card draws: the band follows the cursor, the accent when that list has focus.
    fn pick_row<'a>(&self, line: Element<'a, Message>, chosen: bool, focused: bool, msg: Message) -> Element<'a, Message> {
        let p = self.palette;
        let band = container(line).padding([0.0, 10.0]).height(PICK_H).width(Length::Fill).align_y(iced::Center).style(move |_| {
            match (chosen, focused) {
                (true, true) => container::Style::default().background(Color { a: 0.18, ..p.accent }),
                (true, false) => container::Style::default().background(p.selection),
                _ => container::Style::default(),
            }
        });
        mouse_area(band).on_press(msg).into()
    }

    fn convert_card<'a>(&'a self, c: &'a Convert) -> Element<'a, Message> {
        let p = self.palette;
        let mut body = Column::new().spacing(8).push(text(format!("Convert {}", c.name)).color(p.foreground));
        for (i, f) in FORMATS.iter().enumerate() {
            let line = row![text(label(f)).color(p.foreground).width(Length::Fill), text(format!(".{f}")).size(11.0).color(p.muted)].into();
            body = body.push(self.pick_row(line, i == c.cursor, c.focus == 0, Message::Job(Msg::ConvertChoose(i))));
        }
        let strip = checkbox(c.strip).label("Remove metadata").on_toggle_maybe((!c.busy).then_some(|_| Message::Job(Msg::ConvertStrip)));
        let strip = container(strip).padding(2).style(move |_| if c.focus == 1 { container::Style::default().border(Border { color: p.accent, width: 1.0, radius: 4.0.into() }) } else { container::Style::default() });
        body = body.push(strip).push(text(format!("Output: {}", c.output())).size(11.0).color(p.muted).wrapping(text::Wrapping::Glyph));
        if !c.error.is_empty() {
            body = body.push(text(c.error.clone()).size(11.0).color(p.error));
            if c.collision {
                body = body.push(text("Choose another format.").size(11.0).color(p.muted));
            }
        }
        let go = if c.busy { "Converting..." } else { "Convert" };
        let buttons = row![
            space().width(Length::Fill),
            self.card_button("Cancel", c.focus == 2, false, (!c.busy).then_some(Message::Job(Msg::ConvertClose))),
            self.card_button(go, c.focus == 3 || (c.focus == 0 && c.can_convert()), false, c.can_convert().then_some(Message::Job(Msg::ConvertCommit))),
        ]
        .spacing(8);
        self.card_frame(body.push(buttons), 300.0)
    }

    fn open_with_card<'a>(&'a self, c: &'a OpenWith) -> Element<'a, Message> {
        let p = self.palette;
        let title = if c.name().is_empty() { "Open with".to_string() } else { format!("Open {} with", c.name()) };
        let mut body = Column::new().spacing(8).push(text(title).color(p.foreground));
        // The search line: what was typed, with a caret while it holds focus.
        let query = if c.query.is_empty() && c.focus != 0 { text("Search applications").color(p.muted) } else { text(format!("{}{}", c.query, if c.focus == 0 { "▏" } else { "" })).color(p.foreground) };
        let ring = if c.focus == 0 { p.accent } else { p.muted };
        body = body.push(container(query.size(TEXT_PX)).padding([6.0, 8.0]).width(Length::Fill).style(move |_| container::Style::default().background(p.background).border(Border { color: ring, width: 1.0, radius: 4.0.into() })));
        let rows = c.rows();
        let list: Element<'a, Message> = if rows.is_empty() && !c.busy {
            container(text(openwith::no_match(&c.query)).size(11.0).color(p.muted)).height(VIEWPORT_ROWS as f32 * PICK_H).center_y(Length::Fill).into()
        } else {
            let mut col = column![];
            for r in rows {
                col = col.push(match r {
                    openwith::Row::Eyebrow(t) => Element::from(container(text(t).size(11.0).color(p.muted)).padding([0.0, 10.0]).height(EYEBROW_H).align_y(iced::Bottom)),
                    openwith::Row::App(a, seat) => {
                        let hint = if a.default { "default" } else { "" };
                        let line = row![text(a.label.clone()).color(p.foreground).width(Length::Fill), text(hint).size(11.0).color(p.muted)].into();
                        self.pick_row(line, seat == c.cursor, c.focus == 1, Message::Job(Msg::OpenWithPick(seat)))
                    }
                });
            }
            scrollable(col).id(OPEN_WITH_LIST).height(VIEWPORT_ROWS as f32 * PICK_H).into()
        };
        body = body.push(list);
        let always = if c.kind.is_empty() { "Always use this application for this type".to_string() } else { format!("Always use this application for {} files", c.kind) };
        let check = checkbox(c.always).label(always).on_toggle_maybe((!c.busy).then_some(|_| Message::Job(Msg::OpenWithAlways)));
        body = body
            .push(container(check).padding(2).style(move |_| if c.focus == 2 { container::Style::default().border(Border { color: p.accent, width: 1.0, radius: 4.0.into() }) } else { container::Style::default() }))
            .push(text("Writes your default for this type to mimeapps.list. Undo does not reverse it; change it here again.").size(11.0).color(p.muted));
        if !c.error.is_empty() {
            body = body.push(text(c.error.clone()).size(11.0).color(p.error));
        } else if c.busy && !c.committing {
            body = body.push(text("Reading applications…").size(11.0).color(p.muted));
        }
        let go = if c.committing { "Opening..." } else { "Open" };
        let buttons = row![
            space().width(Length::Fill),
            self.card_button("Cancel", c.focus == 3, false, Some(Message::Job(Msg::OpenWithClose))),
            self.card_button(go, c.focus == 4, false, c.can_submit().then_some(Message::Job(Msg::OpenWithCommit))),
        ]
        .spacing(8);
        self.card_frame(body.push(buttons), 460.0)
    }
}

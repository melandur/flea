// The Trash view as ui/TrashView.qml and ui/TrashConfirm.qml draw it: its own chrome strip with
// Back, the greyed Up, the title, the count and the size, and Empty Trash at the far edge; the
// Name, Original location and Deleted header; the listing, one integer-counted row per item with
// only the held window carrying metadata; and the confirmation card over it all, whose Cancel holds
// the keyboard until the destructive button is reached deliberately.
use crate::trash::{self, Confirm, Extend};
use crate::{format, marks, Flea, Message, MARK, PAD_X, ROW_H, TEXT_PX};
use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, space, stack, svg, text, Id};
use iced::{Border, Color, Element, Length};

pub const LIST_ID: &str = "trash-list";
// The location column is 210 at bodySmall 13, Deleted the date column's own token.
const LOCATION_W: f32 = 210.0;
const DELETED_W: f32 = crate::DATE_W;
const CAPTION: f32 = 11.0;

#[derive(Debug, Clone)]
pub enum Msg {
    Row(usize),
    RowMenu(usize),
    Back,
    Empty,
    Scrolled(scrollable::Viewport),
    Cancel,
    Delete,
    Tick,
    // The count, and whether the rail's Trash menu was waiting on it.
    Count(Option<usize>, bool),
    // The monitor's report, its settle, and the sweep's start two seconds after the window opens.
    Watched(crate::trashwatch::Watch),
    Settled(u64),
    SweepDue,
}

impl Flea {
    pub(crate) fn trash_view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let t = &self.trash;
        let size = if t.bytes_ready { format!(" · {}{}", if t.bytes_partial { "≥ " } else { "" }, format::size(t.total_bytes)) } else { String::new() };
        let count = format!("{} {}{size}", t.total, if t.total == 1 { "item" } else { "items" });
        let empty_ok = t.total > 0 && !t.busy();
        let empty = button(text("Empty Trash").size(CAPTION).color(if empty_ok { p.error } else { Color { a: 0.4, ..p.muted } }))
            .on_press_maybe(empty_ok.then_some(Message::Trash(Msg::Empty)))
            .padding([4.0, 8.0])
            .style(|_, _| button::Style { background: None, ..Default::default() });
        let back = button(svg(marks::handle("arrow-left", p.foreground)).width(16).height(16))
            .on_press(Message::Trash(Msg::Back))
            .padding(6)
            .style(|_, _| button::Style { background: None, ..Default::default() });
        let strip = row![
            back,
            container(svg(marks::handle("arrow-up", p.muted)).width(16).height(16)).padding(6),
            text("Trash").size(CAPTION).color(p.foreground),
            text(count).size(CAPTION).color(p.foreground).width(Length::Fill),
            empty,
        ]
        .spacing(8)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let hairline = || container(space().height(1)).width(Length::Fill).style(move |_| container::Style::default().background(Color { a: 0.12, ..p.foreground }));
        let strip = container(strip).width(Length::Fill).style(move |_| container::Style::default().background(p.surface));
        let header = row![
            space().width(MARK),
            text("Name").size(CAPTION).width(Length::Fill),
            text("Original location").size(CAPTION).width(LOCATION_W).align_x(iced::Right),
            text("Deleted").size(CAPTION).width(DELETED_W).align_x(iced::Right),
        ]
        .spacing(8)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let body: Element<'_, Message> = if t.total == 0 {
            let (words, ink) = if !t.error_text.is_empty() {
                (t.error_text.clone(), p.error)
            } else if t.busy() {
                ("Reading Trash…".to_string(), p.muted)
            } else {
                ("Trash is empty".to_string(), p.muted)
            };
            container(text(words).color(ink)).center(Length::Fill).into()
        } else {
            self.trash_listing()
        };
        let page = container(column![strip, hairline(), header, hairline(), body]).width(Length::Fill).height(Length::Fill).style(move |_| container::Style::default().background(p.background));
        match &t.confirmation {
            Some(c) => stack![page, self.trash_confirm(c)].into(),
            None => page.into(),
        }
    }

    // Rule 2 holds here too: a spacer above, the rows on screen, a spacer below.
    fn trash_listing(&self) -> Element<'_, Message> {
        let p = self.palette;
        let t = &self.trash;
        let first = t.top.min(t.total);
        let shown = (t.visible + 1).min(t.total - first);
        let mut col = column![space().height(first as f32 * ROW_H)];
        for i in first..first + shown {
            let line: Element<'_, Message> = match t.row_at(i) {
                Some(r) => {
                    let marked = t.is_selected(&r.uri);
                    let mark = if r.directory { "folder" } else { marks::glyph_for(&r.icon) };
                    let cells = row![
                        svg(marks::handle(mark, p.foreground)).width(MARK).height(MARK),
                        text(trash::name(&r.original).to_string()).size(TEXT_PX).color(p.foreground).width(Length::Fill).wrapping(text::Wrapping::None),
                        text(trash::location(&r.original, &self.rail.home)).size(TEXT_PX).color(p.foreground).width(LOCATION_W).align_x(iced::Right).wrapping(text::Wrapping::None),
                        text(trash::deleted(&r.deleted)).size(TEXT_PX).color(p.foreground).width(DELETED_W).align_x(iced::Right).wrapping(text::Wrapping::None),
                    ]
                    .spacing(8)
                    .align_y(iced::Center);
                    // HANDOFF rule 7: a marked row takes the selection rung and the accent's edge.
                    let band = container(cells).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Center).clip(true).style(move |_| {
                        let s = container::Style::default();
                        if marked { s.background(p.selection) } else { s }
                    });
                    let edge = container(space().width(2).height(ROW_H)).style(move |_| container::Style::default().background(if marked { p.accent } else { Color::TRANSPARENT }));
                    mouse_area(row![edge, band]).on_press(Message::Trash(Msg::Row(i))).on_right_press(Message::Trash(Msg::RowMenu(i))).into()
                }
                None => space().height(ROW_H).into(),
            };
            col = col.push(line);
        }
        col = col.push(space().height((t.total - first - shown) as f32 * ROW_H));
        scrollable(col.width(Length::Fill)).id(Id::new(LIST_ID)).on_scroll(|vp| Message::Trash(Msg::Scrolled(vp))).height(Length::Fill).into()
    }

    // TrashConfirm: the alert and the question, what it deletes, Cancel and the destructive button.
    fn trash_confirm<'a>(&'a self, c: &'a Confirm) -> Element<'a, Message> {
        let p = self.palette;
        let title = if c.all { "Empty Trash?".to_string() } else { format!("Delete {} {} permanently?", c.count, if c.count == 1 { "item" } else { "items" }) };
        let body = if c.all {
            format!("{} {}, {}. This deletes them from disk. ctrl-z cannot undo it and the undo journal does not cover it.", c.count, if c.count == 1 { "item" } else { "items" }, format::size(c.bytes))
        } else {
            "These Trash items are deleted from disk. This cannot be undone.".to_string()
        };
        let cancel = button(text("Cancel").color(if c.destructive { p.foreground } else { p.background }))
            .on_press(Message::Trash(Msg::Cancel))
            .padding([6.0, 12.0])
            .style(move |_, _| button::Style {
                background: (!c.destructive).then_some(p.accent.into()),
                border: Border { color: p.muted, width: if c.destructive { 1.0 } else { 0.0 }, radius: 0.0.into() },
                ..Default::default()
            });
        let danger = button(text(if c.all { "Empty Trash" } else { "Delete" }).color(p.error))
            .on_press(Message::Trash(Msg::Delete))
            .padding([6.0, 12.0])
            .style(move |_, _| button::Style {
                background: None,
                border: Border { color: if c.destructive { p.error } else { p.muted }, width: 1.0, radius: 0.0.into() },
                ..Default::default()
            });
        let card = column![
            row![svg(marks::handle("alert", p.error)).width(17).height(17), text(title).color(p.foreground).font(iced::Font { weight: iced::font::Weight::Bold, ..iced::Font::DEFAULT })].spacing(8),
            text(body).color(p.foreground),
            row![space().width(Length::Fill), cancel, danger].spacing(8),
        ]
        .spacing(8)
        .padding(16)
        .width(372);
        let card = container(card).style(move |_| container::Style::default().background(p.surface).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }));
        // A press on the backdrop cancels, as TrashConfirm's backdrop MouseArea does.
        let backdrop = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| container::Style::default().background(Color { a: 0.5, ..p.background })))
            .on_press(Message::Trash(Msg::Cancel));
        stack![backdrop, container(opaque(card)).center(Length::Fill)].into()
    }

    pub(crate) fn trash_row_pressed(&mut self, index: usize) -> iced::Task<Message> {
        let extend = if self.modifiers.control() { Extend::Toggle } else { Extend::No };
        let outs = self.trash.choose(index as isize, extend);
        self.run_trash(outs)
    }
}

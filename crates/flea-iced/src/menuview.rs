// The listing menu as ui/ContextMenu.qml and ui/MenuRow.qml draw it: a frame of rows and hairline
// separators over a backdrop that takes the press closing it, the cursor's band, a greyed row for
// what does not apply, the danger ink for what destroys, and Sort by's flyout beside its row.
use crate::menu::{Entry, Item};
use crate::menushell::{Msg, FRAME_PAD, ITEM_H, MENU_W, SEPARATOR_H};
use crate::{marks, Flea, Message, MARK};
use iced::widget::{container, mouse_area, opaque, pin, row, space, stack, svg, text, Column};
use iced::{Border, Color, Element, Length};

impl Flea {
    pub(crate) fn menu_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(m) = &self.menu else { return base };
        let backdrop = mouse_area(space().width(Length::Fill).height(Length::Fill))
            .on_press(Message::Menu(Msg::Close))
            .on_right_press(Message::Menu(Msg::Close));
        let mut rows = Column::new().padding(FRAME_PAD);
        let mut flyout_y = m.y;
        let mut y = m.y + FRAME_PAD;
        for (i, item) in m.items.iter().enumerate() {
            match item {
                Item::Separator => {
                    rows = rows.push(self.separator());
                    y += SEPARATOR_H;
                }
                Item::Row(e) => {
                    if i == m.cursor {
                        flyout_y = y - FRAME_PAD;
                    }
                    let line = self.menu_line(e, e.label.to_string(), i == m.cursor, !e.sub.is_empty());
                    rows = rows.push(if e.disabled { line } else { mouse_area(line).on_press(Message::Menu(Msg::Chose(i))).into() });
                    y += ITEM_H;
                }
            }
        }
        let mut layers = vec![base, opaque(backdrop), pin(self.frame(rows)).x(m.x).y(m.y).into()];
        if let (true, Some(Item::Row(e))) = (m.flyout, m.items.get(m.cursor)) {
            let mut sub = Column::new().padding(FRAME_PAD);
            // Menu.submenuGlyph: one mark for every row of the flyout, keyed on the row that opened it.
            let glyph = match e.action {
                "runScript" => "terminal",
                "sort" => "sort",
                _ => "archive",
            };
            for (k, (_, label)) in e.sub.iter().enumerate() {
                let entry = Entry { glyph, ..e.clone() };
                sub = sub.push(mouse_area(self.menu_line(&entry, label.clone(), k == m.sub_cursor, false)).on_press(Message::Menu(Msg::SubChose(k))));
            }
            let height = 2.0 * FRAME_PAD + e.sub.len() as f32 * ITEM_H;
            // ContextMenu's flyout: right of the frame when it fits, else left of it, and held inside.
            let x = if m.x + 2.0 * MENU_W <= self.window_w { m.x + MENU_W } else { (m.x - MENU_W).max(0.0) };
            let window_h = self.viewport_h + 3.0 * crate::ROW_H;
            let y = flyout_y.min((window_h - height).max(0.0));
            layers.push(pin(self.frame(sub)).x(x).y(y).into());
        }
        stack(layers).into()
    }

    fn frame<'a>(&self, rows: Column<'a, Message>) -> Element<'a, Message> {
        let p = self.palette;
        container(rows)
            .width(MENU_W)
            .style(move |_| container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 6.0.into() }))
            .into()
    }

    fn separator<'a>(&self) -> Element<'a, Message> {
        let p = self.palette;
        let rule = container(space().width(Length::Fill).height(1)).style(move |_| container::Style::default().background(Color { a: 0.4, ..p.muted }));
        container(rule).height(SEPARATOR_H).width(Length::Fill).padding([4.0, 10.0]).into()
    }

    fn menu_line<'a>(&self, e: &Entry, label: String, chosen: bool, flyout: bool) -> Element<'a, Message> {
        let p = self.palette;
        // MenuRow: a row greyed because its service refused draws red, so it reads as an answer.
        let ink = if e.errored { p.error } else if e.disabled { p.muted } else if e.danger { p.error } else { p.foreground };
        let mut line = row![svg(marks::handle(e.glyph, ink)).width(MARK).height(MARK), text(label).color(ink).width(Length::Fill)]
            .spacing(10)
            .align_y(iced::Center);
        // MenuRow's hint slot: the row's own hint first, then its key while key hints are on; an
        // unbound row draws nothing and takes no width.
        let hint = if !e.hint.is_empty() { e.hint } else if self.prefs.key_hints { crate::keysheet::hint_for(e.action) } else { "" };
        if !hint.is_empty() {
            line = line.push(text(hint).size(11.0).color(p.muted));
        }
        if flyout {
            line = line.push(svg(marks::handle("chevron-right", p.muted)).width(MARK).height(MARK));
        }
        let band = !e.disabled && chosen;
        container(line)
            .padding([0.0, 10.0])
            .height(ITEM_H)
            .width(Length::Fill)
            .align_y(iced::Center)
            .style(move |_| if band { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() })
            .into()
    }
}

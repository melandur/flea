// The rail as ui/Sidebar.qml draws it: caption headings, one row per place, the cursor's band while
// the rail holds the keyboard, the folder on screen marked, and the release menu over its row.
// The rail does not scroll yet, which is what lets the menu sit at the row's own laid-out height.
use crate::rail::{Entry, Group};
use crate::{marks, Flea, Message, MARK, PAD_X, ROW_H, TEXT_PX};
use iced::widget::{container, mouse_area, opaque, pin, row, space, stack, svg, text, text_input, Column, Id};
use iced::{Background, Color, Element, Length};

pub const RAIL_RENAME_ID: &str = "rail-rename";

// iced 0.14 draws no ellipsis, so a label is cut to what the rail can hold, from an average glyph
// width of 0.55 em. That is an estimate, and the band clips besides, so a wide label can never
// paint over the listing whatever the estimate says.
fn elide(label: &str, width: f32) -> String {
    let budget = ((width - 2.0 * PAD_X - MARK - 10.0) / (TEXT_PX * 0.55)).floor().max(1.0) as usize;
    if label.chars().count() <= budget {
        return label.to_string();
    }
    let kept: String = label.chars().take(budget.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

impl Flea {
    pub(crate) fn rail_view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let mut col = Column::new().padding([8.0, 0.0]);
        let mut y = 8.0;
        let mut menu_y = None;
        let mut last_group = None;
        // NETWORK is drawn whenever its switch is on, rows or none: it is the way into the connect
        // dialog, and a heading that carries an action may not hide itself (AGENTS.md, 2026-09-18).
        let empty_network = self.rail.show_network && !self.rail.entries.iter().any(|e| e.group() == Group::Network);
        for (i, e) in self.rail.entries.iter().enumerate() {
            let group = e.group();
            if empty_network && group == Group::Devices && last_group != Some(Group::Network) {
                col = col.push(self.heading("NETWORK"));
                y += ROW_H;
                last_group = Some(Group::Network);
            }
            if last_group != Some(group) {
                // Headings follow their rows: an empty group draws no heading, as DEVICES and
                // FAVORITES do in QML.
                let title = match group {
                    Group::Places => "PLACES",
                    Group::Favourites => "FAVORITES",
                    Group::Network => "NETWORK",
                    Group::Devices => "DEVICES",
                };
                col = col.push(self.heading(title));
                y += ROW_H;
                last_group = Some(group);
            }
            col = col.push(self.rail_row(i, e));
            y += ROW_H;
            if self.rail.menu.as_ref().is_some_and(|m| m.entry == i) {
                menu_y = Some(y);
            }
        }
        if empty_network && last_group != Some(Group::Network) && last_group != Some(Group::Devices) {
            col = col.push(self.heading("NETWORK"));
        }
        let base = container(col)
            .width(self.rail.width)
            .height(Length::Fill)
            .style(move |_| container::Style::default().background(p.surface));
        let Some((menu, y)) = self.rail.menu.as_ref().zip(menu_y) else { return base.into() };
        let mut rows = Column::new().padding(4.0);
        for (k, action) in menu.actions.iter().enumerate() {
            let chosen = k == menu.selected;
            let line = row![
                svg(marks::handle(action.glyph(), p.foreground)).width(MARK).height(MARK),
                text(action.label()).color(p.foreground),
            ]
            .spacing(10)
            .align_y(iced::Center);
            let band = container(line).padding([0.0, 10.0]).height(ROW_H - 6.0).width(Length::Fill).align_y(iced::Center).style(move |_| {
                if chosen { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() }
            });
            rows = rows.push(mouse_area(band).on_press(Message::RailChose(*action)));
        }
        let card = container(rows)
            .width(self.rail.width - 2.0 * PAD_X)
            .style(move |_| container::Style::default().background(p.background).border(iced::Border { color: p.muted, width: 1.0, radius: 6.0.into() }));
        // The backdrop takes the press that closes the menu, as ui/ContextMenu.qml's does.
        let backdrop = mouse_area(space().width(Length::Fill).height(Length::Fill)).on_press(Message::RailMenuClosed);
        stack![base, opaque(backdrop), pin(card).x(PAD_X).y(y)].into()
    }

    fn heading(&self, title: &'static str) -> Element<'_, Message> {
        let p = self.palette;
        let label = text(title).size(11.0).color(p.muted);
        // NETWORK's "+" is the door into the connect dialog, on the indicators' right-hand line.
        let line: Element<'_, Message> = if title == "NETWORK" {
            let plus = mouse_area(svg(marks::handle("plus", p.muted)).width(11.0).height(11.0)).on_press(Message::Dialog(crate::dialog::Msg::Open));
            row![label, space().width(Length::Fill), plus].align_y(iced::Center).into()
        } else {
            label.into()
        };
        container(line).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Bottom).into()
    }

    fn rail_row(&self, i: usize, e: &Entry) -> Element<'_, Message> {
        let p = self.palette;
        if let Some((_, draft)) = self.rail.renaming.as_ref().filter(|(at, _)| *at == i) {
            let field = text_input("Name", draft)
                .id(Id::new(RAIL_RENAME_ID))
                .on_input(Message::RailRenameInput)
                .on_submit(Message::RailRenameSubmit)
                .size(TEXT_PX);
            return container(field).padding([0.0, PAD_X]).height(ROW_H).align_y(iced::Center).into();
        }
        let cursor = self.rail.focused && i == self.rail.cursor;
        // The folder the listing is on is the place the rail marks, whatever holds the keyboard.
        let here = !e.path.is_empty() && e.path == self.pane.path;
        let ink = if here { p.accent } else { p.foreground };
        let line = row![
            svg(marks::handle(e.glyph, if here { p.accent } else { p.dim })).width(MARK).height(MARK),
            text(elide(&e.label, self.rail.width)).color(ink).wrapping(iced::widget::text::Wrapping::None),
        ]
        .spacing(10)
        .align_y(iced::Center);
        let band = container(line).padding([0.0, PAD_X]).height(ROW_H).width(Length::Fill).align_y(iced::Center).clip(true).style(move |_| {
            if cursor {
                container::Style::default().background(Background::Color(Color { a: 0.18, ..p.accent }))
            } else {
                container::Style::default()
            }
        });
        mouse_area(band).on_press(Message::RailPressed(i)).on_right_press(Message::RailMenu(i)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::elide;

    #[test]
    fn a_label_that_fits_is_untouched_and_a_long_one_ends_in_an_ellipsis() {
        assert_eq!(elide("Downloads", 192.0), "Downloads");
        let cut = elide("Samsung SSD 990 PRO with Heatsink 4TB", 192.0);
        assert!(cut.ends_with('…') && cut.chars().count() < 37, "{cut}");
    }
}

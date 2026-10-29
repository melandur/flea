// The settings panel as ui/SettingsPanel.qml lays it out: a card over the dimmed window, the section
// rail on its left and the section's rows on its right, the cursor's band on whichever side holds
// the keyboard. A row is drawn by its kind: a heading, a switch, a set of segments, a fact, a hint.
use crate::settings::{self, Kind, Row, SECTIONS};
use crate::{marks, Flea, Message, MARK, PAD_X, ROW_H};
use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, space, stack, svg, text, Column, Row as IcedRow};
use iced::{Border, Color, Element, Length};

impl Flea {
    pub(crate) fn panel_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.panel.open {
            return base;
        }
        let p = self.palette;
        let ground = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| {
            container::Style::default().background(Color { a: 0.5, ..p.background })
        }))
        .on_press(Message::PanelClose);
        let card = container(row![self.panel_rail(), self.panel_pane()].spacing(12))
            .padding(PAD_X)
            .width(Length::FillPortion(1))
            .max_width(900)
            .height(Length::Fill)
            .style(move |_| container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }));
        stack![base, opaque(ground), container(opaque(card)).padding(48).center(Length::Fill)].into()
    }

    fn panel_rail(&self) -> Element<'_, Message> {
        let p = self.palette;
        let mut col = Column::new().width(170);
        let current = if self.panel.section == "columns" { "view" } else { self.panel.section.as_str() };
        for (id, label, glyph) in SECTIONS {
            let here = id == current;
            let band = container(row![svg(marks::handle(glyph, if here { p.accent } else { p.dim })).width(MARK).height(MARK), text(label).color(if here { p.foreground } else { p.muted })].spacing(10).align_y(iced::Center))
                .padding([0.0, 10.0])
                .height(ROW_H)
                .width(Length::Fill)
                .align_y(iced::Center)
                .style(move |_| if here { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() });
            col = col.push(mouse_area(band).on_press(Message::PanelSection(id)));
        }
        col.into()
    }

    fn panel_pane(&self) -> Element<'_, Message> {
        let rows = self.panel_rows();
        let mut col = Column::new().spacing(2).width(Length::Fill);
        for (i, r) in rows.into_iter().enumerate() {
            col = col.push(self.panel_row(i, r));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn panel_row(&self, i: usize, r: Row) -> Element<'_, Message> {
        let p = self.palette;
        let ink = if r.unavailable { Color { a: 0.45, ..p.foreground } } else { p.foreground };
        let mark = |g: Option<&'static str>| -> Element<'_, Message> {
            match g {
                Some(g) => svg(marks::handle(g, p.dim)).width(MARK).height(MARK).into(),
                None => space().width(MARK).into(),
            }
        };
        let label_ink = if r.role == "error" { p.error } else { ink };
        let body: Element<'_, Message> = match r.kind {
            Kind::Group => {
                let mut line = IcedRow::new().push(text(r.label.to_uppercase()).size(11.0).color(p.muted)).push(space().width(Length::Fill));
                if r.master {
                    line = line.push(text(format!("{} {}", r.value, match r.state { "all" => "●", "some" => "◐", _ => "○" })).size(11.0).color(p.muted));
                } else if !r.value.is_empty() {
                    line = line.push(text(r.value.clone()).size(11.0).color(if r.action.is_some() { p.accent } else { p.muted }));
                }
                container(line.align_y(iced::Bottom)).height(ROW_H).align_y(iced::Bottom).into()
            }
            Kind::Check => row![
                mark(r.glyph),
                column![text(r.label.clone()).color(label_ink), text(r.caption.clone()).size(11.0).color(p.muted)].width(Length::Fill),
                text(r.value.clone()).size(11.0).color(p.muted),
                text(if r.on { "●" } else { "○" }).color(if r.on { p.accent } else { p.muted }),
            ]
            .spacing(10)
            .align_y(iced::Center)
            .into(),
            Kind::Choice => {
                let mut segs = IcedRow::new().spacing(4);
                for (k, label) in r.labels.iter().enumerate() {
                    let on = k == r.selected || (r.values.is_empty() && *label == r.value);
                    segs = segs.push(button(text(label.clone()).size(11.0)).on_press_maybe((!r.unavailable).then_some(Message::PanelPick(i, k))).style(move |_, _| button::Style {
                        background: Some(if on { Color { a: 0.25, ..p.accent } } else { p.surface }.into()),
                        text_color: p.foreground,
                        border: Border { color: if on { p.accent } else { p.muted }, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    }));
                }
                row![mark(r.glyph), text(r.label.clone()).color(ink).width(Length::Fill), segs].spacing(10).align_y(iced::Center).into()
            }
            Kind::Ruler => {
                let mut stops = IcedRow::new().spacing(14);
                for (k, s) in r.stops.iter().enumerate() {
                    stops = stops.push(text(s.to_string()).color(if k == r.selected && r.on { p.accent } else { p.muted }));
                }
                row![space().width(MARK), stops].spacing(10).into()
            }
            Kind::Action | Kind::Favourite => row![mark(r.glyph), text(r.label.clone()).color(ink).width(Length::Fill), text(r.value.clone()).size(11.0).color(p.muted)].spacing(10).align_y(iced::Center).into(),
            Kind::Lock => row![mark(r.glyph), text(r.label.clone()).color(p.muted).width(Length::Fill), text("always").size(11.0).color(p.muted)].spacing(10).into(),
            Kind::Fact => row![
                mark(r.glyph),
                column![text(r.label.clone()).color(ink), text(r.caption.clone()).size(11.0).color(p.muted)].width(Length::Fill),
                text(r.value.clone()).color(if r.role == "live" { p.accent } else { p.muted }),
            ]
            .spacing(10)
            .into(),
            Kind::Hero => column![text(r.label.clone()).size(22.0), text(r.value.clone()).color(p.muted)].into(),
            Kind::Hint => text(r.label.clone()).size(11.0).color(match r.role { "error" => p.error, "accent" => p.accent, _ => p.muted }).into(),
            Kind::KeyPreview => {
                let mut col = Column::new();
                for (k, l) in &r.items {
                    col = col.push(row![text(k.clone()).color(p.accent).width(90), text(l.clone()).color(p.muted)]);
                }
                col.into()
            }
        };
        let focused = i == self.panel.cursor && !self.panel.on_rail;
        let band = container(body).padding([4.0, 10.0]).width(Length::Fill).style(move |_| {
            if focused { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() }
        });
        if settings::focusable(&r) { mouse_area(band).on_press(Message::PanelActivate(i)).into() } else { band.into() }
    }
}

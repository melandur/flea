// The Quick Look overlay as ui/Preview.qml lays it out: the listing dimmed under a ground that
// closes on a press, and the surface inset to 0.82 of the listing area or expanded to fill it, with
// the file's name and kind over whichever of text, table, image, archive or a sentence it draws.
use crate::kinds::Kind;
use crate::preview::INSET;
use crate::previewshell::Table;
use crate::{format, marks, Flea, Message, PAD_X, ROW_H, TEXT_PX};
use iced::widget::{column, container, image, mouse_area, opaque, row, scrollable, space, stack, svg, text, text_editor, Column, Id};
use iced::{Border, Color, ContentFit, Element, Length};

pub const TABLE_ID: &str = "preview-table";
// The average glyph advance at the body size, which is what PreviewTable sizes a column by.
const GLYPH: f32 = TEXT_PX * 0.6;

impl Flea {
    pub(crate) fn preview_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.preview.active {
            return base;
        }
        let p = self.palette;
        // groundOpacity 0.5, the dim ui/SettingsPanel.qml lays over the listing too.
        let ground = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| {
            container::Style::default().background(Color { a: 0.5, ..p.background })
        }))
        .on_press(Message::PreviewClose);
        let area_w = self.window_w - if self.rail.shown { self.rail.width } else { 0.0 };
        let (w, h) = if self.preview.expanded { (Length::Fill, Length::Fill) } else { (Length::Fixed(area_w * INSET), Length::Fixed((self.viewport_h + ROW_H) * INSET)) };
        let card = container(column![self.preview_header(), self.preview_body()].spacing(8))
            .padding(PAD_X)
            .width(w)
            .height(h)
            .style(move |_| container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }));
        stack![base, opaque(ground), container(opaque(card)).center(Length::Fill)].into()
    }

    fn preview_header(&self) -> Element<'_, Message> {
        let p = self.palette;
        let mut facts = vec![];
        if !self.preview.kind_name.is_empty() {
            facts.push(self.preview.kind_name.clone());
        }
        facts.push(format::size(self.preview.size));
        row![
            text(self.preview.name().to_string()).color(p.foreground).width(Length::Fill).wrapping(text::Wrapping::None),
            text(facts.join(" · ")).size(11.0).color(p.muted),
        ]
        .align_y(iced::Center)
        .into()
    }

    fn preview_body(&self) -> Element<'_, Message> {
        let p = self.palette;
        let failed = self.surfaces.text_failed || self.pdf.as_ref().is_some_and(|v| v.failed) || self.playback.failed;
        if let Some((sentence, failed)) = self.preview.status(failed) {
            // PreviewDeclined: a mark over the sentence, never a bare surface.
            let mark = svg(marks::handle(if failed { "alert" } else { "file" }, if failed { p.error } else { p.muted })).width(40).height(40);
            let words = text(sentence).color(if failed { p.foreground } else { p.muted });
            return container(column![mark, words].spacing(8).align_x(iced::Center)).center(Length::Fill).into();
        }
        match self.preview.kind {
            Some(Kind::Text) => {
                if let Some((_, table)) = &self.surfaces.table {
                    return self.table_view(table);
                }
                match &self.surfaces.text {
                    // No on_action: the editor is read-only, and its buffer shapes only what is shown.
                    Some((_, content)) => text_editor(content).size(TEXT_PX).height(Length::Fill).into(),
                    None => space().height(Length::Fill).into(),
                }
            }
            Some(Kind::Image) => match &self.surfaces.image {
                Some((_, Some(h))) => image(h.clone()).content_fit(ContentFit::Contain).width(Length::Fill).height(Length::Fill).into(),
                Some((_, None)) => {
                    let mark = svg(marks::handle("alert", p.error)).width(40).height(40);
                    container(column![mark, text("This image could not be read.")].spacing(8).align_x(iced::Center)).center(Length::Fill).into()
                }
                None => space().height(Length::Fill).into(),
            },
            Some(Kind::Archive) => self.archive_view(),
            Some(Kind::Pdf) => match &self.pdf {
                Some(v) => self.pdf_view(v),
                None => space().height(Length::Fill).into(),
            },
            Some(Kind::Video) => self.media_view(true),
            Some(Kind::Audio) => self.media_view(false),
            _ => space().height(Length::Fill).into(),
        }
    }

    // PreviewArchivePane: the names from the offset, as many as fit, then "+ N more" for every entry
    // the wire left out or the frame had no room for.
    fn archive_view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let Some(a) = &self.preview.archive else { return space().height(Length::Fill).into() };
        let line = 11.0 * 1.8;
        let shown = ((self.viewport_h * if self.preview.expanded { 1.0 } else { INSET } - ROW_H) / line).floor().max(1.0) as usize - 1;
        let mut col = Column::new();
        let end = (self.preview.archive_offset + shown).min(a.names.len());
        for (name, dir) in &a.names[self.preview.archive_offset.min(end)..end] {
            col = col.push(
                row![svg(marks::handle(if *dir { "folder" } else { "file" }, p.muted)).width(11).height(11), text(name.clone()).size(11.0).color(p.muted)]
                    .spacing(8)
                    .height(line)
                    .align_y(iced::Center),
            );
        }
        let more = a.entries.saturating_sub(end as u64);
        if more > 0 {
            col = col.push(text(format!("+ {more} more")).size(11.0).color(Color { a: 0.6, ..p.muted }));
        }
        col.height(Length::Fill).into()
    }

    // A gutter numbers the records, a numeric column aligns right, and only the records on screen are
    // built, the listing's own rule 2. The header row is the scrollable's first row, so it scrolls
    // sideways with its columns; QML pins it vertically too, which iced's scrollable cannot do here.
    fn table_view<'a>(&'a self, t: &'a Table) -> Element<'a, Message> {
        let p = self.palette;
        let records = t.records();
        if records == 0 {
            return container(text("This file is empty.").color(p.muted)).center(Length::Fill).into();
        }
        let widths: Vec<f32> = t.widths.iter().map(|c| (*c).max(3) as f32 * GLYPH + 16.0).collect();
        let gutter = records.to_string().len() as f32 * GLYPH + 16.0;
        let line = |cells: Vec<String>, number: String, head: bool| -> Element<'a, Message> {
            let mut r = row![text(number).size(TEXT_PX).color(p.muted).width(gutter)];
            for (c, w) in widths.iter().enumerate() {
                let cell = crate::delimited::shown(cells.get(c).map(String::as_str).unwrap_or(""));
                let align = if !head && t.numeric.get(c) == Some(&true) { iced::Right } else { iced::Left };
                r = r.push(text(cell).size(TEXT_PX).color(p.foreground).width(*w).align_x(align).wrapping(text::Wrapping::None));
            }
            container(r).height(ROW_H).align_y(iced::Center).into()
        };
        let body_rows = records - 1;
        let first = (((self.table_scroll - ROW_H).max(0.0) / ROW_H).floor() as usize).min(body_rows);
        let shown = ((self.viewport_h / ROW_H).ceil() as usize + 1).min(body_rows - first);
        let head = container(line(t.row(0), String::new(), true)).style(move |_| container::Style::default().background(p.surface));
        let mut rows = column![head, space().height(first as f32 * ROW_H)];
        for r in first..first + shown {
            rows = rows.push(line(t.row(r + 1), (r + 1).to_string(), false));
        }
        rows = rows.push(space().height((body_rows - first - shown) as f32 * ROW_H));
        scrollable(rows)
            .id(Id::new(TABLE_ID))
            .on_scroll(|v| Message::TableScrolled(v.absolute_offset().y))
            .direction(scrollable::Direction::Both { vertical: Default::default(), horizontal: Default::default() })
            .height(Length::Fill)
            .into()
    }
}

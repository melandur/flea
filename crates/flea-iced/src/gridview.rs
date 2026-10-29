// The grid view as ui/GridArea.qml and ui/GridTile.qml draw it: tiles in rows of grid.rs's columns,
// each the row's thumbnail or mark at the thumbnail size over two centred caption lines, the cursor's
// accent fill and edge, the plain fill for a marked tile, and the rename field in the caption's place.
// Rule 2 holds the same way the list's does: a spacer above, the tile rows on screen, a spacer below.
use crate::grid::{Geometry, CAPTION_LINE, GAP};
use crate::view::PaneView;
use crate::{contrast, list_id, marks, Flea, Message, PAD_X, RENAME_ID, TEXT_PX};
use iced::widget::{column, container, image, mouse_area, row, scrollable, space, svg, text, text_input, Column, Id};
use iced::{Border, Color, Element, Length};

impl Flea {
    pub(crate) fn grid_geometry(&self) -> Geometry {
        crate::grid::geometry(self.pane_width(), &self.prefs.thumb_size)
    }

    pub(crate) fn grid_listing<'a>(&'a self, v: PaneView<'a>) -> Element<'a, Message> {
        let g = self.grid_geometry();
        let total = v.pane.view_total();
        let tile_rows = total.div_ceil(g.columns);
        let first_row = ((v.scroll_y / g.cell_h).floor().max(0.0) as usize).min(tile_rows);
        let shown_rows = ((self.viewport_h / g.cell_h).ceil() as usize + 1).min(tile_rows - first_row);
        let mut col: Column<'a, Message> = column![space().height(first_row as f32 * g.cell_h)];
        for r in first_row..first_row + shown_rows {
            let mut line = row![];
            for c in 0..g.columns {
                let at = r * g.columns + c;
                line = line.push(match v.pane.at_view(at).filter(|_| at < total) {
                    Some(i) => self.tile(v, i, g),
                    None => space().width(g.cell_w).height(g.cell_h).into(),
                });
            }
            col = col.push(line);
        }
        col = col.push(space().height((tile_rows - first_row - shown_rows) as f32 * g.cell_h));
        let side = v.side;
        scrollable(col.width(Length::Fill))
            .id(list_id(side))
            .on_scroll(move |vp| Message::Scrolled(side, vp))
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn tile<'a>(&'a self, v: PaneView<'a>, i: usize, g: Geometry) -> Element<'a, Message> {
        let p = self.palette;
        let pane = v.pane;
        let Some(r) = pane.row_at(i) else { return space().width(g.cell_w).height(g.cell_h).into() };
        let (cursor, selected, focused) = (i == pane.cursor, pane.selection.contains(&i), v.focused);
        let slot: Element<'a, Message> = match pane.thumb_file(i).and_then(|f| self.decoded.get(f)) {
            Some(h) => image(h.clone()).width(g.thumb).height(g.thumb).into(),
            None => {
                let tier = pane.file_type_colours;
                let mark = marks::glyph_for_row(&r.i, r.p, &r.n, r.d, tier);
                let rest = if cursor || selected { p.foreground } else { p.muted };
                let ink = tier.then(|| marks::row_colour(&r.n, r.p, r.d)).flatten().map(|c| contrast::ensure_ratio(c, p.background, 3.0)).unwrap_or(rest);
                svg(marks::handle(mark, ink)).width(g.thumb).height(g.thumb).into()
            }
        };
        let caption: Element<'a, Message> = match pane.renaming.as_ref().filter(|e| e.index == i && focused) {
            Some(e) => column![
                text_input("Name", &e.draft).id(Id::new(RENAME_ID)).on_input_maybe(e.pending.is_none().then_some(Message::RenameInput)).on_submit(Message::RenameSubmit).size(TEXT_PX),
                text(e.error.clone()).size(11.0).color(p.error),
            ]
            .into(),
            None => text(r.n.clone()).size(11.0).color(p.foreground).align_x(iced::Center).width(Length::Fill).into(),
        };
        let body = column![
            container(slot).width(Length::Fill).center_x(Length::Fill),
            container(caption).height(2.0 * CAPTION_LINE).width(Length::Fill).clip(true),
        ]
        .spacing(GAP)
        .padding([PAD_X, GAP]);
        let band = container(body).width(g.cell_w).height(g.cell_h).style(move |_| {
            // GridTile: the cursor's accent fill with its hairline edge, a marked tile the plain fill.
            if cursor && focused {
                container::Style::default().background(Color { a: 0.18, ..p.accent }).border(Border { color: p.accent, width: 1.0, radius: 0.0.into() })
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
}

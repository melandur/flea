// The grid view's geometry and keys, as ui/GridArea.qml and ui/js/Grid.js hold them: the same rows,
// marks and thumbnails as the list, laid out as tiles, with a tile row standing in for a text row in
// every viewport sum. Toolkit free; gridview.rs draws it.
use crate::pane::{Effect, Pane};
use crate::PAD_X;

// Theme.grid at this box's base size: minCellWidth is Style.space(125), 146 px at base 14, and the
// caption reserves two lines of the 11 px caption at 15 px each. GridTile's gap is the spacing gap.
pub const MIN_CELL_W: f32 = 146.0;
pub const CAPTION_LINE: f32 = 15.0;
pub const GAP: f32 = 8.0;

// ViewState.thumbnailPixels: preview.thumbSize, "medium" when absent or unknown.
pub fn thumb_px(size: &str) -> f32 {
    match size {
        "small" => 48.0,
        "large" => 96.0,
        "xlarge" => 128.0,
        _ => 64.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub columns: usize,
    pub cell_w: f32,
    pub cell_h: f32,
    pub thumb: f32,
}

// GridArea: as many tiles across as the wider of the minimum cell and the thumbnail with its padding
// allows; a cell is the mark, a gap, two caption lines and the padding above and below.
pub fn geometry(width: f32, size: &str) -> Geometry {
    let thumb = thumb_px(size);
    let columns = ((width / MIN_CELL_W.max(thumb + 2.0 * PAD_X)).floor() as usize).max(1);
    Geometry { columns, cell_w: (width / columns as f32).floor(), cell_h: thumb + GAP + 2.0 * CAPTION_LINE + 2.0 * PAD_X, thumb }
}

impl Geometry {
    // GridArea.visibleRange: the viewport in items, (first, fully visible, last with any pixel).
    pub fn viewport(&self, scroll_y: f32, height: f32) -> (usize, usize, usize) {
        let first_row = (scroll_y / self.cell_h).floor().max(0.0) as usize;
        let whole = ((height / self.cell_h).floor() as usize).max(1);
        let last_row = (((scroll_y + height) / self.cell_h).ceil() as usize).max(first_row + 1) - 1;
        (first_row * self.columns, whole * self.columns, (last_row + 1) * self.columns - 1)
    }

    // A scroll to an item lands on the top of the tile row holding it.
    pub fn scroll_y(&self, item: usize) -> f32 {
        (item / self.columns) as f32 * self.cell_h
    }

    // GridView.positionViewAtIndex(Contain) from above: the item's tile row ends on the bottom edge.
    pub fn scroll_under(&self, item: usize, height: f32) -> f32 {
        ((item / self.columns + 1) as f32 * self.cell_h - height).max(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

impl Pane {
    // Grid.arrow: an arrow needs a real cell to land on, so a step off the grid, or across the end of
    // a tile row sideways, is taken and goes nowhere; the wrap setting is the list's, not the grid's.
    pub fn grid_arrow(&mut self, arrow: Arrow, columns: usize) -> Vec<Effect> {
        let count = self.view_total();
        let Some(here) = self.view_of(self.cursor).filter(|_| count > 0) else { return vec![] };
        let next = match arrow {
            Arrow::Down => here + columns,
            Arrow::Up if here >= columns => here - columns,
            Arrow::Left if here % columns != 0 => here - 1,
            Arrow::Right if (here + 1) % columns != 0 => here + 1,
            _ => return vec![],
        };
        match self.at_view(next).filter(|_| next < count) {
            Some(row) => self.place_cursor(row),
            None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::tests::listed;

    #[test]
    fn the_columns_follow_the_wider_of_the_minimum_cell_and_the_thumbnail() {
        let g = geometry(1408.0, "medium");
        assert_eq!((g.columns, g.cell_w, g.cell_h, g.thumb), (9, 156.0, 130.0, 64.0));
        assert_eq!(geometry(1408.0, "xlarge").columns, 9);
        assert_eq!(geometry(100.0, "medium").columns, 1);
        assert_eq!(thumb_px("nonsense"), 64.0);
    }

    #[test]
    fn the_viewport_is_whole_tile_rows_in_items() {
        let g = geometry(1408.0, "medium");
        // 900 px of a 130 px row: six whole rows, a seventh partly, scrolled one row and a bit down.
        assert_eq!(g.viewport(0.0, 900.0), (0, 54, 62));
        assert_eq!(g.viewport(140.0, 900.0), (9, 54, 71));
        assert_eq!(g.scroll_y(20), 260.0);
    }

    #[test]
    fn a_tile_below_the_screen_comes_in_with_its_row_on_the_bottom_edge() {
        let g = geometry(1408.0, "medium");
        // Tile 197 sits in row 21, column 8, and tile 198 opens row 22: each ends on an 889 px edge.
        assert_eq!(g.scroll_under(197, 889.0), 22.0 * 130.0 - 889.0);
        assert_eq!(g.scroll_under(198, 889.0), 23.0 * 130.0 - 889.0);
        // The item count rounded down, (198 + 1 - 54) / 9, put row 16 on top: row 22 ended past the edge.
        assert!(g.scroll_y(198 + 1 - 54) + 889.0 < 23.0 * 130.0);
        assert_eq!(g.scroll_under(3, 889.0), 0.0, "a row already on the first screen scrolls nowhere");
    }

    #[test]
    fn an_arrow_needs_a_cell_to_land_on() {
        let mut p = Pane::new(false, 10);
        p.open("/d".into(), None);
        listed(&mut p, "/d", 10);
        // Four across: rows 0-3, 4-7, 8-9.
        p.grid_arrow(Arrow::Down, 4);
        assert_eq!(p.cursor, 4);
        p.grid_arrow(Arrow::Down, 4);
        assert_eq!(p.cursor, 8);
        p.grid_arrow(Arrow::Down, 4);
        assert_eq!(p.cursor, 8, "no cell below");
        p.grid_arrow(Arrow::Left, 4);
        assert_eq!(p.cursor, 8, "the start of a tile row does not step back into the one above");
        p.grid_arrow(Arrow::Right, 4);
        p.grid_arrow(Arrow::Right, 4);
        assert_eq!(p.cursor, 9, "no cell past the last");
        p.grid_arrow(Arrow::Up, 4);
        p.grid_arrow(Arrow::Right, 4);
        p.grid_arrow(Arrow::Right, 4);
        assert_eq!(p.cursor, 7, "the end of a tile row does not step on into the next");
    }
}

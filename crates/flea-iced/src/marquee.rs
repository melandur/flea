// The selection band's arithmetic, ui/js/Marquee.js: the cells a rectangle covers in a grid of
// `columns` (the list is a grid of one), and the marks it leaves as it grows and shrinks. Only the
// strips entering or leaving the rectangle are visited, so a one-row move never scans the old marks.
// The band is a gesture over view positions; a filter's view maps each to the listing row it draws.
// band.rs is the widget that tracks the pointer; this is the state it changes. Toolkit free.
use crate::pane::{Effect, Pane};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cells {
    pub left: usize,
    pub right: usize,
    pub top: usize,
    pub bottom: usize,
}

#[derive(Debug, Default)]
pub struct Band {
    gesture: Option<Gesture>,
    // Ctrl at the press keeps what was marked and adds the band to it.
    additive: bool,
    // Bumped by every finish and cancel, so the widget drops a sweep the pane already ended.
    pub epoch: u64,
}

#[derive(Debug)]
struct Gesture {
    before: BTreeSet<usize>,
    cursor: usize,
    anchor: usize,
    additive: bool,
    cells: Option<Cells>,
    last: Option<usize>,
}

// Marquee.cells: content coordinates in, the covered cells out, or nothing for an empty rectangle
// or one wholly outside the cells.
#[allow(clippy::too_many_arguments)]
pub fn cells(x1: f32, y1: f32, x2: f32, y2: f32, width: f32, height: f32, columns: usize, count: usize) -> Option<Cells> {
    let (left, right) = (x1.min(x2), x1.max(x2));
    let (top, bottom) = (y1.min(y2), y1.max(y2));
    if width <= 0.0 || height <= 0.0 || count == 0 || columns == 0 || left == right || top == bottom || right <= 0.0 || bottom <= 0.0 || left >= columns as f32 * width {
        return None;
    }
    let last_row = count.div_ceil(columns) - 1;
    let floor = |v: f32| (v.floor().max(0.0)) as usize;
    let box_ = Cells {
        left: floor(left / width),
        right: ((right / width).ceil() as usize).saturating_sub(1).min(columns - 1),
        top: floor(top / height),
        bottom: ((bottom / height).ceil() as usize).saturating_sub(1).min(last_row),
    };
    (box_.top <= box_.bottom).then_some(box_)
}

// Marquee.difference: every cell of `of` that `other` does not hold, strip by strip, walked toward
// the pointer, so the last cell visited is the one the band reached last.
fn difference(of: Option<Cells>, other: Option<Cells>, columns: usize, count: usize, back_x: bool, back_y: bool, mut visit: impl FnMut(usize)) {
    let Some(b) = of else { return };
    // Signed, because a strip above or left of the overlap can be empty.
    let strip = |l: usize, r: isize, t: usize, bot: isize| (l as isize, r, t as isize, bot);
    let mut strips = vec![strip(b.left, b.right as isize, b.top, b.bottom as isize)];
    if let Some(o) = other.filter(|o| o.left <= b.right && o.right >= b.left && o.top <= b.bottom && o.bottom >= b.top) {
        let (top, bottom) = (b.top.max(o.top), b.bottom.min(o.bottom));
        strips = vec![
            strip(b.left, b.right as isize, b.top, top as isize - 1),
            strip(b.left, b.right.min(o.left.wrapping_sub(1)) as isize, top, bottom as isize),
            strip(b.left.max(o.right + 1), b.right as isize, top, bottom as isize),
            strip(b.left, b.right as isize, bottom + 1, b.bottom as isize),
        ];
        // o.left - 1 under o.left == 0 is no column at all.
        if o.left == 0 {
            strips[1].1 = -1;
        }
        strips.sort_by(|a, c| {
            let rows = if back_y { c.3.cmp(&a.3) } else { a.2.cmp(&c.2) };
            rows.then(if back_x { c.1.cmp(&a.1) } else { a.0.cmp(&c.0) })
        });
    }
    for (l, r, t, bot) in strips {
        if l > r || t > bot {
            continue;
        }
        let rows: Vec<isize> = if back_y { (t..=bot).rev().collect() } else { (t..=bot).collect() };
        for row in rows {
            let cols: Vec<isize> = if back_x { (l..=r).rev().collect() } else { (l..=r).collect() };
            for column in cols {
                let index = row as usize * columns + column as usize;
                if index < count {
                    visit(index);
                }
            }
        }
    }
}

impl Pane {
    pub fn band_active(&self) -> bool {
        self.band.gesture.is_some()
    }

    pub fn band_press(&mut self, additive: bool) {
        self.band.additive = additive;
    }

    // Marquee.begin then Marquee.update: the first move past the drag distance starts the gesture
    // from an empty selection, or from the marks held when Ctrl was down at the press.
    pub fn band_update(&mut self, box_: Option<Cells>, columns: usize, back_x: bool, back_y: bool) {
        let count = self.view_total();
        let mut g = match self.band.gesture.take() {
            Some(g) => g,
            None => {
                let before = std::mem::take(&mut self.selection);
                if self.band.additive {
                    self.selection = before.clone();
                }
                Gesture { before, cursor: self.cursor, anchor: self.select_anchor, additive: self.band.additive, cells: None, last: None }
            }
        };
        if g.cells != box_ {
            let (mut left, mut entered) = (vec![], vec![]);
            difference(g.cells, box_, columns, count, false, false, |view| left.push(view));
            difference(box_, g.cells, columns, count, back_x, back_y, |view| entered.push(view));
            for i in left.into_iter().filter_map(|v| self.at_view(v)).collect::<Vec<_>>() {
                if !(g.additive && g.before.contains(&i)) {
                    self.selection.remove(&i);
                }
            }
            for i in entered.into_iter().filter_map(|v| self.at_view(v)).collect::<Vec<_>>() {
                self.selection.insert(i);
                g.last = Some(i);
            }
            g.cells = box_;
        }
        self.band.gesture = Some(g);
    }

    // Marquee.finish: a cancel puts back the marks, the cursor and the anchor the band started from;
    // a release lands the cursor on the last cell the band reached.
    pub fn band_finish(&mut self, cancelled: bool) -> Vec<Effect> {
        let Some(g) = self.band.gesture.take() else { return vec![] };
        self.band.epoch += 1;
        if cancelled {
            self.selection = g.before;
            self.cursor = g.cursor;
            self.select_anchor = g.anchor;
            return vec![];
        }
        match g.last {
            Some(last) => {
                self.select_anchor = last;
                self.place_cursor(last)
            }
            None => vec![],
        }
    }
}

#[cfg(test)]
#[path = "marquee_tests.rs"]
mod tests;

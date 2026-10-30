// tests/js/marquee.js, case for case where the band's own arithmetic is concerned; its Tap and
// Keymap cases are the pane's and the key router's, tested beside them.
use super::*;
use crate::pane::tests::listed;

fn cells_of(x1: f32, y1: f32, x2: f32, y2: f32, columns: usize, count: usize) -> Option<Cells> {
    cells(x1, y1, x2, y2, 100.0, 20.0, columns, count)
}

fn b(x1: f32, y1: f32, x2: f32, y2: f32) -> Option<Cells> {
    cells_of(x1, y1, x2, y2, 1, 100)
}

fn pane(total: usize) -> Pane {
    let mut p = Pane::new(false, 200);
    p.open("/d".into(), None);
    listed(&mut p, "/d", total);
    p
}

fn marked(p: &Pane) -> Vec<usize> {
    p.selection.iter().copied().collect()
}

#[test]
fn the_cells_a_rectangle_covers() {
    assert_eq!(b(2.0, 0.0, 2.0, 50.0), None, "a zero-width band marks nothing");
    assert_eq!(b(101.0, 0.0, 120.0, 50.0), None, "a band right of all cells marks nothing");
    assert_eq!(b(1.0, -50.0, 50.0, -1.0), None, "a band above all cells marks nothing");
    assert_eq!(b(1.0, 0.0, 50.0, 80.0).unwrap().bottom, 3, "an exact bottom boundary excludes the next row");
}

#[test]
fn a_plain_band_marks_live_lands_on_release_and_escape_restores() {
    let mut p = pane(100);
    p.selection = [20].into();
    p.cursor = 20;
    p.select_anchor = 18;
    p.band_press(false);
    p.band_update(b(50.0, 90.0, 1.0, 21.0), 1, false, true);
    assert_eq!(marked(&p), [1, 2, 3, 4], "an upward band marks four intersecting rows live");
    assert_eq!(p.cursor, 20, "the cursor stays still while the band moves");
    p.band_update(b(50.0, 89.0, 1.0, 22.0), 1, false, true);
    assert_eq!(marked(&p), [1, 2, 3, 4]);
    p.band_finish(false);
    assert_eq!(p.cursor, 1, "release moves the cursor to the last entering row, upward");
    assert_eq!(marked(&p).len(), 4, "release leaves marks for keyboard operations");
    assert!(!p.band_active());

    p.band_update(b(1.0, 50.0, 50.0, 99.0), 1, false, false);
    assert_eq!(marked(&p), [2, 3, 4], "a fresh plain band replaces the old marks");
    p.band_finish(true);
    assert_eq!(marked(&p), [1, 2, 3, 4], "Escape restores the exact preceding marks");
    assert_eq!((p.cursor, p.select_anchor), (1, 1), "and the cursor and the anchor");
}

#[test]
fn ctrl_at_the_press_adds_and_keeps_the_original_overlap() {
    let mut p = pane(100);
    p.selection = [1, 2, 3, 4].into();
    p.band_press(true);
    p.band_update(b(1.0, 60.0, 50.0, 120.0), 1, false, false);
    assert_eq!(marked(&p), [1, 2, 3, 4, 5]);
    p.band_update(b(1.0, 80.0, 50.0, 99.0), 1, false, false);
    assert_eq!(marked(&p), [1, 2, 3, 4], "shrinking a Ctrl band preserves the original overlap");
    p.band_update(None, 1, false, false);
    assert_eq!(marked(&p), [1, 2, 3, 4], "leaving every row keeps the original Ctrl marks");
    let epoch = p.band.epoch;
    p.band_finish(false);
    assert_eq!(p.band.epoch, epoch + 1, "a finish tells the widget its sweep is over");
}

#[test]
fn a_grid_band_respects_columns_and_never_marks_a_trailing_empty_cell() {
    let mut p = pane(8);
    p.band_press(false);
    p.band_update(cells_of(101.0, 1.0, 299.0, 39.0, 3, 8), 3, false, false);
    assert_eq!(marked(&p), [1, 2, 4, 5]);
    p.band_update(cells_of(201.0, 21.0, 330.0, 90.0, 3, 8), 3, false, false);
    assert_eq!(marked(&p), [5]);
}

#[test]
fn a_filtered_band_stores_listing_rows_never_hidden_ones() {
    let mut p = pane(100);
    p.filter_shown = Some(vec![10, 15, 24, 47]);
    p.band_press(false);
    p.band_update(cells_of(1.0, 0.0, 50.0, 61.0, 1, 4), 1, false, false);
    assert_eq!(marked(&p), [10, 15, 24, 47]);
}

#[test]
fn the_difference_visits_only_what_enters_and_matches_cell_membership_everywhere() {
    let mut visited = 0;
    let c = |l, r, t, bot| Some(Cells { left: l, right: r, top: t, bottom: bot });
    difference(c(0, 0, 0, 100_000), c(0, 0, 0, 99_999), 1, 100_001, false, false, |_| visited += 1);
    assert_eq!(visited, 1, "extending a large band visits only the entering row");

    let mut boxes = vec![None];
    for left in 0..3 {
        for right in left..3 {
            for top in 0..3 {
                for bottom in top..3 {
                    boxes.push(c(left, right, top, bottom));
                }
            }
        }
    }
    let includes = |r: Option<Cells>, i: usize| r.is_some_and(|r| i % 3 >= r.left && i % 3 <= r.right && i / 3 >= r.top && i / 3 <= r.bottom);
    let mut cases = 0;
    for &a in &boxes {
        for &o in &boxes {
            let expected: Vec<usize> = (0..8).filter(|&i| includes(a, i) && !includes(o, i)).collect();
            for direction in 0..4 {
                let mut actual = vec![];
                difference(a, o, 3, 8, direction & 1 != 0, direction & 2 != 0, |i| actual.push(i));
                actual.sort();
                assert_eq!(actual, expected, "{a:?} less {o:?}, direction {direction}");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 5476);
}

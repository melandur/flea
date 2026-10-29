use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::{Reply, Request};

fn pane(names: &[&str]) -> Pane {
    let mut p = Pane::new(false, 10);
    p.open("/d".into(), None);
    p.receive(Reply::Listed { n: names.len() });
    p.receive(Reply::Rows { start: 0, rows: names.iter().map(|n| row(n, false)).collect(), kinds: vec![] });
    p
}

#[test]
fn the_filter_narrows_the_held_rows_in_their_own_order_and_maps_both_ways() {
    let mut p = pane(&["Alpha", "beta", "gamma", "ALPINE"]);
    p.filter_start();
    p.filter_typed("al");
    assert_eq!(p.filter_shown, Some(vec![0, 3]));
    assert_eq!((p.view_total(), p.at_view(1), p.view_of(3), p.view_of(1)), (2, Some(3), Some(1), None));
    assert_eq!(p.filter_summary(), "2 of 4 shown");
    p.filter_backspace();
    p.filter_backspace();
    assert!(!p.filtering() && p.view_total() == 4 && p.view_of(1) == Some(1));
}

#[test]
fn a_hidden_cursor_takes_the_first_row_standing_and_the_selection_loses_what_it_hides() {
    let mut p = pane(&["a1", "b", "a2", "c"]);
    // The cursor ends on "b", row 1, with rows 2 and 1 marked.
    p.tap(2, 1, false, false);
    p.tap(1, 1, true, false);
    p.filter_typed("a");
    assert_eq!(p.cursor, 0);
    assert_eq!(p.selection.iter().copied().collect::<Vec<_>>(), vec![2]);
    // A step moves through what is drawn: from row 0 to row 2, never onto the hidden row 1.
    p.step(1);
    assert_eq!(p.cursor, 2);
    p.select_all();
    assert_eq!(p.selection.iter().copied().collect::<Vec<_>>(), vec![0, 2]);
    // A shift range takes only the rows drawn between its ends: from "c", row 3, back to row 0 takes
    // rows 0 and 2 and not the hidden row 1 between them.
    p.select_anchor = 3;
    p.tap(0, 1, false, true);
    assert_eq!(p.selection.iter().copied().collect::<Vec<_>>(), vec![0, 2]);
}

#[test]
fn enter_stands_it_escape_and_a_new_folder_close_it_and_the_watch_keeps_it() {
    let mut p = pane(&["a1", "b"]);
    p.filter_start();
    p.filter_commit();
    assert!(!p.filter_typing && p.filter_query.is_empty(), "an empty query has nothing to stand");
    p.filter_start();
    p.filter_typed("a");
    p.filter_commit();
    assert!(!p.filter_typing && p.filtering());
    // A watched re-read narrows the same directory again.
    p.receive(Reply::Changed { path: "/d".into() });
    let out = p.watch_fired();
    assert!(matches!(sends(&out)[..], [Request::List { .. }]));
    listed(&mut p, "/d", 3);
    assert_eq!(p.filter_query, "a");
    // Escape takes it off before anything else it unwinds.
    p.escape();
    assert!(!p.filtering());
    p.filter_typed("a");
    p.open("/e".into(), None);
    assert!(!p.filtering(), "a new listing forgets the filter");
}

#[test]
fn a_typing_filter_owes_the_watch_and_nothing_drifts_the_window_under_it() {
    let mut p = Pane::new(false, 10);
    p.open("/d".into(), None);
    listed(&mut p, "/d", 1000);
    p.filter_start();
    // The fixture names run f000000 upward and the first window holds 310 of them.
    p.filter_typed("f0003");
    assert!(p.watch_busy());
    assert!(p.viewport(0, 10, 10).iter().all(|e| !matches!(e, crate::pane::Effect::Send(Request::Window { .. }))));
    assert_eq!(p.filter_summary(), "10 of 310 shown · of 1,000 in this folder");
}

#[test]
fn the_sentences_are_filter_js() {
    assert_eq!(no_match(1), "1 row here, and not it");
    assert_eq!(no_match(1200), "1,200 rows here, none of them");
    assert!(matches("README", "read") && !matches("x", ""));
}

#[test]
fn back_returns_where_a_navigation_left_and_forward_is_kept() {
    let mut p = pane(&["a"]);
    p.open("/e".into(), None);
    listed(&mut p, "/e", 1);
    assert_eq!(p.history, vec!["/d".to_string()]);
    let out = p.back();
    assert!(matches!(&sends(&out)[..], [Request::List { path, .. }] if path == "/d"));
    listed(&mut p, "/d", 1);
    assert!(p.history.is_empty() && p.forward == vec!["/e".to_string()]);
    // With no history the mouse's back button climbs.
    let out = p.mouse_back();
    assert!(matches!(&sends(&out)[..], [Request::List { path, .. }] if path == "/"));
}

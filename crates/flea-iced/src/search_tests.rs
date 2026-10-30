use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::{Order, Reply};

fn pane() -> Pane {
    let mut p = Pane::new(false, 10);
    p.open("/home/u".into(), None);
    listed(&mut p, "/home/u", 50);
    p
}

fn timer(out: &[Effect]) -> u64 {
    out.iter().find_map(|e| if let Effect::SearchTimer(g) = e { Some(*g) } else { None }).expect("no timer")
}

#[test]
fn typing_settles_into_one_walk_of_the_last_query() {
    let mut p = pane();
    p.search_start(false);
    let a = timer(&p.search_typed("h"));
    let b = timer(&p.search_typed("e"));
    assert!(p.search_live(a).is_empty(), "a superseded timer walks nothing");
    let out = p.search_live(b);
    assert_eq!(sends(&out), vec![Request::Search { path: "/home/u".into(), query: "he".into(), hidden: false, shallow: true }]);
    assert!(p.search_listed());
    // The same settled query is not walked twice.
    let c = timer(&p.search_typed(""));
    assert!(p.search_live(c).is_empty());
}

#[test]
fn a_superseded_walks_lines_are_dropped_and_the_newest_is_ranked_into_a_fresh_window() {
    let mut p = pane();
    p.search_start(true);
    let g = timer(&p.search_typed("a"));
    p.search_live(g);
    let g = timer(&p.search_typed("b"));
    p.search_live(g);
    // The first walk's listed and searching arrive, then the second walk's.
    p.receive(Reply::Listed { n: 0, v: 0 });
    assert!(p.receive(Reply::Searching { n: 99, scanned: 1, ms: 1.0 }).is_empty());
    p.receive(Reply::Listed { n: 0, v: 0 });
    p.receive(Reply::Searching { n: 3, scanned: 40, ms: 5.0 });
    assert_eq!(p.total, 3);
    let out = p.receive(Reply::Searched { n: 3, scanned: 40, ms: 6.0, cancelled: false });
    assert!(sends(&out).contains(&Request::Window { start: 0, count: p.window_size() }));
    assert!(!p.search.running && p.cursor == 0);
}

#[test]
fn escape_cancels_a_running_walk_then_returns_to_the_folder() {
    let mut p = pane();
    p.search_start(false);
    let g = timer(&p.search_typed("x"));
    p.search_live(g);
    p.receive(Reply::Listed { n: 0, v: 0 });
    assert_eq!(sends(&p.search_cancel()), vec![Request::SearchCancel]);
    p.receive(Reply::Searched { n: 1, scanned: 5, ms: 1.0, cancelled: true });
    let out = p.search_cancel();
    assert!(matches!(&sends(&out)[0], Request::List { path, .. } if path == "/home/u"));
    assert_eq!(p.search.mode, Mode::Off);
}

#[test]
fn right_on_the_line_waits_for_the_walk_of_this_exact_query() {
    let mut p = pane();
    p.search_start(false);
    p.search_typed("q");
    let refused = p.search_commit_for_right().unwrap();
    assert_eq!(refused, vec![Effect::Message("Still searching; Right opens a result once the search is done.".into())]);
}

#[test]
fn reveal_opens_the_results_folder_on_the_result() {
    let mut p = pane();
    p.search_start(true);
    let g = timer(&p.search_typed("help"));
    p.search_live(g);
    p.receive(Reply::Listed { n: 0, v: 0 });
    p.receive(Reply::Searched { n: 1, scanned: 2, ms: 1.0, cancelled: false });
    p.receive(Reply::Rows { start: 0, rows: vec![row("docs/helper.txt", false)], kinds: vec![] });
    let out = p.search_reveal();
    assert!(matches!(&sends(&out)[0], Request::List { path, .. } if path == "/home/u/docs"));
    assert_eq!(p.search.mode, Mode::Off);
}

#[test]
fn a_chord_starts_in_its_first_direction_and_the_same_chord_reverses() {
    let mut p = pane();
    let out = p.sort_chord("size");
    assert_eq!(sends(&out)[0], Request::Sort { order: Order { by: "size".into(), desc: true, ..Order::default() } });
    assert!(sends(&out).contains(&Request::Window { start: 0, count: p.window_size() }));
    p.sort_chord("size");
    assert!(!p.order.desc);
    // A header click starts ascending; the same click again reverses.
    p.sort_column("mtime");
    assert!(!p.order.desc && p.order.by == "mtime");
    p.sort_column("mtime");
    assert!(p.order.desc);
    // And the next listing carries the session's order.
    let out = p.reread();
    assert!(matches!(&sends(&out)[0], Request::List { order, .. } if order.by == "mtime" && order.desc));
}

#[test]
fn a_change_under_the_results_is_owed_to_the_folder_not_paid_into_them() {
    let mut p = pane();
    p.search_start(false);
    let g = timer(&p.search_typed("x"));
    p.search_live(g);
    assert!(p.receive(Reply::Changed { path: "/home/u".into(), sizes: true }).is_empty());
}

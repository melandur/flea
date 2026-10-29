use super::*;

pub(crate) fn row(n: &str, d: bool) -> Row {
    Row { n: n.into(), d, s: 0, m: 0, p: 0o100644, i: String::new(), t: false, k: 0, l: None }
}

pub(crate) fn rows(start: usize, count: usize) -> Vec<Row> {
    (start..start + count).map(|i| row(&format!("f{i:06}"), false)).collect()
}

pub(crate) fn listed(p: &mut Pane, path: &str, total: usize) -> Vec<Effect> {
    let first = p.window_size().min(total);
    let mut out = p.receive(Reply::Listed { n: total });
    out.extend(p.receive(Reply::Rows { start: 0, rows: rows(0, first), kinds: vec![] }));
    assert_eq!(p.path, path);
    out
}

pub(crate) fn sends(effects: &[Effect]) -> Vec<Request> {
    effects.iter().filter_map(|e| if let Effect::Send(r) = e { Some(r.clone()) } else { None }).collect()
}

#[test]
fn list_asks_for_the_first_screenful_with_it() {
    let mut p = Pane::new(false, 36);
    let out = p.open("/a".into(), None);
    assert_eq!(sends(&out), vec![Request::List { path: "/a".into(), first: 36 + 2 * BUFFER, hidden: false, order: Order::default() }]);
}

#[test]
fn a_window_reply_racing_the_new_listing_is_dropped() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 1000);
    p.open("/b".into(), None);
    // An old window answered before /b's listed line.
    p.receive(Reply::Rows { start: 500, rows: rows(500, 10), kinds: vec![] });
    assert_eq!(p.path, "/a");
    assert_eq!(p.held, 0);
    listed(&mut p, "/b", 5);
    assert_eq!((p.total, p.held, p.rows.len()), (5, 0, 5));
}

#[test]
fn scrolling_inside_the_window_asks_nothing_and_drifting_asks_once() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 100_000);
    assert!(sends(&p.viewport(100, 10, 10)).is_empty());
    let drift = sends(&p.viewport(5000, 10, 5010));
    assert_eq!(drift, vec![Request::Window { start: 5000 - BUFFER, count: 10 + 2 * BUFFER }]);
    // The same position again while that window is out costs no second request.
    assert!(sends(&p.viewport(5000, 10, 5010)).is_empty());
    p.receive(Reply::Rows { start: 5000 - BUFFER, rows: rows(5000 - BUFFER, 310), kinds: vec![] });
    assert_eq!(p.row_at(5000).unwrap().n, "f005000");
    assert!(sends(&p.viewport(5000, 10, 5010)).is_empty());
}

#[test]
fn a_failed_list_leaves_the_directory_on_screen() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 3);
    p.open("/root".into(), None);
    let out = p.receive(Reply::Error { r#where: "scan".into(), path: "/root".into(), msg: "Permission denied (os error 13)".into() });
    assert_eq!(out, vec![Effect::Alert("Permission denied on root".into())]);
    assert_eq!(p.path, "/a");
    assert!(!p.list_in_flight());
}

#[test]
fn parent_lands_on_the_directory_it_left_even_past_the_first_screen() {
    let mut p = Pane::new(false, 10);
    p.open("/a/b".into(), None);
    listed(&mut p, "/a/b", 3);
    let out = p.up();
    assert_eq!(sends(&out)[0], Request::List { path: "/a".into(), first: 310, hidden: false, order: Order::default() });
    let out = listed(&mut p, "/a", 1000);
    assert_eq!(sends(&out), vec![Request::Locate { path: "/a/b".into() }]);
    let out = p.receive(Reply::Located { directory: "/a".into(), path: "/a/b".into(), index: 700 });
    assert_eq!(p.cursor, 700);
    assert_eq!(out, vec![Effect::ScrollTo(691)]);
}

#[test]
fn a_watch_burst_is_one_timer_and_one_reread_on_the_name() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 20);
    p.step(3);
    assert_eq!(p.receive(Reply::Changed { path: "/a".into() }), vec![Effect::WatchTimer]);
    // Absorbed, not restarted.
    assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
    // A change for a directory we are not on says nothing.
    assert!(p.receive(Reply::Changed { path: "/elsewhere".into() }).is_empty());
    let out = p.watch_fired();
    assert_eq!(sends(&out)[0], Request::List { path: "/a".into(), first: 310, hidden: false, order: Order::default() });
    // A file created above the cursor shifts it down by one; the anchor follows the name.
    let first = Reply::Listed { n: 21 };
    p.receive(first);
    let mut shifted = vec![row("aaa-new", false)];
    shifted.extend(rows(0, 20));
    p.receive(Reply::Rows { start: 0, rows: shifted, kinds: vec![] });
    assert_eq!(p.row_at(p.cursor).unwrap().n, "f000003");
}

#[test]
fn a_change_during_a_navigation_away_is_not_paid_in_the_new_directory() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 5);
    p.open("/b".into(), None);
    assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
    let out = listed(&mut p, "/b", 5);
    assert!(sends(&out).is_empty());
}

#[test]
fn a_change_during_the_same_directorys_list_is_paid_when_it_lands() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 5);
    p.reread();
    assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
    let out = listed(&mut p, "/a", 6);
    assert_eq!(sends(&out).len(), 1);
}

#[test]
fn steps_clamp_at_both_ends_and_scroll_the_cursor_into_view() {
    let mut p = Pane::new(false, 10);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 30);
    assert!(p.step(-1).is_empty());
    assert_eq!(p.cursor, 0);
    assert_eq!(p.step(12), vec![Effect::ScrollTo(3)]);
    p.step(1000);
    assert_eq!(p.cursor, 29);
}

#[test]
fn wrap_at_ends_comes_round_only_from_an_end() {
    let mut p = Pane::new(false, 10).with_order(Order::default(), true);
    p.open("/a".into(), None);
    listed(&mut p, "/a", 30);
    p.step(-1);
    assert_eq!(p.cursor, 29);
    p.step(1);
    assert_eq!(p.cursor, 0);
    p.step(5);
    p.step(-10);
    assert_eq!(p.cursor, 0);
}

#[test]
fn enter_navigates_a_directory_and_hands_a_file_to_the_opener() {
    let mut p = Pane::new(false, 10);
    p.open("/".into(), None);
    p.receive(Reply::Listed { n: 2 });
    p.receive(Reply::Rows { start: 0, rows: vec![row("etc", true), row("x.txt", false)], kinds: vec![] });
    assert_eq!(sends(&p.activate())[0], Request::List { path: "/etc".into(), first: 310, hidden: false, order: Order::default() });
    let mut p2 = Pane::new(false, 10);
    p2.open("/".into(), None);
    p2.receive(Reply::Listed { n: 2 });
    p2.receive(Reply::Rows { start: 0, rows: vec![row("etc", true), row("x.txt", false)], kinds: vec![] });
    p2.step(1);
    assert_eq!(p2.activate(), vec![Effect::OpenFile("/x.txt".into())]);
}

#[test]
fn root_has_no_parent() {
    assert_eq!(parent("/"), None);
    assert_eq!(parent("/a"), Some(("/".into(), "a".into())));
    assert_eq!(parent("/a/b/"), Some(("/a".into(), "b".into())));
}

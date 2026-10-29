use super::*;
use crate::picker::request;
use crate::wire::PickerReply;

fn file(n: &str) -> Row {
    Row { n: n.into(), d: false, s: 3, m: 0, p: 0o100644, i: "text-x-generic".into(), t: false, k: 0, l: None }
}
fn dir(n: &str) -> Row {
    Row { d: true, i: "folder".into(), ..file(n) }
}
fn sent(e: &[Effect]) -> Vec<Request> {
    e.iter().filter_map(|e| if let Effect::Send(r) = e { Some(r.clone()) } else { None }).collect()
}
fn answer(id: u64) -> PickerReply {
    PickerReply { id, ok: true, ..Default::default() }
}
fn listed(c: &mut Chooser, rows: Vec<Row>) {
    c.listed(rows.len());
    c.rows_arrived(0, rows);
}

#[test]
fn the_start_folder_must_be_absolute_and_the_listing_carries_the_chips_rule() {
    let mut c = Chooser::new(request(r#"{"folder":"flea:recent","filters":[{"label":"Images","globs":["*.png"],"mimes":["image/*"]}]}"#), 10);
    let e = c.start("/home/u");
    assert_eq!(c.path, "/home/u");
    assert_eq!(sent(&e), vec![Request::PickerList { path: "/home/u".into(), first: 70, hidden: false, filter: (vec!["*.png".into()], vec!["image/*".into()]) }]);
    // All files is the chip with no rule, and choosing it lists again.
    let e = c.set_filter(-1);
    assert!(matches!(&sent(&e)[0], Request::PickerList { filter, .. } if filter.0.is_empty() && filter.1.is_empty()));
}

#[test]
fn only_the_newest_listing_is_drawn() {
    let mut c = Chooser::new(request("{}"), 10);
    c.start("/a");
    c.open("/b");
    c.listed(5);
    c.rows_arrived(0, vec![file("stale")]);
    assert_eq!((c.state, c.rows.len()), (Listing::Loading, 0));
    listed(&mut c, vec![file("fresh")]);
    assert_eq!((c.state, c.rows[0].n.as_str()), (Listing::Ready, "fresh"));
    assert_eq!(c.history, vec!["/a"]);
    c.go_back();
    assert_eq!((c.path.as_str(), c.history.len()), ("/a", 0));
}

#[test]
fn a_mark_is_checked_by_the_backend_and_enter_sends_only_what_it_reviewed() {
    let mut c = Chooser::new(request(r#"{"multiple":true}"#), 10);
    c.start("/x");
    listed(&mut c, vec![dir("sub"), file("a #.png")]);
    // A directory is not markable in a file request, and Enter on nothing marked sends nothing.
    assert!(c.toggle_mark(0).is_empty());
    assert!(matches!(c.accept(false)[0], Effect::Expire(_)));
    let e = c.toggle_mark(1);
    assert_eq!(sent(&e), vec![Request::PickerMark { id: 1, path: "/x/a #.png".into(), directory: false, multiple: true }]);
    // A second Space while the first is still being checked is refused out loud.
    c.toggle_mark(1);
    assert_eq!(c.message.as_ref().unwrap().0, "Selection is still being checked.");
    c.checked(PickerReply { marks: vec![("/x/a #.png".into(), 3)], ..answer(1) });
    assert_eq!(c.marks.len(), 1);
    assert!(c.marked(&file("a #.png")));
    let e = c.activate(1);
    assert_eq!(sent(&e), vec![Request::PickerValidate { id: 2 }]);
    assert!(c.submitting());
    let e = c.checked(PickerReply { marks: vec![("/x/a #.png".into(), 3)], ..answer(2) });
    assert_eq!(e, vec![Effect::Send(Request::PickerClose), Effect::Finish(r#"{"response":0,"uris":["file:///x/a%20%23.png"],"filter":-1}"#.into())]);
    // Exactly one answer leaves.
    assert!(c.cancel().is_empty());
}

#[test]
fn a_mark_that_moved_is_dropped_and_said() {
    let mut c = Chooser::new(request("{}"), 10);
    c.start("/x");
    listed(&mut c, vec![file("a")]);
    c.toggle_mark(0);
    c.checked(PickerReply { marks: vec![], removed: 1, ..answer(1) });
    assert_eq!(c.message, Some(("1 selected item moved or changed; select it again.".into(), true)));
    assert!(c.marks.is_empty());
}

#[test]
fn a_folder_request_with_nothing_marked_takes_the_folder_it_stands_in() {
    let mut c = Chooser::new(request(r#"{"directory":true}"#), 10);
    c.start("/x");
    listed(&mut c, vec![dir("sub")]);
    assert!(c.can_accept());
    let e = c.accept(false);
    assert_eq!(sent(&e), vec![Request::PickerMark { id: 1, path: "/x".into(), directory: true, multiple: false }]);
    let e = c.checked(PickerReply { marks: vec![("/x".into(), 0)], ..answer(1) });
    assert!(matches!(e.last(), Some(Effect::Finish(t)) if t.contains("file:///x")));
}

#[test]
fn a_save_is_probed_reviewed_and_a_collision_asks_first() {
    let mut c = Chooser::new(request(r#"{"mode":"save","name":"out.txt"}"#), 10);
    let e = c.start("/x");
    assert!(sent(&e).contains(&Request::PickerSave { id: 1, folder: "/x".into(), name: "out.txt".into() }));
    c.checked(PickerReply { review: 1, path: "/x/out.txt".into(), collision: true, ..answer(1) });
    assert!(c.save_ready() && c.save_collision());
    assert_eq!(c.accept(false), vec![Effect::FocusCancel]);
    let e = c.accept(true);
    assert_eq!(sent(&e), vec![Request::PickerReview { id: 2, review: 1 }]);
    let e = c.checked(PickerReply { path: "/x/out.txt".into(), collision: true, ..answer(2) });
    assert!(matches!(e.last(), Some(Effect::Finish(t)) if t.contains("file:///x/out.txt")));
}

#[test]
fn a_name_typed_while_its_probe_ran_is_probed_again_and_a_separator_is_refused() {
    let mut c = Chooser::new(request(r#"{"mode":"save","name":"../pwn"}"#), 10);
    assert_eq!(c.save_name, "");
    c.start("/x");
    let e = c.set_save_name("a".into());
    assert_eq!(sent(&e), vec![Request::PickerSave { id: 1, folder: "/x".into(), name: "a".into() }]);
    c.set_save_name("ab".into());
    let e = c.checked(PickerReply { review: 1, path: "/x/a".into(), ..answer(1) });
    assert_eq!(sent(&e), vec![Request::PickerSave { id: 2, folder: "/x".into(), name: "ab".into() }]);
    assert!(!c.save_ready());
    c.set_save_name("a/b".into());
    c.accept(false);
    assert_eq!(c.message.as_ref().unwrap().0, picker::NAME_REFUSED);
}

#[test]
fn recent_has_no_parent_and_lists_the_history_it_was_handed() {
    let mut c = Chooser::new(request("{}"), 10);
    c.start("/x");
    let e = c.open(picker::RECENT);
    assert_eq!(e.last(), Some(&Effect::ReadRecent));
    assert!(c.go_up().is_empty());
    let e = c.recent_read(vec!["/x/a".into()]);
    assert_eq!(sent(&e), vec![Request::ListPaths { paths: vec!["/x/a".into()], first: 70, filter: (vec![], vec![]) }]);
    c.go_back();
    // A history that lands after the location was left lists nothing.
    assert!(c.recent_read(vec!["/x/a".into()]).is_empty());
}

#[test]
fn a_dead_backend_stops_every_check_and_the_cancel_still_answers() {
    let mut c = Chooser::new(request("{}"), 10);
    c.start("/x");
    c.failed("backend", "The backend stopped");
    assert!(c.backend_unavailable && !c.can_accept());
    assert!(c.open("/y").is_empty());
    assert_eq!(c.cancel(), vec![Effect::Finish(r#"{"response":1}"#.into())]);
}

#[test]
fn scrolling_past_the_held_rows_asks_for_a_window() {
    let mut c = Chooser::new(request("{}"), 10);
    c.start("/x");
    c.listed(1000);
    c.rows_arrived(0, (0..70).map(|i| file(&format!("f{i}"))).collect());
    assert_eq!(c.drifted(30), None);
    assert_eq!(c.drifted(65), Some(Effect::Send(Request::Window { start: 48, count: 70 })));
}

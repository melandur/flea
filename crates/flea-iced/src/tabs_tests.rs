use super::*;
use crate::pane::tests::{listed, sends};
use crate::wire::Request;

fn pane_at(path: &str, total: usize) -> Pane {
    let mut p = Pane::new(false, 10);
    p.open(path.into(), None);
    listed(&mut p, path, total);
    p
}

fn lists(m: &TabMove) -> Vec<String> {
    sends(&m.effects).into_iter().filter_map(|r| if let Request::List { path, .. } = r { Some(path) } else { None }).collect()
}

#[test]
fn a_new_tab_snapshots_the_one_left_and_lists_its_own_folder_without_history() {
    let mut p = pane_at("/a", 20);
    p.step(3);
    let m = p.tab_open("/b", "list");
    assert_eq!(lists(&m), ["/b"]);
    assert_eq!((p.tab_count(), p.tabs.index), (2, 1));
    assert_eq!(p.history, Vec::<String>::new(), "a tab's opening is no navigation to step back from");
    listed(&mut p, "/b", 5);
    assert_eq!((p.tab_path(0), p.tab_path(1)), ("/a".to_string(), "/b".to_string()));
    // Back on the first tab: its folder lists again and the cursor lands on the row it was on.
    let m = p.tab_select(0, "grid");
    assert_eq!((lists(&m), m.view.as_deref()), (vec!["/a".to_string()], Some("list")));
    listed(&mut p, "/a", 20);
    assert_eq!(p.cursor, 3);
    assert_eq!(p.tab_path(1), "/b");
}

#[test]
fn a_switch_that_lists_nothing_brings_the_selection_back_and_one_that_listed_does_not() {
    let mut p = pane_at("/a", 20);
    p.selection = [2, 4].into();
    p.tab_open("/a", "list");
    assert_eq!(p.tab_count(), 2, "the same folder opens a tab without listing");
    p.selection.clear();
    let m = p.tab_select(0, "list");
    assert!(lists(&m).is_empty());
    assert_eq!(p.selection, [2, 4].into());
    p.tab_select(1, "list");
    p.reread();
    listed(&mut p, "/a", 20);
    p.selection = [7].into();
    p.tab_select(0, "list");
    assert!(p.selection.is_empty(), "a listing since the snapshot names other files by the same indices");
}

#[test]
fn the_limits_say_one_sentence_and_change_nothing() {
    let mut p = pane_at("/a", 3);
    assert_eq!(p.tab_close(0, "list").say.as_deref(), Some("Can't close the last tab."));
    for _ in 1..MAX {
        p.tab_open("/a", "list");
    }
    assert_eq!(p.tab_count(), MAX);
    assert_eq!(p.tab_open("/a", "list").say.as_deref(), Some("Nine tabs is the most."));
    assert_eq!(p.tab_select(12, "list").say.as_deref(), Some("No tab 13."));
    p.open("/b".into(), None);
    assert_eq!(p.tab_select(0, "list").say.as_deref(), Some("A directory is already loading."));
}

#[test]
fn closing_a_tab_before_the_current_keeps_the_current_and_closing_the_current_opens_its_neighbour() {
    let mut p = pane_at("/a", 3);
    p.tab_open("/b", "list");
    listed(&mut p, "/b", 3);
    p.tab_open("/c", "list");
    listed(&mut p, "/c", 3);
    assert_eq!(p.tab_close(0, "list"), TabMove::default());
    assert_eq!((p.tab_count(), p.tabs.index, p.tab_path(0)), (2, 1, "/b".to_string()));
    let m = p.tab_close(1, "list");
    assert_eq!((lists(&m), p.tabs.index), (vec!["/b".to_string()], 0));
    assert_eq!(label("/home/u", "/home/u"), "Home");
    assert_eq!(label("/home/u/docs/", "/home/u"), "docs");
    assert_eq!(label("/", "/home/u"), "/");
}

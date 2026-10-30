use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::Reply;

fn pane(names: &[(&str, bool)]) -> Pane {
    let mut p = Pane::new(false, 10);
    p.open("/a/b".into(), None);
    p.receive(Reply::Listed { n: names.len(), v: 0 });
    p.receive(Reply::Rows { start: 0, rows: names.iter().map(|(n, d)| row(n, *d)).collect(), kinds: vec![] });
    p
}

fn peeks(out: &[Request]) -> Vec<String> {
    out.iter().filter_map(|r| if let Request::Peek { path, .. } = r { Some(path.clone()) } else { None }).collect()
}

#[test]
fn the_parent_and_the_cursor_folder_are_peeked_once_and_a_move_empties_the_cache() {
    let mut p = pane(&[("dir", true), ("file", false)]);
    let mut c = Columns::default();
    assert_eq!(peeks(&c.follow(&p)), ["/a", "/a/b/dir"]);
    assert!(c.follow(&p).is_empty(), "asked once");
    assert!(c.answered("/a/b/dir", false, vec![row("x", false)], false));
    assert!(!c.answered("/elsewhere", false, vec![], false), "the path bar's own peek is not this view's");
    assert_eq!(c.rows("/a/b/dir").len(), 1);
    // A file under the cursor has no child column.
    p.step(1);
    assert_eq!(child_path(&p), "");
    assert!(c.follow(&p).is_empty());
    p.open("/a".into(), None);
    listed(&mut p, "/a", 1);
    assert_eq!(peeks(&c.follow(&p)), ["/"]);
    assert!(c.rows("/a/b/dir").is_empty(), "a pane that moved empties the cache");
    assert_eq!(parent_path("/"), "");
}

#[test]
fn a_denied_folder_draws_nothing_and_an_empty_one_draws_its_tile() {
    let p = pane(&[("dir", true)]);
    let mut c = Columns::default();
    c.follow(&p);
    c.answered("/a", false, vec![], true);
    c.answered("/a/b/dir", false, vec![], false);
    assert!(!c.draws_empty("/a") && c.draws_empty("/a/b/dir"));
}

#[test]
fn the_middle_column_enters_a_folder_on_one_tap_and_a_file_keeps_the_lists_tap() {
    let mut p = pane(&[("dir", true), ("file", false)]);
    let out = p.tap_middle(0, 1, false, false);
    assert!(matches!(&sends(&out)[..], [Request::List { path, .. }] if path == "/a/b/dir"));
    let mut p = pane(&[("dir", true), ("file", false)]);
    assert!(sends(&p.tap_middle(1, 1, false, false)).is_empty());
    assert_eq!(p.cursor, 1);
    assert!(matches!(p.tap_middle(1, 2, false, false)[..], [Effect::OpenFile(_)]));
    // A chord on a folder selects it as the list would.
    let mut p = pane(&[("dir", true), ("file", false)]);
    assert!(sends(&p.tap_middle(0, 1, true, false)).is_empty());
    assert!(p.selection.contains(&0));
}

#[test]
fn a_neighbour_folder_reveals_on_one_tap_and_a_file_opens_on_two() {
    assert_eq!(tapped_neighbour("/a", &row("d", true), 1), Some(Effect::OpenDir("/a/d".into())));
    assert_eq!(tapped_neighbour("/a", &row("d", true), 2), None);
    assert_eq!(tapped_neighbour("/a", &row("f", false), 1), None);
    assert_eq!(tapped_neighbour("/a", &row("f", false), 2), Some(Effect::OpenFile("/a/f".into())));
}

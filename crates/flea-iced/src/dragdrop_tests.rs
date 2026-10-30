// tests/js/drag.js, case for case where a drag stays inside the window; its marker, URI, shelf and
// foreign-drop cases are the platform drag's (stage 9d-ii).
use super::*;

fn lift(rows: &[usize], paths: Option<&[&str]>, copy: bool) -> Lift {
    Lift { side: 0, source: "/d".into(), src_dev: 42, rows: rows.to_vec(), paths: paths.map(|p| p.iter().map(|s| s.to_string()).collect()), copy, own: true, shelf: None }
}

#[test]
fn a_lift_carries_the_selection_only_when_the_pressed_row_is_in_it() {
    let picked: BTreeSet<usize> = [1, 2].into();
    assert_eq!(carried(&picked, 2), [1, 2], "a drag from a selected row carries the whole selection");
    assert_eq!(carried(&picked, 4), [4], "a drag from a row outside the selection carries that row alone");
    assert_eq!(carried(&BTreeSet::new(), 0), [0], "with nothing selected the pressed row is the drag");
}

#[test]
fn the_words_on_the_folder_and_in_the_bar() {
    assert_eq!((label(false), label(true)), ("move here", "copy here"));
    assert_eq!(line(2, "omarchy", false, true), "Move 2 items to omarchy · ctrl at lift copies");
    assert_eq!(line(1, "omarchy", true, true), "Copy 1 item to omarchy");
    assert_eq!(line(3, "", false, true), "Move 3 items to a folder · ctrl at lift copies");
    assert_eq!(line(3, "", false, false), "Move 3 items to a folder · ctrl at lift copies · too wide to drag out");
}

#[test]
fn finders_rule_decides_move_against_copy() {
    assert!(!copies(false, 42, 42), "same filesystem, no ctrl: a move");
    assert!(copies(true, 42, 42), "ctrl at the lift copies");
    assert!(copies(false, 42, 7), "another filesystem copies");
    assert!(copies(false, 42, 0) && copies(false, 0, 42), "an unknown filesystem copies");
}

#[test]
fn a_drop_by_path_refuses_the_source_a_folder_into_itself_and_an_item_into_its_own_folder() {
    let files = lift(&[0, 1], Some(&["/d/a.txt", "/d/b.txt"]), false);
    assert!(!can_drop_into(&files, "/d"), "rows dropped where they already live are refused");
    assert!(can_drop_into(&files, "/e"), "and taken into another directory");
    assert!(!can_drop_into(&lift(&[0], None, false), "/e"), "a drag carrying no paths is refused");
    let folder = lift(&[0], Some(&["/d/omarchy"]), false);
    assert!(!can_drop_into(&folder, "/d/omarchy"), "a folder cannot land on itself by path");
    assert!(!can_drop_into(&folder, "/d/omarchy/deep"), "nor inside its own subtree");
    assert!(can_drop_into(&folder, "/d/omarchy2"), "a sibling that starts with the same name is fine");
    let elsewhere = Lift { source: "/x".into(), ..lift(&[0], Some(&["/d/a.txt"]), false) };
    assert!(!can_drop_into(&elsewhere, "/d"), "an item cannot land in the folder it already lives in");
    assert!(can_drop_into(&elsewhere, "/d/omarchy"), "but the same item can land one folder down");
    let root = Lift { source: "/x".into(), ..lift(&[0], Some(&["/a.txt"]), false) };
    assert!(!can_drop_into(&root, "/"), "and a file straight under the root cannot land in the root");
}

#[test]
fn a_wide_drag_drops_by_index_only_on_its_own_listing() {
    let wide = lift(&[0, 2], None, false);
    assert!(can_drop_by_index(&wide, "/d", 1));
    assert!(!can_drop_by_index(&wide, "/d", 0), "but not onto a folder it carries");
    assert!(!can_drop_by_index(&wide, "/e", 1), "and never on another listing");
    assert!(!can_drop_by_index(&lift(&[], None, false), "/d", 1));
}

#[test]
fn a_lift_resolves_paths_for_held_rows_and_none_for_a_selection_reaching_past_them() {
    let mut p = Pane::new(false, 10);
    p.open("/d".into(), None);
    p.receive(crate::wire::Reply::Listed { n: 1000, v: 42 });
    p.receive(crate::wire::Reply::Rows { start: 0, rows: crate::pane::tests::rows(0, 30), kinds: vec![] });
    let one = p.lift(0, 3, false).unwrap();
    assert_eq!((one.rows.clone(), one.paths.clone(), one.src_dev), (vec![3], Some(vec!["/d/f000003".to_string()]), 42));
    p.selection = [3, 900].into();
    let wide = p.lift(0, 3, true).unwrap();
    assert_eq!((wide.rows, wide.paths, wide.copy), (vec![3, 900], None, true));
    assert!(p.lift(0, 500, false).is_none(), "a row the pane does not hold lifts nothing");
    p.open("/e".into(), None);
    assert!(p.lift(0, 3, false).is_none(), "nor does anything while a listing is out");
    assert_eq!((p.drop_path(), p.drop_dev()), ("/e".to_string(), 0));
}

#[test]
fn a_path_becomes_a_file_uri_and_comes_back() {
    assert_eq!(uri_for("/d/a.txt"), "file:///d/a.txt");
    assert_eq!(uri_for("/d/a b.txt"), "file:///d/a%20b.txt", "a space is percent encoded");
    assert_eq!(uri_for("/d/a#b.txt"), "file:///d/a%23b.txt", "and so is a hash, which encodeURI would leave alone");
    assert_eq!(uri_for("/d/x/y.txt"), "file:///d/x/y.txt", "the separators survive the encoding");
    let odd = "/d/ä %25#?.txt";
    assert_eq!(paths_from_uris(&[uri_for(odd)]), [odd], "a URI this side writes round trips back to its path");
    let uris: Vec<String> = ["file:///d/a.txt", "https://example.com/x", "file:///d/b%20c.txt"].map(String::from).to_vec();
    assert_eq!(paths_from_uris(&uris), ["/d/a.txt", "/d/b c.txt"], "a non-file URI is left behind rather than guessed at");
}

#[test]
fn the_payload_offers_a_marker_and_for_everyone_else_paths_only_when_every_row_resolved() {
    let held = lift(&[1, 3], Some(&["/d/a.txt", "/d/b.txt"]), false);
    let p = payload(&held);
    let get = |m: &str| p.iter().find(|(k, _)| k == m).map(|(_, v)| String::from_utf8(v.clone()).unwrap());
    assert_eq!(get(crate::dnd::ROWS_MIME), Some(format!("{}\n1,3\nmove\n/d\n42", instance())));
    assert_eq!(get(crate::dnd::URI_MIME).as_deref(), Some("file:///d/a.txt\r\nfile:///d/b.txt\r\n"));
    assert_eq!(get("text/plain").as_deref(), Some("/d/a.txt\n/d/b.txt"));
    let wide = payload(&lift(&[1, 900], None, true));
    assert_eq!(wide.len(), 1, "a wide selection offers the marker alone, so it cannot leave the window");
}

#[test]
fn a_foreign_drag_always_copies_and_a_shelf_drag_keeps_its_token_and_word() {
    let f = foreign(&["file:///x/a.txt".to_string()], None);
    assert!(!f.own && f.copy && f.rows.is_empty());
    assert!(can_drop_into(&f, "/d"), "an external drop lands in a folder that is not its own");
    assert!(!can_drop_into(&f, "/x"), "but not in the folder the file already lives in");
    let s = foreign(&[], Some("tok\nmove".into()));
    assert_eq!((s.shelf.as_deref().map(shelf_token), s.shelf.as_deref().map(shelf_copying)), (Some("tok"), Some(false)));
    assert!(foreign(&[], Some("\ncopy".into())).shelf.is_none(), "a shelf payload with no token is no shelf drag");
}

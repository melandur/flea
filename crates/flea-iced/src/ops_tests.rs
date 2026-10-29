use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::Reply;

fn pane(total: usize) -> Pane {
    let mut p = Pane::new(false, 10);
    p.open("/d".into(), None);
    listed(&mut p, "/d", total);
    p
}

#[test]
fn copy_resolves_rows_then_paste_sends_a_copy_into_the_current_directory() {
    let mut p = pane(5);
    p.tap(1, 1, false, false);
    p.tap(3, 1, true, false);
    assert_eq!(sends(&p.copy(false)), vec![Request::Paths { rows: vec![1, 3] }]);
    let out = p.receive(Reply::Paths { paths: vec!["/d/a".into(), "/d/b".into()] });
    assert_eq!(out, vec![Effect::Message("Copied 2 items · Ctrl+V pastes".into())]);
    let out = p.paste();
    assert_eq!(sends(&out), vec![Request::Transfer { moving: false, paths: vec!["/d/a".into(), "/d/b".into()], dest: "/d".into() }]);
    // A copy stays on the clipboard so it can be pasted again.
    p.receive(Reply::TransferStarted { id: 1, n: 2, moving: false });
    assert!(p.ops.clipboard.is_some());
}

#[test]
fn a_cut_is_spent_by_the_line_that_accepts_it_and_kept_on_a_refusal() {
    let mut p = pane(5);
    p.copy(true);
    p.receive(Reply::Paths { paths: vec!["/d/a".into()] });
    p.paste();
    assert_eq!(p.paste(), vec![Effect::Message("That cut is already being pasted.".into())]);
    p.receive(Reply::Error { r#where: "transfer".into(), path: "/d".into(), msg: "dest is not a directory".into() });
    assert!(!p.ops.clipboard.as_ref().unwrap().spending);
    p.paste();
    p.receive(Reply::TransferQueued { id: 4, n: 1, moving: true, ahead: 1 });
    assert!(p.ops.clipboard.is_none());
}

#[test]
fn nothing_to_paste_names_the_chords_that_exist() {
    let mut p = pane(1);
    assert_eq!(p.paste(), vec![Effect::Message("There is nothing to paste; Ctrl+C copies and Ctrl+X cuts.".into())]);
}

#[test]
fn a_transfer_reports_progress_one_failure_and_its_done_line() {
    let mut p = pane(3);
    p.receive(Reply::TransferStarted { id: 7, n: 2, moving: true });
    assert_eq!(p.sticky, "Moving 1 of 2");
    p.receive(Reply::TransferProgress { id: 7, index: 0, name: "a.iso".into(), bytes: 1, total: 9 });
    assert_eq!(p.sticky, "Moving 1 of 2 · a.iso");
    let out = p.receive(Reply::TransferItem { id: 7, index: 1, name: "b".into(), ok: false, err: "permission denied".into() });
    assert_eq!(out, vec![Effect::Alert("Move failed: b · permission denied".into())]);
    let out = p.receive(Reply::TransferDone { id: 7, ok: 1, failed: 1, skipped: 0, cancelled: false });
    assert_eq!(out[0], Effect::Message("Moved 1 of 2 · 1 failed · Ctrl+Z undoes".into()));
    assert!(matches!(sends(&out)[0], Request::List { .. }));
    assert!(p.sticky.is_empty() && p.ops.transfer.is_none());
}

#[test]
fn escape_unwinds_the_selection_then_cancels_then_takes_the_newest_queued() {
    let mut p = pane(3);
    p.receive(Reply::TransferStarted { id: 1, n: 1, moving: false });
    p.receive(Reply::TransferQueued { id: 2, n: 1, moving: false, ahead: 1 });
    p.receive(Reply::TransferQueued { id: 3, n: 1, moving: false, ahead: 2 });
    p.toggle_select();
    assert!(p.escape().is_empty());
    assert!(p.selection.is_empty());
    assert_eq!(sends(&p.escape()), vec![Request::TransferCancel { id: 1 }]);
    assert_eq!(sends(&p.escape()), vec![Request::TransferCancel { id: 3 }]);
    let out = p.receive(Reply::TransferDone { id: 3, ok: 0, failed: 0, skipped: 1, cancelled: true });
    assert_eq!(out, vec![Effect::Message("Cancelled a queued transfer.".into())]);
}

#[test]
fn trash_sends_rows_and_lands_on_the_row_that_took_the_blocks_place() {
    let mut p = pane(10);
    p.tap(4, 1, false, false);
    p.tap(6, 1, false, true);
    assert_eq!(sends(&p.trash()), vec![Request::Trash { rows: vec![4, 5, 6] }]);
    let out = p.receive(Reply::Trashed { ok: 3, failed: 0 });
    assert_eq!(out[0], Effect::Message("Moved 3 items to Trash · Ctrl+Z undoes".into()));
    p.receive(Reply::Listed { n: 7 });
    let mut after: Vec<_> = (0..4).map(|i| row(&format!("f{i:06}"), false)).collect();
    after.extend((7..10).map(|i| row(&format!("f{i:06}"), false)));
    let out = p.receive(Reply::Rows { start: 0, rows: after, kinds: vec![] });
    // The anchor named a row that is gone, so the name arena is asked and answers -1.
    assert_eq!(sends(&out), vec![Request::Locate { path: "/d/f000004".into() }]);
    p.receive(Reply::Located { directory: "/d".into(), path: "/d/f000004".into(), index: -1 });
    assert_eq!(p.row_at(p.cursor).unwrap().n, "f000007");
    assert!(p.selection.is_empty());
}

#[test]
fn rename_refuses_a_bad_name_inside_the_editor_and_a_late_reply_finishes_nothing() {
    let mut p = pane(3);
    p.step(1);
    assert!(p.start_rename().contains(&Effect::FocusRename));
    p.rename_input("a/b".into());
    assert!(p.commit_rename().is_empty());
    assert_eq!(p.renaming.as_ref().unwrap().error, "A name cannot be empty, . or .., or contain /.");
    p.rename_input("new.txt".into());
    assert_eq!(sends(&p.commit_rename()), vec![Request::Rename { path: "/d/f000001".into(), to: "new.txt".into() }]);
    // A reply for some other rename is not this one.
    assert!(p.receive(Reply::Renamed { path: "/d/other".into() }).is_empty());
    let out = p.receive(Reply::Error { r#where: "rename".into(), path: "/d/f000001".into(), msg: "File exists (os error 17)".into() });
    assert!(out.is_empty());
    assert_eq!(p.renaming.as_ref().unwrap().error, "A file with that name is already here.");
    p.commit_rename();
    let out = p.receive(Reply::Renamed { path: "/d/new.txt".into() });
    assert!(p.renaming.is_none());
    assert!(matches!(sends(&out)[0], Request::List { .. }));
}

#[test]
fn a_new_folder_lands_the_cursor_on_it_with_its_rename_field_open() {
    let mut p = pane(2);
    assert_eq!(sends(&p.new_folder()), vec![Request::Mkdir { path: "/d".into() }]);
    let out = p.receive(Reply::Made { path: "/d/New Folder".into() });
    assert_eq!(out[0], Effect::Message("Created New Folder · Ctrl+Z undoes".into()));
    p.receive(Reply::Listed { n: 3 });
    let out = p.receive(Reply::Rows { start: 0, rows: vec![row("New Folder", true), row("a", false), row("b", false)], kinds: vec![] });
    assert!(out.contains(&Effect::FocusRename));
    assert_eq!(p.renaming.as_ref().unwrap().source, "/d/New Folder");
}

#[test]
fn a_selection_owes_the_watched_reread_and_clearing_it_pays() {
    let mut p = pane(3);
    p.toggle_select();
    assert!(p.receive(Reply::Changed { path: "/d".into() }).is_empty());
    let out = p.escape();
    assert!(matches!(sends(&out)[0], Request::List { .. }));
}

#[test]
fn a_plain_double_tap_selects_and_opens() {
    let mut p = pane(3);
    let out = p.tap(2, 2, false, false);
    assert_eq!(p.selection, [2].into());
    assert_eq!(out.last(), Some(&Effect::OpenFile("/d/f000002".into())));
}

#[test]
fn duplicate_names_the_cursor_row_alone_and_its_answer_lands_on_the_copy() {
    let mut p = pane(5);
    p.tap(1, 1, false, false);
    p.tap(3, 1, true, false);
    assert_eq!(sends(&p.duplicate()), vec![Request::Duplicate { path: "/d/f000003".into() }]);
    let out = p.receive(Reply::Duplicated { path: "/d/f000003 copy".into() });
    assert_eq!(out[0], Effect::Message("Duplicated to f000003 copy · Ctrl+Z undoes".into()));
    assert!(matches!(sends(&out)[..], [Request::List { .. }]));
    assert!(Pane::new(false, 10).duplicate().is_empty());
}

#[test]
fn an_open_menu_owes_the_watched_reread_and_closing_it_pays() {
    let mut p = pane(5);
    p.menu_open = true;
    assert!(p.receive(Reply::Changed { path: "/d".into() }).iter().all(|e| !matches!(e, Effect::Send(_))));
    p.watch_fired();
    assert!(sends(&p.pay_debt()).is_empty());
    p.menu_open = false;
    assert!(matches!(sends(&p.pay_debt())[..], [Request::List { .. }]));
}

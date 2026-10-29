use super::*;

fn sent(out: &[Out]) -> Vec<Value> {
    out.iter().filter_map(|o| if let Out::Send(v) = o { Some(v.clone()) } else { None }).collect()
}

fn item(n: usize) -> Value {
    json!({"uri": format!("trash:///f{n}"), "original": format!("/home/u/f{n}"), "size": 10, "deleted": "2026-09-08T10:00:00",
           "directory": false, "icon": "text-x-generic", "identity": format!("id{n}"), "bulkSelected": true})
}

// Answers the one request in flight with `fields`, as the backend would.
fn answer(t: &mut Trash, out: &[Out], fields: Value) -> Vec<Out> {
    let ask = sent(out).pop().expect("a request");
    let mut reply = json!({"t": "trashbrowse", "id": ask["id"], "op": ask["op"], "ok": true});
    for (k, v) in fields.as_object().unwrap() {
        reply[k] = v.clone();
    }
    t.receive(&reply)
}

fn opened(total: usize) -> Trash {
    let mut t = Trash { window_rows: 20, visible: 10, ..Default::default() };
    let out = t.open("");
    let rows: Vec<Value> = (0..total.min(20)).map(item).collect();
    let after = answer(&mut t, &out, json!({"total": total, "start": 0, "rows": rows, "present": []}));
    assert_eq!(sent(&after)[0]["op"], "summary");
    answer(&mut t, &after, json!({"bytes": 30, "partial": false}));
    t
}

#[test]
fn opening_lists_with_recovery_then_asks_the_summary_and_drops_any_other_reply() {
    let mut t = Trash { window_rows: 20, ..Default::default() };
    let out = t.open("");
    let ask = &sent(&out)[0];
    assert_eq!((ask["c"].as_str(), ask["op"].as_str(), ask["recover"].as_bool()), (Some("trashbrowse"), Some("list"), Some(true)));
    assert!(t.receive(&json!({"id": 99, "op": "list", "ok": true, "total": 5})).is_empty(), "not the request in flight");
    let t = opened(3);
    assert_eq!((t.total, t.rows.len(), t.total_bytes, t.bytes_ready), (3, 3, 30, true));
    assert!(!t.busy());
}

#[test]
fn a_selection_restores_by_uri_with_the_identity_each_row_was_listed_with() {
    let mut t = opened(3);
    t.choose(0, Extend::No);
    t.choose(2, Extend::Toggle);
    assert_eq!(t.selected_count(), 2);
    t.choose(2, Extend::Toggle);
    assert_eq!(t.selected_count(), 1);
    let out = t.restore(false);
    let ask = &sent(&out)[0];
    assert_eq!(ask["uris"], json!(["trash:///f0"]));
    assert_eq!(ask["identities"], json!(["id0"]));
    let done = answer(&mut t, &out, json!({"done": 1, "failed": 0, "failures": []}));
    assert!(done.contains(&Out::Result("Restored 1 of 1".into(), String::new(), false)));
    assert_eq!(sent(&done)[0]["op"], "list", "a finished operation re-lists");
}

#[test]
fn deletion_goes_through_a_reviewed_confirmation_and_enter_on_cancel_deletes_nothing() {
    let mut t = opened(2);
    t.choose(1, Extend::No);
    let out = t.prepare(false);
    assert!(t.confirming());
    answer(&mut t, &out, json!({"token": 7, "all": false, "count": 1, "bytes": 10}));
    assert_eq!(t.confirmation.as_ref().map(|c| (c.token, c.destructive)), Some((7, false)));
    let out = t.confirm_activate();
    assert_eq!(sent(&out)[0]["op"], "cancel");
    assert!(sent(&t.prepare(false)).is_empty(), "nothing is prepared while the cancel is in flight");
    let relist = answer(&mut t, &out, json!({"cancelled": true}));
    let summary = answer(&mut t, &relist, json!({"total": 2, "start": 0, "rows": [item(0), item(1)], "present": ["trash:///f1"]}));
    answer(&mut t, &summary, json!({"bytes": 20, "partial": false}));
    assert!(t.is_selected("trash:///f1"), "the selection survives a re-list while its item is there");
    // Reached deliberately, the destructive button sends the delete with the reviewed token.
    let out = t.prepare(false);
    answer(&mut t, &out, json!({"token": 8, "all": false, "count": 1, "bytes": 10}));
    t.confirmation.as_mut().unwrap().destructive = true;
    let out = t.confirm_activate();
    assert_eq!((sent(&out)[0]["op"].as_str(), sent(&out)[0]["token"].as_u64()), (Some("delete"), Some(8)));
    // A failure stays selected, so the operator can try again.
    let done = answer(&mut t, &out, json!({"done": 0, "failed": 1, "failures": [{"uri": "trash:///f1", "name": "f1", "identity": "id1", "error": "busy"}]}));
    assert!(done.contains(&Out::Result("Deleted 0 of 1 · 1 failed".into(), "f1 failed: busy".into(), true)));
    assert!(t.is_selected("trash:///f1"));
}

#[test]
fn a_stale_delete_is_reviewed_again_over_the_same_ask() {
    let mut t = opened(2);
    t.choose(0, Extend::No);
    let out = t.prepare(false);
    answer(&mut t, &out, json!({"token": 3, "all": false, "count": 1, "bytes": 10}));
    t.confirmation.as_mut().unwrap().destructive = true;
    let out = t.confirm_activate();
    let ask = sent(&out).pop().unwrap();
    let again = t.receive(&json!({"id": ask["id"], "op": "delete", "ok": false, "stale": true, "error": "changed"}));
    let prep = &sent(&again)[0];
    assert_eq!((prep["op"].as_str(), prep["refreshToken"].as_u64(), prep["uris"].clone()), (Some("prepare"), Some(3), json!(["trash:///f0"])));
    assert!(t.confirming());
}

#[test]
fn select_all_is_the_backends_snapshot_and_a_row_taken_out_counts_down() {
    let mut t = opened(3);
    let out = t.select_all();
    let win = answer(&mut t, &out, json!({"selectionToken": 5, "selectionCount": 3}));
    assert_eq!(sent(&win)[0]["op"], "window");
    let rows: Vec<Value> = (0..3).map(item).collect();
    answer(&mut t, &win, json!({"total": 3, "start": 0, "rows": rows, "selectionToken": 5, "selectionCount": 3}));
    assert_eq!(t.selected_count(), 3);
    assert!(t.is_selected("trash:///f1"));
    t.choose(1, Extend::Toggle);
    assert_eq!(t.selected_count(), 2);
    let out = t.prepare(false);
    let ask = &sent(&out)[0];
    assert_eq!((ask["selectionToken"].as_u64(), ask["exclude"].clone()), (Some(5), json!(["trash:///f1"])));
}

#[test]
fn an_action_asked_from_outside_runs_after_the_listing() {
    let mut t = Trash { window_rows: 20, ..Default::default() };
    let out = t.open("emptyTrash");
    let after = answer(&mut t, &out, json!({"total": 1, "start": 0, "rows": [item(0)], "present": []}));
    let ask = &sent(&after)[0];
    assert_eq!((ask["op"].as_str(), ask["emptyTrash"].as_bool()), (Some("prepare"), Some(true)));
}

#[test]
fn closing_cancels_the_session_unless_an_operation_is_running() {
    let mut t = opened(1);
    let out = t.close();
    assert_eq!(sent(&out)[0]["clearSelection"], json!(true));
    assert!(out.contains(&Out::Closed));
    let mut t = opened(1);
    t.restore(true);
    let out = t.close();
    assert!(sent(&out).is_empty(), "the running restore still answers");
}

#[test]
fn the_dates_and_locations_read_as_the_board_draws_them() {
    assert_eq!(location("/home/u/docs/a.txt", "/home/u"), "~/docs");
    assert_eq!(location("/home/u/a.txt", "/home/u"), "~");
    assert_eq!(location("/a.txt", "/home/u"), "/");
    assert_eq!(location("/home/users/a", "/home/u"), "/home/users");
    assert_eq!(location("nothing", "/home/u"), "Unknown");
    assert_eq!(deleted("2026-09-08T10:00:00"), "2026-09-08 10:00");
    assert_eq!(deleted(""), "Unknown");
    assert_eq!(name("/home/u/a.txt"), "a.txt");
}

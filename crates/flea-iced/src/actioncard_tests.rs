use super::*;

fn sent(out: &[Out]) -> Vec<Value> {
    out.iter().filter_map(|o| if let Out::Send(v) = o { Some(v.clone()) } else { None }).collect()
}

fn ops(out: &[Out]) -> Vec<String> {
    sent(out).iter().map(|v| format!("{}/{}", v["c"].as_str().unwrap(), v["op"].as_str().unwrap_or(""))).collect()
}

fn reply(id: u64, op: &str, more: Value) -> Value {
    let mut v = json!({"t": "menuaction", "id": id, "op": op, "ok": true});
    v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    v
}

#[test]
fn properties_snapshots_the_rows_then_asks_and_draws_the_answer() {
    let (mut c, out) = Card::open(Kind::Properties, 4, "/d", &[2], 2, "/d/a");
    assert_eq!(sent(&out), vec![json!({"c": "menuaction", "op": "snapshot", "id": 4, "rows": [2], "cursor": 2})]);
    assert_eq!(ops(&c.receive_menu(&reply(4, "snapshot", json!({"count": 1})))), ["menuaction/properties"]);
    assert_eq!(c.phase, Phase::Busy);
    c.receive_menu(&reply(4, "properties", json!({"path": "/d/a", "kind": "File", "bytes": 12})));
    assert_eq!((c.phase, c.facts["kind"].as_str()), (Phase::Ready, Some("File")));
    let out = c.close();
    assert_eq!(ops(&out), ["menuaction/close"]);
    assert_eq!(out.last(), Some(&Out::Close));
}

#[test]
fn a_reply_for_another_request_or_a_refused_snapshot_is_handled_as_qml_does() {
    let (mut c, _) = Card::open(Kind::Properties, 4, "/d", &[0], 0, "/d/a");
    assert!(c.receive_menu(&reply(3, "snapshot", json!({}))).is_empty());
    let refused = json!({"t": "menuaction", "id": 4, "op": "snapshot", "ok": false, "error": "Selected items changed; reopen the menu."});
    assert_eq!(c.receive_menu(&refused), vec![Out::Alert("Selected items changed; reopen the menu.".into()), Out::Close]);
}

#[test]
fn new_file_needs_no_snapshot_refuses_a_path_and_lands_on_the_file_it_made() {
    let (mut c, out) = Card::open(Kind::NewFile, 5, "/d", &[], 0, "");
    assert!(out.is_empty());
    assert_eq!(c.text, "New File");
    c.input("a/b".into());
    assert!(c.submit().is_empty());
    assert_eq!(c.error, "Enter one filename without a path separator.");
    c.input("notes.txt".into());
    assert_eq!(sent(&c.submit()), vec![json!({"c": "newfile", "op": "newFile", "id": 5, "path": "/d", "name": "notes.txt"})]);
    // A file being created cannot be dismissed out from under its answer.
    assert!(c.close().is_empty());
    let out = c.receive_menu(&reply(5, "newFile", json!({"path": "/d/notes.txt"})));
    assert_eq!(out[..2], [Out::Say("File created.".into()), Out::Refresh("notes.txt".into())]);
    assert_eq!(out.last(), Some(&Out::Close));
}

#[test]
fn move_to_validates_the_destination_then_moves_the_captured_sources() {
    let (mut c, _) = Card::open(Kind::MoveTo, 6, "/d", &[1, 3], 1, "/d/b");
    assert_eq!(c.text, "/d");
    c.receive_menu(&reply(6, "snapshot", json!({"count": 2})));
    c.input("relative".into());
    assert!(c.submit().is_empty());
    assert_eq!(c.error, "Enter an absolute destination folder.");
    c.input("/e".into());
    assert_eq!(sent(&c.submit()), vec![json!({"c": "menuaction", "op": "validate", "id": 6, "action": "moveTo", "dest": "/e"})]);
    let out = c.receive_menu(&reply(6, "validate", json!({"action": "moveTo", "paths": ["/d/b", "/d/c"], "dest": "/e"})));
    // The transfer goes out before the close that expires the selection it names.
    assert_eq!(ops(&out), ["transfer/move", "menuaction/close"]);
    assert_eq!(sent(&out)[0], json!({"c": "transfer", "op": "move", "menuId": 6, "dest": "/e"}));
}

#[test]
fn delete_reviews_confirms_with_its_token_and_reports_partial_completion() {
    let (mut c, _) = Card::open(Kind::Delete, 7, "/d", &[0], 0, "/d/a");
    assert_eq!(ops(&c.receive_menu(&reply(7, "snapshot", json!({"count": 1})))), ["menuaction/prepareDelete"]);
    // Delete is not reachable before the review answers.
    assert!(c.confirm().is_empty());
    let out = c.receive_menu(&reply(7, "prepareDelete", json!({"token": 12, "count": 2, "bytes": 40})));
    assert_eq!((c.phase, c.focus), (Phase::Confirm, 0));
    assert_eq!(out, vec![Out::Check(1)]);
    assert_eq!(sent(&c.confirm()), vec![json!({"c": "menuaction", "op": "delete", "id": 7, "token": 12})]);
    assert!(!c.visible());
    let out = c.receive_menu(&reply(7, "delete", json!({"deleted": 1, "failed": 1, "cancelled": false, "error": "a failed", "remaining": ["/d/b"]})));
    assert_eq!(out[0], Out::Alert("Deleted 1 of 2 · 1 failed · a failed".into()));
    assert_eq!(out[1], Out::Refresh(String::new()));
    assert_eq!(ops(&out), ["menuaction/close"]);
}

#[test]
fn a_changed_set_is_reviewed_again_and_an_old_check_is_ignored() {
    let (mut c, _) = Card::open(Kind::Delete, 7, "/d", &[0], 0, "/d/a");
    c.receive_menu(&reply(7, "snapshot", json!({})));
    c.receive_menu(&reply(7, "prepareDelete", json!({"token": 12, "count": 1, "bytes": 4})));
    assert_eq!(ops(&c.check_due(1)), ["menuaction/checkDelete"]);
    assert_eq!(c.receive_menu(&reply(7, "checkDelete", json!({"valid": true}))), vec![Out::Check(2)]);
    assert!(c.check_due(1).is_empty());
    assert_eq!(ops(&c.receive_menu(&reply(7, "checkDelete", json!({"valid": false})))), ["menuaction/refreshDelete"]);
    assert_eq!(c.phase, Phase::Busy);
    c.receive_menu(&reply(7, "refreshDelete", json!({"token": 13, "count": 1, "bytes": 4})));
    assert_eq!(c.facts["token"], 13);
    // A stale delete is a fresh review, never a deletion.
    c.confirm();
    assert_eq!(ops(&c.receive_menu(&reply(7, "delete", json!({"stale": true, "error": "changed"})))), ["menuaction/refreshDelete"]);
}

#[test]
fn permissions_inspects_the_path_flips_bits_and_applies_the_typed_mode() {
    let (mut c, out) = Card::open(Kind::Permissions, 8, "/d", &[], 0, "/d/a");
    assert_eq!(sent(&out), vec![json!({"c": "permissions", "op": "inspect", "id": 8, "path": "/d/a"})]);
    assert!(c.receive_menu(&reply(8, "properties", json!({}))).is_empty());
    c.receive_permissions(&json!({"t": "permissions", "id": 8, "op": "inspect", "ok": true, "mode": "0644", "directory": false, "reason": ""}));
    assert_eq!(c.text, "0644");
    c.toggle_bit(0o100);
    assert_eq!(c.text, "0744");
    c.input("9".into());
    assert!(!c.can_submit());
    c.input("750".into());
    assert_eq!(sent(&c.submit()), vec![json!({"c": "permissions", "op": "apply", "id": 8, "mode": "750"})]);
    assert!(c.close().is_empty());
    let out = c.receive_permissions(&json!({"t": "permissions", "id": 8, "op": "apply", "ok": true, "mode": "0750"}));
    assert_eq!(out[..2], [Out::Say("Permissions changed.".into()), Out::Refresh(String::new())]);
    assert_eq!(ops(&out), ["permissions/close"]);
}

#[test]
fn a_read_only_reason_keeps_the_mode_from_being_edited() {
    let (mut c, _) = Card::open(Kind::Permissions, 8, "/d", &[], 0, "/d/a");
    c.receive_permissions(&json!({"t": "permissions", "id": 8, "op": "inspect", "ok": true, "mode": "0644", "reason": "Read-only: setuid bit is present."}));
    c.toggle_bit(0o100);
    assert_eq!(c.text, "0644");
    assert!(!c.can_submit());
}

#[test]
fn the_octal_grammar_is_permissions_js() {
    assert_eq!(parse_mode("644"), Some(0o644));
    assert_eq!(parse_mode("0644"), Some(0o644));
    for bad in ["", "64", "1644", "00644", "648", "6440", "-644"] {
        assert_eq!(parse_mode(bad), None, "{bad}");
    }
    assert_eq!(octal(0o7), "0007");
}

use super::*;

fn app(id: &str, label: &str, default: bool) -> App {
    App { id: id.into(), label: label.into(), default }
}

fn answered() -> OpenWith {
    let (mut c, _) = OpenWith::open(3, &[0], 0);
    c.receive(&json!({"id": 3, "op": "snapshot", "ok": true}));
    c.receive(&json!({"id": 3, "op": "applications", "ok": true, "path": "/d/a.txt", "kind": "Plain text document",
        "applications": [{"id": "nvim.desktop", "label": "Neovim", "default": true}, {"id": "kate.desktop", "label": "Kate", "default": false}],
        "installed": [{"id": "gimp.desktop", "label": "GIMP"}, {"id": "kate.desktop", "label": "Kate"}, {"id": "nvim.desktop", "label": "Neovim"}]}));
    c
}

#[test]
fn the_registry_heads_the_list_and_the_search_filters_both_groups() {
    let handlers = [app("nvim.desktop", "Neovim", true), app("kate.desktop", "Kate", false)];
    let installed = [app("gimp.desktop", "GIMP", false), app("kate.desktop", "Kate", false)];
    let all = rows(&handlers, &installed, "Plain text document", "");
    assert_eq!(all[0], Row::Eyebrow("Registered for Plain text document".into()));
    assert_eq!(all[3], Row::Eyebrow("All applications".into()));
    assert_eq!(all[5], Row::App(app("kate.desktop", "Kate", false), 3));
    // A group that matches nothing takes its eyebrow with it.
    assert_eq!(rows(&handlers, &installed, "x", " GI "), vec![Row::Eyebrow("All applications".into()), Row::App(app("gimp.desktop", "GIMP", false), 0)]);
    assert!(rows(&handlers, &installed, "x", "zzz").is_empty());
    assert_eq!(no_match(" zzz "), "No application matches “zzz”. Clear the search to see every installed application.");
}

#[test]
fn it_snapshots_then_reads_the_catalogue_and_launches_the_chosen_one() {
    let (mut c, snapshot) = OpenWith::open(3, &[2], 2);
    assert_eq!(snapshot["op"], "snapshot");
    assert!(!c.can_submit());
    let out = c.receive(&json!({"id": 3, "op": "snapshot", "ok": true}));
    assert_eq!(out, vec![Out::Send(json!({"c": "menuaction", "op": "applications", "id": 3, "installed": true}))]);
    let mut c = answered();
    assert_eq!((c.name(), c.chosen().unwrap().id), ("a.txt".to_string(), "nvim.desktop".to_string()));
    // Neovim and Kate are seats 0 and 1, so GIMP, the first of All applications, is two down.
    c.move_cursor(2);
    c.always = true;
    assert_eq!(c.commit(), vec![Out::Send(json!({"c": "menuaction", "op": "openWith", "id": 3, "application": "gimp.desktop", "always": true}))]);
    assert!(c.commit().is_empty());
    let out = c.receive(&json!({"id": 3, "op": "openWith", "ok": true, "path": "/d/a.txt"}));
    assert_eq!(out, vec![Out::Send(json!({"c": "menuaction", "op": "close", "id": 3})), Out::Close]);
}

#[test]
fn a_refused_launch_says_why_and_a_cancelled_one_says_nothing() {
    let mut c = answered();
    c.commit();
    assert!(c.receive(&json!({"id": 3, "op": "openWith", "ok": false, "error": "Opened it, but the default was not saved: x"})).is_empty());
    assert_eq!(c.error, "Opened it, but the default was not saved: x");
    c.error.clear();
    c.commit();
    c.receive(&json!({"id": 3, "op": "openWith", "ok": false, "cancelled": true, "error": "cancelled"}));
    assert!(c.error.is_empty() && !c.busy);
}

#[test]
fn the_cursor_holds_at_the_ends_and_a_search_puts_it_back_on_top() {
    let mut c = answered();
    c.move_cursor(-5);
    assert_eq!(c.cursor, 0);
    c.move_cursor(50);
    assert_eq!(c.cursor, 4);
    c.search("kate".into());
    assert_eq!((c.cursor, c.applications().len()), (0, 2));
    c.focus = 4;
    c.step_focus(false);
    assert_eq!(c.focus, 0);
}

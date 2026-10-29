use super::*;

fn facts() -> Facts {
    Facts { home: "/home/u".into(), base_size: 14, monitor_scale: 0.0, version: String::new(), handler: String::new(), save_status: String::new() }
}

#[test]
fn places_switches_read_their_own_defaults() {
    let r = rows(&json!({ "places": { "showTrash": false, "showUnmounted": true, "rail": "hidden", "sidebarWidth": 230 } }), &facts());
    let get = |id: &str| r.iter().find(|x| x.id == id).unwrap().clone();
    assert!(!get("places.showTrash").on && get("places.showHome").on);
    assert!(get("places.showUnmounted").on && !get("places.driveSize").on);
    assert!(!get("places.rail").on);
    assert_eq!(get("places.sidebarWidth").value, "224 px");
}

#[test]
fn a_menu_master_turns_its_group_on_and_off_and_carries_the_shelf_with_it() {
    let data = json!({ "menu": { "hidden": ["copy", "delete"] }, "shelf": { "enabled": false } });
    let basic = menu_rows(&data).into_iter().find(|r| r.id == "basic").unwrap();
    assert_eq!((basic.state, basic.value.as_str()), ("some", "5 of 6"));
    // The master on a partly-on group turns all of it on.
    assert_eq!(menu_patch(&data, &BASIC, true), json!({ "menu": { "hidden": ["delete"] } }));
    // A single row toggles alone, and Delete stays hidden by default.
    assert_eq!(menu_patch(&data, &["delete"], false), json!({ "menu": { "hidden": ["copy"] } }));
    let extras = GROUPS[3].2;
    let on = menu_patch(&data, extras, true);
    assert_eq!(on["shelf"], json!({ "enabled": true }));
    // The shelf is never written into the hidden set.
    assert!(!on["menu"]["hidden"].as_array().unwrap().iter().any(|v| v == "shelf"));
    assert_eq!(menu_patch(&data, &["shelf"], false), json!({ "shelf": { "enabled": true } }));
}

#[test]
fn display_follows_omarchy_until_overridden_and_names_the_nearest_stop() {
    let follow = display_rows(&json!({}), &Facts { base_size: 13, ..facts() });
    assert_eq!(follow[1].value, "Follow Omarchy");
    assert_eq!(follow.iter().find(|r| r.label == "Effective").unwrap().caption, "Omarchy's own size. The ruler marks 12, the nearest stop.");
    let pinned = display_rows(&json!({ "display": { "textSize": { "mode": 16 } } }), &facts());
    assert_eq!(pinned[1].value, "Override");
    assert_eq!(pinned.iter().find(|r| r.label == "Effective").unwrap().value, "16 px");
    assert_eq!(nearest(13), 12);
    assert_eq!(nearest(18), 16);
}

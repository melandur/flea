use super::*;

fn facts() -> Facts {
    Facts { home: "/home/u".into(), base_size: 14, monitor_scale: 1.25, version: "flea 0.3.8".into(), handler: "flea".into(), save_status: String::new() }
}

#[test]
fn every_section_draws_rows_and_the_cursor_skips_what_it_cannot_change() {
    for (id, _, _) in SECTIONS {
        let r = rows(id, &json!({}), &facts());
        assert!(!r.is_empty(), "{id} drew nothing");
    }
    let view = rows("view", &json!({}), &facts());
    // The heading is not focusable, so the cursor starts on the first control.
    assert_eq!(view[first_row(&view)].id, "view");
    let next = step_row(&view, first_row(&view), 1);
    assert_eq!(view[next].id, "density");
    // A step past the last control stays where it is.
    let last = (0..view.len()).rev().find(|i| focusable(&view[*i])).unwrap();
    assert_eq!(step_row(&view, last, 1), last);
}

#[test]
fn a_choice_names_its_stored_value_and_an_unknown_one_reads_as_the_first() {
    let view = rows("view", &json!({ "sort": { "key": "date", "reverse": true }, "density": "bogus" }), &facts());
    let by = view.iter().find(|r| r.id == "sort.key").unwrap();
    assert_eq!((by.value.as_str(), by.selected), ("Date", 2));
    assert_eq!(view.iter().find(|r| r.id == "sort.reverse").unwrap().value, "Descending");
    assert_eq!(view.iter().find(|r| r.id == "density").unwrap().value, "Compact");
}

#[test]
fn the_write_rule_is_view_state_change_setting() {
    assert_eq!(patch("hidden", json!(true)), json!({ "hidden": true }));
    assert_eq!(patch("sort.key", json!("size")), json!({ "sort": { "key": "size" } }));
    assert_eq!(patch("places.rail", json!(false)), json!({ "places": { "rail": "hidden" } }));
    assert_eq!(patch("theme:catppuccin-mocha", json!(true)), json!({ "display": { "theme": "catppuccin-mocha" } }));
}

#[test]
fn the_theme_section_lists_the_catalog_and_marks_the_one_in_force() {
    let t = rows("theme", &json!({ "display": { "theme": "3024" } }), &facts());
    assert!(!t.iter().find(|r| r.id == "theme:omarchy").unwrap().on);
    assert!(t.iter().find(|r| r.id == "theme:3024").unwrap().on);
    assert_eq!(t.iter().filter(|r| r.id.starts_with("theme:") && r.id != "theme:omarchy").count(), 95);
    // An id this catalog lost follows the desktop.
    let lost = rows("theme", &json!({ "display": { "theme": "gone-theme" } }), &facts());
    assert!(lost.iter().find(|r| r.id == "theme:omarchy").unwrap().on);
}

use super::*;

fn ids(items: &[Item]) -> Vec<&'static str> {
    items.iter().map(|i| match i {
        Item::Separator => "|",
        Item::Row(e) => e.id,
    }).collect()
}

fn row<'a>(items: &'a [Item], id: &str) -> &'a Entry {
    items.iter().find_map(|i| match i {
        Item::Row(e) if e.id == id => Some(e),
        _ => None,
    }).unwrap_or_else(|| panic!("no {id} row"))
}

fn file(targets: usize, mode: u32) -> Facts {
    Facts { path: "/home/u".into(), targets, row_mode: mode, clipboard: false, show_hidden: false }
}

// uischema.rs DEFAULT's menu.hidden, the set a first run reads.
fn shipped() -> Vec<String> {
    ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copypath"].map(String::from).to_vec()
}

#[test]
fn a_file_menu_under_the_shipped_hidden_set_has_the_rows_and_separators_the_qml_draws() {
    let items = entries(Kind::File, &file(1, 0o100644), &shipped());
    assert_eq!(ids(&items), ["open", "openwith", "|", "cut", "copy", "paste", "duplicate", "rename", "|", "trash", "|", "addFavourite", "|", "toggleHidden"]);
}

#[test]
fn the_background_menu_carries_its_own_rows_and_no_file_rows() {
    let items = entries(Kind::Background, &file(0, 0), &[]);
    assert_eq!(
        ids(&items),
        ["newFolder", "newFile", "|", "paste", "selectAll", "|", "openTerminal", "addFavourite", "|", "sort", "toggleHidden", "|", "settings"]
    );
    assert_eq!(row(&items, "sort").sub, SORTS.to_vec());
}

#[test]
fn a_hidden_group_leaves_no_doubled_or_trailing_separator() {
    let hidden: Vec<String> = ["trash", "addFavourite", "settings"].map(String::from).to_vec();
    let items = entries(Kind::Background, &file(0, 0), &hidden);
    assert!(!items.windows(2).any(|w| w == [Item::Separator, Item::Separator]));
    assert_ne!(items.last(), Some(&Item::Separator));
    assert_ne!(items.first(), Some(&Item::Separator));
}

#[test]
fn open_and_the_hidden_toggle_survive_a_state_file_that_hides_them() {
    let hidden: Vec<String> = ["open", "toggleHidden"].map(String::from).to_vec();
    let items = entries(Kind::File, &file(1, 0o100644), &hidden);
    row(&items, "open");
    row(&items, "toggleHidden");
}

#[test]
fn the_single_row_actions_grey_out_over_a_selection_of_two() {
    let items = entries(Kind::File, &file(2, 0o100644), &[]);
    for id in ["duplicate", "rename", "properties", "openwith", "permissions"] {
        assert!(row(&items, id).disabled, "{id}");
    }
    assert_eq!(row(&items, "permissions").hint, "Unavailable");
    assert!(!row(&items, "copy").disabled);
}

#[test]
fn a_favourite_is_only_a_folder_and_permissions_never_follow_a_link() {
    let on_file = entries(Kind::File, &file(1, 0o100644), &[]);
    assert!(row(&on_file, "addFavourite").disabled);
    let on_dir = entries(Kind::File, &file(1, 0o040755), &[]);
    assert!(!row(&on_dir, "addFavourite").disabled);
    let on_link = entries(Kind::File, &file(1, 0o120777), &[]);
    assert!(row(&on_link, "permissions").disabled);
    assert_eq!(row(&on_link, "permissions").hint, "Symlink target not changed");
}

#[test]
fn paste_follows_the_clipboard_and_the_hidden_row_flips_its_label() {
    let mut f = file(0, 0);
    assert!(row(&entries(Kind::Background, &f, &[]), "paste").disabled);
    f.clipboard = true;
    f.show_hidden = true;
    let items = entries(Kind::Background, &f, &[]);
    assert!(!row(&items, "paste").disabled);
    assert_eq!((row(&items, "toggleHidden").label, row(&items, "toggleHidden").glyph), ("Hide hidden files", "eye-off"));
}

#[test]
fn a_camera_mount_offers_no_trash_and_the_two_destroyers_are_marked() {
    let mut f = file(1, 0o100644);
    f.path = "/run/user/1000/gvfs/mtp:host=Pixel/DCIM".into();
    assert!(!ids(&entries(Kind::File, &f, &[])).contains(&"trash"));
    let items = entries(Kind::File, &file(1, 0o100644), &[]);
    assert!(row(&items, "trash").danger && row(&items, "delete").danger && !row(&items, "copy").danger);
}

#[test]
fn the_capability_probed_rows_are_absent_until_something_probes_them() {
    let items = ids(&entries(Kind::File, &file(1, 0o100644), &[]));
    for id in ["compress", "extract", "convert", "runScript", "shelf", "taildrop", "localsend", "dropbox", "sharelink"] {
        assert!(!items.contains(&id), "{id}");
    }
}

#[test]
fn every_glyph_is_a_mark_the_table_carries() {
    let fallback = crate::marks::path_for("\u{0}not a mark");
    for &(id, _, glyph, ..) in INVENTORY {
        // The three reproduced brand marks are their own components (TailscaleMark, DropboxMark),
        // never PATHS rows, and their rows are absent until stage 8c draws them.
        if ["tailscale", "localsend", "dropbox"].contains(&glyph) {
            continue;
        }
        assert!(crate::marks::path_for(glyph) != fallback || glyph == "file", "{id} draws {glyph}");
    }
    assert!(crate::marks::path_for("eye-off") != fallback);
}

#[test]
fn the_cursor_skips_separators_and_greyed_rows_and_holds_at_the_ends() {
    let items = entries(Kind::File, &file(2, 0o100644), &shipped());
    // open, openwith(grey), |, cut ...
    assert_eq!(first(&items), Some(0));
    assert_eq!(step(&items, 0, 1), 3);
    assert_eq!(step(&items, 3, -1), 0);
    assert_eq!(step(&items, 0, -1), 0);
    let last = items.len() - 1;
    assert_eq!(step(&items, last, 1), last);
}

#[test]
fn a_frame_flips_at_the_far_edge_and_shifts_only_when_neither_side_fits() {
    assert_eq!(clamp(100.0, 50.0, 400.0), 100.0);
    assert_eq!(clamp(380.0, 50.0, 400.0), 330.0);
    assert_eq!(clamp(55.0, 50.0, 60.0), 5.0);
    assert_eq!(clamp(20.0, 50.0, 60.0), 0.0);
    assert_eq!(clamp(10.0, 500.0, 400.0), 0.0);
}

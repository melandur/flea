use super::*;

#[test]
fn the_longest_extension_wins_and_a_dotfile_is_a_name() {
    // d.ts is a compound key in the table; ts alone is code as well, so test via the name path.
    assert_eq!(extensions("a.d.ts").collect::<Vec<_>>(), vec!["d.ts", "ts"]);
    assert_eq!(extensions(".bashrc").count(), 0);
    assert_eq!(mark_for(".bashrc", false), Some("terminal"));
    assert_eq!(mark_for("Cargo.lock", false), Some("lock"));
    assert_eq!(mark_for("photo.JPG", false), Some("image"));
    assert_eq!(mark_for("brain.nii.gz", false), Some("archive"));
    assert_eq!(mark_for("anything", true), None);
}

#[test]
fn a_link_keeps_the_link_mark_and_the_tier_only_applies_when_on() {
    assert_eq!(glyph_for_row("text-x-generic", 0o120777, "a.rs", false, true), "symlink");
    assert_eq!(glyph_for_row("text-x-generic", 0o100644, "a.rs", false, false), "file-text");
    assert_eq!(glyph_for_row("text-x-generic", 0o100644, "a.rs", false, true), "code");
}

#[test]
fn every_mark_draws_something_other_than_the_file_mark() {
    for mark in ["folder", "file-text", "code", "image", "film", "music", "archive", "type", "terminal", "table", "presentation", "symlink", "lock", "braces"] {
        assert!(!path_for(mark).is_empty());
        assert!(mark == FALLBACK || path_for(mark) != path_for(FALLBACK), "{mark} falls back to the file mark");
    }
}

#[test]
fn colours_follow_the_vendored_table() {
    // go is #00add8 in vendor/yazi-icons.toml; Downloads is one of the #00bcd4 directories.
    assert_eq!(row_colour("main.go", 0o100644, false), hex("#00add8"));
    assert_eq!(row_colour("Downloads", 0o040755, true), hex("#00bcd4"));
    assert_eq!(row_colour("main.go", 0o120777, false), None);
}

#[test]
fn hyprland_aware_icons_round_the_caps_and_joins_and_the_omarchy_cut_squares_them() {
    let square = svg_doc("file", [1, 2, 3], false);
    assert!(square.contains(r#"stroke-linecap="square""#) && square.contains(r#"stroke-linejoin="miter""#) && square.contains("#010203"));
    let round = svg_doc("file", [1, 2, 3], true);
    assert!(round.contains(r#"stroke-linecap="round""#) && round.contains(r#"stroke-linejoin="round""#));
}

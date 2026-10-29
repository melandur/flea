use super::*;

fn row(d: bool, p: u32, i: &str) -> Row {
    Row { n: "x".into(), d, s: 0, m: 0, p, i: i.into(), t: false, k: 0, l: None }
}

// The cases tests/js/picker.js drives, so the two ports cannot drift.
#[test]
fn a_request_is_read_with_every_field_defaulted() {
    let asked = r#"{"mode":"open","title":"Send to unraid","app":"","accept":"Send","multiple":true,
        "directory":false,"folder":"/home/gm/Pictures","file":"","name":"","files":[],
        "filters":[{"label":"Images","globs":["*.png","*.jpg"],"mimes":[]},{"label":"Any text","globs":[],"mimes":["text/plain"]}],
        "current":"Images"}"#;
    let req = request(asked);
    assert_eq!((req.mode, req.multiple, req.folder.as_str()), (Mode::Open, true, "/home/gm/Pictures"));
    assert_eq!(req.filters[0].globs, vec!["*.png", "*.jpg"]);
    let empty = request("not json");
    assert_eq!((empty.mode, empty.filters.len(), empty.multiple), (Mode::Open, 0, false));
    assert_eq!(request(r#"{"mode":"nonsense"}"#).mode, Mode::Open);

    assert_eq!(title(&req), "Send to unraid");
    assert_eq!(title(&request("{}")), "Choose file");
    assert_eq!(title(&request(r#"{"directory":true}"#)), "Choose folder");
    assert_eq!(title(&request(r#"{"mode":"save"}"#)), "Save file");
    assert_eq!(subtitle(&req), "");
    assert_eq!(subtitle(&request(r#"{"app":"org.gnome.gedit"}"#)), "Requested by org.gnome.gedit");

    assert_eq!(accept_label(&req, 3), "Send 3");
    assert_eq!(accept_label(&req, 1), "Send");
    assert_eq!(accept_label(&request("{}"), 0), "Open");
    assert_eq!(accept_label(&request(r#"{"directory":true}"#), 0), "Choose folder");

    assert_eq!(status_line(0, 0), "0 selected");
    assert_eq!(status_line(3, 2_100_000), "3 selected · 2.1 MB");
    assert_eq!(hints(&req), "Space select · Enter open/send · Esc cancel");
    assert_eq!(hints(&request(r#"{"mode":"save"}"#)), "Enter save · Esc cancel");

    let c = chips(&req);
    assert_eq!(c.len(), 3);
    assert_eq!(c[2], ("All files".to_string(), -1));
    assert!(chips(&request("{}")).is_empty());
    assert_eq!(current_chip(&req), 0);
    assert_eq!(current_chip(&request(r#"{"filters":[{"label":"A"}],"current":"Z"}"#)), 0);
    assert_eq!(current_chip(&request("{}")), -1);
    assert_eq!(current_chip(&request(r#"{"filters":[{"label":"A"},{"label":"A"}],"current":"A","currentIndex":1}"#)), 1);
}

#[test]
fn recent_rows_and_directories_are_told_apart() {
    assert!(!RECENT.starts_with('/'));
    assert_eq!(row_path("/home/gm", "a.png"), "/home/gm/a.png");
    assert_eq!(row_path(RECENT, "home/gm/Pictures/a.png"), "/home/gm/Pictures/a.png");
    assert_eq!(row_path("/", "etc"), "/etc");
    assert!(is_directory(&row(false, 0o120777, "folder")));
    assert!(!is_directory(&row(false, 0o100644, "folder")));
    assert_eq!(parent_of("/home/gm/a.txt"), "/home/gm");
    assert_eq!(parent_of("/"), "/");
    assert_eq!(join("/", "etc"), "/etc");
}

#[test]
fn marks_keep_their_first_uri_and_the_reply_matches_the_js_bytes() {
    let marks = reviewed_marks(&[], &[("/x/a #.png".into(), 10), ("/x/b.png".into(), 20)]);
    assert_eq!(total_bytes(&marks), 30);
    assert_eq!(marks[0].uri, "file:///x/a%20%23.png");
    let marks = reviewed_marks(&marks, &[("/x/a #.png".into(), 15)]);
    assert_eq!((marks.len(), marks[0].bytes), (1, 15));
    let uris: Vec<String> = marks.iter().map(|m| m.uri.clone()).collect();
    assert_eq!(reply(0, &uris, 1), r#"{"response":0,"uris":["file:///x/a%20%23.png"],"filter":1}"#);
    assert_eq!(reply(1, &uris, 1), r#"{"response":1}"#);
    assert_eq!(file_uri("/home/gm/my file.txt"), "file:///home/gm/my%20file.txt");
    assert_eq!(file_uri("/home/gm/a#b?.txt"), "file:///home/gm/a%23b%3F.txt");
    // encodeURI escapes UTF-8 a byte at a time, upper-case hex, and a literal percent too.
    assert_eq!(file_uri("/é%"), "file:///%C3%A9%25");
}

#[test]
fn a_save_name_is_held_to_the_backends_own_rule() {
    for ok in ["ordinary.txt", ".bashrc", "a name with spaces"] {
        assert!(valid_name(ok), "{ok}");
    }
    for bad in ["", ".", "..", "../escape", "/etc/passwd", "sub/child", "../../.config/autostart/pwn.desktop", "nul\0byte"] {
        assert!(!valid_name(bad), "{bad:?}");
    }
    // tools/flea-portal passes current_name verbatim, so the window is what refuses it.
    assert_eq!(request(r#"{"name":"../../pwn"}"#).name, "../../pwn");
}

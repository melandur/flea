use super::*;
use serde_json::json;

const DIRS: &str = r#"# comment
XDG_DESKTOP_DIR="$HOME/"
XDG_DOWNLOAD_DIR="$HOME/Downloads"
XDG_PICTURES_DIR="$HOME/Pictures/"
XDG_MUSIC_DIR="/srv/music"
"#;

#[test]
fn user_dirs_skip_home_and_expand_home() {
    let d = user_dirs(DIRS, "/home/u");
    assert_eq!(d, vec![
        ("/home/u/Downloads".into(), "Downloads".into()),
        ("/home/u/Pictures".into(), "Pictures".into()),
        ("/srv/music".into(), "music".into()),
    ]);
}

#[test]
fn the_groups_come_in_order_and_a_bad_favourite_says_why() {
    let favs = vec![json!({"label": "Work", "path": "~/work"}), json!({"label": "", "path": "/x"}), json!({"label": "Rel", "path": "rel"})];
    let e = entries(&Inputs { home: "/home/u", user_dirs: DIRS, favourites: &favs, devices: &[], shares: &[], show_network: true, show_trash: true, show_devices: true });
    let labels: Vec<_> = e.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, vec!["Home", "Downloads", "Pictures", "music", "Trash", "Work", "Invalid favorite", "Rel"]);
    assert_eq!(e[5].path, "/home/u/work");
    assert_eq!(activate(&e[5]), Activation::Open("/home/u/work".into()));
    assert_eq!(activate(&e[6]), Activation::Say("Could not open Invalid favorite · label needs visible text".into(), true));
    assert_eq!(activate(&e[7]), Activation::Say("Could not open Rel · path must be absolute, ~/ relative, or a supported URI".into(), true));
    assert_eq!(e[1].glyph, "download");
}

#[test]
fn a_file_uri_favourite_opens_its_decoded_path() {
    let favs = vec![json!({"label": "Sp", "path": "file:///home/u/a%20b"})];
    let e = entries(&Inputs { home: "/home/u", user_dirs: "", favourites: &favs, devices: &[], shares: &[], show_network: true, show_trash: false, show_devices: false });
    assert_eq!(activate(&e[1]), Activation::Open("/home/u/a b".into()));
}

#[test]
fn a_malformed_escape_is_the_raw_text() {
    assert_eq!(decode_path("/a%20b"), "/a b");
    assert_eq!(decode_path("/a%+1b"), "/a%+1b");
    assert_eq!(decode_path("/a%2"), "/a%2");
    assert_eq!(decode_path("/%ff"), "/%ff");
}

#[test]
fn the_menu_offers_eject_only_where_it_can_be_pulled() {
    let mut d = Device { kind: "volume", label: "S".into(), device: "/dev/sda1".into(), path: "/m".into(), mounted: true, removable: true, size: None, volume_menu: false };
    assert_eq!(menu(&d), vec![Action::Eject]);
    d.removable = false;
    assert!(menu(&d).is_empty());
    d.volume_menu = true;
    assert_eq!(menu(&d), vec![Action::OpenVolume, Action::UnmountVolume]);
    d.mounted = false;
    assert_eq!(menu(&d), vec![Action::MountVolume]);
}

#[test]
fn the_width_snaps_to_its_stops() {
    assert_eq!(width(None), 192.0);
    assert_eq!(width(Some(200.0)), 192.0);
    assert_eq!(width(Some(250.0)), 256.0);
}

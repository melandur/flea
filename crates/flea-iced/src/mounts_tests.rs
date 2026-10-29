use super::*;

// The shape `gio mount -li` prints under LC_ALL=C, with the indented detail lines -i adds.
const LISTING: &str = "Drive(0): Samsung SSD\n  Type: GProxyDrive\nMount(0): data on nas -> smb://nas/data/\n  Type: GProxyShadowMount\nMount(1): Pixel 7 -> mtp://Google_Pixel_7/\nMount(2): home -> file:///home\nMount(3): user on host -> sftp://user@host/\n";

#[test]
fn a_listing_keeps_network_mounts_and_names_them_by_share() {
    assert_eq!(parse_mounts(LISTING), vec![
        Mount { label: "data".into(), uri: "smb://nas/data/".into() },
        Mount { label: "user".into(), uri: "sftp://user@host/".into() },
    ]);
}

#[test]
fn a_mount_with_an_arrow_in_its_label_keeps_it() {
    assert_eq!(parse_mounts("Mount(0): a -> b -> smb://h/s/\n")[0].label, "a -> b");
    assert!(parse_mounts("Mount(x): a -> smb://h/\n").is_empty());
}

#[test]
fn local_path_reads_only_its_own_line() {
    assert_eq!(local_path("display name: data\nlocal path: /run/user/1000/gvfs/smb-share:server=nas,share=data\n"),
               "/run/user/1000/gvfs/smb-share:server=nas,share=data");
    assert_eq!(local_path("name: x\n"), "");
}

#[test]
fn normalize_makes_a_trailing_slash_and_a_default_port_one_place() {
    assert_eq!(normalize("smb://h/data"), normalize("smb://h/data/"));
    assert_eq!(normalize("smb://h:445/data/"), "smb://h/data");
    assert_eq!(normalize("smb://192.168.1.10"), "smb://192.168.1.10/");
    assert_eq!(normalize("smb://192.168.1.10///"), "smb://192.168.1.10/");
}

#[test]
fn bookmarks_skip_local_folders_and_label_by_leaf() {
    let b = bookmarks("file:///home/u/Work Work\nsmb://nas/data/ NAS data\n  sftp://u@h/home/u\nnot a uri\n");
    assert_eq!(b, vec![
        Bookmark { uri: "smb://nas/data/".into(), label: "NAS data".into() },
        Bookmark { uri: "sftp://u@h/home/u".into(), label: "u".into() },
    ]);
}

#[test]
fn bare_roots_and_credentials_are_read_off_the_authority() {
    assert!(is_bare_root("smb://192.168.1.10/") && is_bare_root("smb://h"));
    assert!(!is_bare_root("smb://h/share/"));
    assert!(credentialed("sftp://u@h/x") && !credentialed("sftp://h/a@b") && !credentialed("nfs://u@h/x"));
    assert!(address_mount_covers("sftp://u@h/", "sftp://u@h/home/u"));
    assert!(!address_mount_covers("smb://h/", "smb://h/share"));
}

#[test]
fn remove_drops_every_spelling_and_keeps_the_rest_byte_for_byte() {
    let body = "file:///a A\nsmb://h:445/s/ S\n  smb://h/s  again\nsftp://x/ X";
    assert_eq!(remove_bookmark(body, "smb://h/s"), "file:///a A\nsftp://x/ X");
    assert_eq!(remove_bookmark(body, "smb://other/"), body);
}

#[test]
fn relabel_rewrites_every_duplicate_strips_newlines_and_appends_when_absent() {
    let body = "smb://h/s S\n  smb://h/s/ T\nfile:///a A\n";
    assert_eq!(relabel(body, "smb://h/s/", "New\nName"), "smb://h/s NewName\nsmb://h/s/ NewName\nfile:///a A\n");
    assert_eq!(relabel("file:///a A", "smb://n/x", "X"), "file:///a A\nsmb://n/x X\n");
    assert_eq!(relabel(body, "smb://h/s", "   "), body);
}

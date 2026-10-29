use super::*;

fn ok(text: &str) -> Ran {
    Ran::Exited(Some(0), text.into())
}


#[test]
fn mounts_and_saved_places_make_one_row_each() {
    let mut n = Network::new(false);
    n.set_bookmarks("smb://nas/data NAS data\nsftp://u@h/home/u Home on h\nsmb://nas/iso/ ISOs\n".into());
    n.poll();
    n.listed(ok("Mount(0): data on nas -> smb://nas/data/\nMount(1): u on h -> sftp://u@h/\nMount(2): other on x -> smb://x/other/\n"));
    let rows: Vec<_> = n.entries.iter().map(|e| (e.label.as_str(), e.mounted)).collect();
    assert_eq!(rows, vec![("NAS data", true), ("Home on h", true), ("other", true), ("ISOs", false)]);
    // The covered saved path unmounts the live root mount.
    assert_eq!(n.entries[1].mount_uri.as_deref(), Some("sftp://u@h/"));
}

#[test]
fn a_listing_that_failed_keeps_the_rows_and_one_asked_mid_listing_runs_after() {
    let mut n = Network::new(false);
    n.poll();
    n.listed(ok("Mount(0): data on nas -> smb://nas/data/\n"));
    assert_eq!(n.poll(), vec![Effect::List]);
    assert!(n.poll().is_empty());
    assert_eq!(n.listed(Ran::TimedOut), vec![Effect::List]);
    assert_eq!(n.entries.len(), 1);
}

#[test]
fn unmount_names_the_live_mount_and_reports_either_way() {
    let mut n = Network::new(false);
    n.set_bookmarks("sftp://u@h/home/u Home on h\n".into());
    n.poll();
    n.listed(ok("Mount(1): u on h -> sftp://u@h/\n"));
    assert_eq!(n.unmount(0), vec![Effect::Unmount(vec!["gio".into(), "mount".into(), "-u".into(), "sftp://u@h/".into()])]);
    let out = n.unmounted(ok(""));
    assert!(out.contains(&Effect::Message("Unmounted Home on h.".into(), false)));
}

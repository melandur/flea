use super::*;

fn ok(text: &str) -> Ran {
    Ran::Exited(Some(0), text.into())
}

fn legs(effects: &[Effect]) -> Vec<(Leg, Vec<String>)> {
    effects.iter().filter_map(|e| if let Effect::Leg(l, a) = e { Some((*l, a.clone())) } else { None }).collect()
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
fn an_already_mounted_refusal_still_opens_because_gio_info_decides() {
    let mut n = Network::new(false);
    let out = n.open("smb://nas/data", false, "data");
    assert_eq!(legs(&out), vec![(Leg::Mount, vec!["gio".into(), "mount".into(), "--anonymous".into(), "smb://nas/data".into()])]);
    // gvfsd answers exit 2 with a translated sentence; the code is recorded and not believed.
    let out = n.leg_done(Leg::Mount, Ran::Exited(Some(2), String::new()));
    assert_eq!(legs(&out)[0].0, Leg::Info);
    let out = n.leg_done(Leg::Info, ok("local path: /run/user/1000/gvfs/smb-share:server=nas,share=data\n"));
    assert!(out.contains(&Effect::Opened("/run/user/1000/gvfs/smb-share:server=nas,share=data".into())));
}

#[test]
fn a_bare_root_lists_its_shares_even_when_info_refuses() {
    let mut n = Network::new(false);
    n.open("smb://192.168.1.10/", false, "NAS");
    n.leg_done(Leg::Mount, ok(""));
    let out = n.leg_done(Leg::Info, Ran::Exited(Some(1), String::new()));
    assert_eq!(legs(&out), vec![(Leg::List, vec!["gio".into(), "list".into(), "smb://192.168.1.10/".into()])]);
    let out = n.leg_done(Leg::List, ok("appdata\nbackups\n\ndata\n"));
    assert_eq!(out, vec![Effect::SharesListed { base: "smb://192.168.1.10/".into(), label: "NAS".into(), names: vec!["appdata".into(), "backups".into(), "data".into()] }]);
}

#[test]
fn every_leg_is_bounded_and_the_guard_says_why_it_refused() {
    let mut n = Network::new(false);
    n.open("smb://nas/data", true, "data");
    assert_eq!(n.open("smb://nas/other", false, "other"), vec![Effect::Message("Another network location is still opening; give it a moment.".into(), false)]);
    let out = n.leg_done(Leg::Info, Ran::TimedOut);
    assert_eq!(out, vec![Effect::Message("Connect failed: host did not respond".into(), true)]);
    // And the guard is free again afterwards: a hung leg does not lock every later share out.
    assert!(!n.busy());
    n.open("smb://192.168.1.10/", false, "NAS");
    n.leg_done(Leg::Mount, ok(""));
    n.leg_done(Leg::Info, ok(""));
    assert_eq!(n.leg_done(Leg::List, Ran::TimedOut), vec![Effect::Message("Connect failed: host did not respond".into(), true)]);
}

#[test]
fn a_refused_mount_and_an_empty_location_say_different_things() {
    let mut n = Network::new(false);
    n.open("smb://nas/data", false, "data");
    n.leg_done(Leg::Mount, Ran::Exited(Some(2), String::new()));
    let out = n.leg_done(Leg::Info, Ran::Exited(Some(1), String::new()));
    assert!(out.contains(&Effect::Message("Connect failed: network location was refused".into(), true)));
    n.open("smb://nas/data", false, "data");
    n.leg_done(Leg::Mount, ok(""));
    let out = n.leg_done(Leg::Info, ok("name: x\n"));
    assert!(out.contains(&Effect::Message("Connect failed: location has no browsable folder".into(), true)));
}

#[test]
fn a_credentialed_place_says_signing_in_is_not_here_unless_the_keyring_is_used() {
    let mut n = Network::new(false);
    let out = n.open("sftp://u@h/home/u", false, "h");
    assert_eq!(out, vec![Effect::Message("Signing in to a network location is not in this build yet.".into(), false)]);
    let mut n = Network::new(true);
    assert_eq!(legs(&n.open("sftp://u@h/home/u", false, "h"))[0].1, vec!["gio".to_string(), "mount".into(), "sftp://u@h/home/u".into()]);
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

// tests/ui.sh case_sharebrowser's stub, here against the real argv the legs run: it answers
// `gio info` in Spanish unless the caller pinned LC_ALL=C, refuses every mount the way gvfsd refuses
// an already-mounted share, in Spanish and with exit 2, and lists two shares for the bare root.
const STUB: &str = r#"#!/bin/sh
case "$1 $2" in
  "mount --anonymous"|"mount smb://"*)
    echo "smb://stubhost/: La ubicación ya está montada" >&2; exit 2 ;;
  "info smb://stubhost/share1")
    if [ "$LC_ALL" = C ]; then echo "local path: /run/stub/share1"; else echo "ruta local: /run/stub/share1"; fi ;;
  "info "*) exit 1 ;;
  "list smb://stubhost/") printf 'share1\nshare2\n' ;;
  *) exit 9 ;;
esac
"#;

fn drive(n: &mut Network, mut effects: Vec<Effect>, path: &str) -> Vec<Effect> {
    let mut seen = vec![];
    while let Some(e) = effects.pop() {
        match e {
            Effect::Leg(leg, argv) => {
                // The product's own pin, plus the stub's PATH: removing the pin reddens this.
                let mut env: Vec<(&str, &str)> = crate::process::GIO_ENV.to_vec();
                env.push(("PATH", path));
                let ran = crate::process::run_within(&argv, &env, std::time::Duration::from_secs(5));
                effects.extend(n.leg_done(leg, ran));
            }
            Effect::List => {}
            other => seen.push(other),
        }
    }
    seen
}

#[test]
fn a_bare_root_lists_through_a_spanish_gvfs_and_a_share_opens_by_the_pinned_locale() {
    let dir = std::env::temp_dir().join(format!("flea-iced-gio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("gio");
    std::fs::write(&stub, STUB).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", dir.display());

    let mut n = Network::new(false);
    let out = n.open("smb://stubhost/", false, "Stub");
    let seen = drive(&mut n, out, &path);
    assert_eq!(seen, vec![Effect::SharesListed { base: "smb://stubhost/".into(), label: "Stub".into(), names: vec!["share1".into(), "share2".into()] }]);
    // The share is refused as already mounted, in Spanish, and opens anyway because info decides.
    let out = n.open("smb://stubhost/share1/", false, "share1");
    let seen = drive(&mut n, out, &path);
    assert_eq!(seen, vec![Effect::Opened("/run/stub/share1".into())]);
    let _ = std::fs::remove_dir_all(&dir);
}

use super::*;
use crate::network::{Effect, Leg, Network};
use crate::process::Ran;

fn ok(text: &str) -> Ran {
    Ran::Exited(Some(0), text.into())
}

fn legs(effects: &[Effect]) -> Vec<(Leg, Vec<String>)> {
    effects.iter().filter_map(|e| if let Effect::Leg(_, l, a) = e { Some((*l, a.clone())) } else { None }).collect()
}

fn auth(effects: &[Effect]) -> Option<(Vec<String>, String)> {
    effects.iter().find_map(|e| if let Effect::Auth(_, a, p) = e { Some((a.clone(), p.clone())) } else { None })
}

#[test]
fn an_already_mounted_refusal_still_opens_because_gio_info_decides() {
    let mut n = Network::new(false);
    let out = n.open("smb://nas/data", false, "data");
    assert_eq!(legs(&out), vec![(Leg::Mount, vec!["gio".into(), "mount".into(), "--anonymous".into(), "smb://nas/data".into()])]);
    // gvfsd answers exit 2 with a translated sentence; the code is recorded and not believed.
    let out = n.leg_done(n.generation, Leg::Mount, Ran::Exited(Some(2), String::new()));
    assert_eq!(legs(&out)[0].0, Leg::Info);
    let out = n.leg_done(n.generation, Leg::Info, ok("local path: /run/user/1000/gvfs/smb-share:server=nas,share=data\n"));
    assert!(out.contains(&Effect::Opened("/run/user/1000/gvfs/smb-share:server=nas,share=data".into())));
}

#[test]
fn a_bare_root_lists_its_shares_even_when_info_refuses() {
    let mut n = Network::new(false);
    n.open("smb://192.168.1.10/", false, "NAS");
    n.leg_done(n.generation, Leg::Mount, ok(""));
    let out = n.leg_done(n.generation, Leg::Info, Ran::Exited(Some(1), String::new()));
    assert_eq!(legs(&out), vec![(Leg::List, vec!["gio".into(), "list".into(), "smb://192.168.1.10/".into()])]);
    let out = n.leg_done(n.generation, Leg::List, ok("appdata\nbackups\n\ndata\n"));
    assert_eq!(out, vec![Effect::SharesListed { base: "smb://192.168.1.10/".into(), label: "NAS".into(), names: vec!["appdata".into(), "backups".into(), "data".into()] }]);
}

#[test]
fn every_leg_is_bounded_and_the_guard_says_why_it_refused() {
    let mut n = Network::new(false);
    n.open("smb://nas/data", true, "data");
    assert_eq!(n.open("smb://nas/other", false, "other"), vec![Effect::Message("Another network location is still opening; give it a moment.".into(), false)]);
    let out = n.leg_done(n.generation, Leg::Info, Ran::TimedOut);
    assert_eq!(out, vec![Effect::Message("Connect failed: host did not respond".into(), true)]);
    // And the guard is free again afterwards: a hung leg does not lock every later share out.
    assert!(!n.busy());
    n.open("smb://192.168.1.10/", false, "NAS");
    n.leg_done(n.generation, Leg::Mount, ok(""));
    n.leg_done(n.generation, Leg::Info, ok(""));
    assert_eq!(n.leg_done(n.generation, Leg::List, Ran::TimedOut), vec![Effect::Message("Connect failed: host did not respond".into(), true)]);
}

#[test]
fn a_refused_mount_and_an_empty_location_say_different_things() {
    let mut n = Network::new(false);
    n.open("smb://nas/data", false, "data");
    n.leg_done(n.generation, Leg::Mount, Ran::Exited(Some(2), String::new()));
    let out = n.leg_done(n.generation, Leg::Info, Ran::Exited(Some(1), String::new()));
    assert!(out.contains(&Effect::Message("Connect failed: network location was refused".into(), true)));
    n.open("smb://nas/data", false, "data");
    n.leg_done(n.generation, Leg::Mount, ok(""));
    let out = n.leg_done(n.generation, Leg::Info, ok("name: x\n"));
    assert!(out.contains(&Effect::Message("Connect failed: location has no browsable folder".into(), true)));
}

#[test]
fn a_credentialed_place_with_no_password_asks_for_one_unless_the_keyring_is_used() {
    let mut n = Network::new(false);
    let out = n.open("sftp://u@h/home/u", false, "h");
    assert_eq!(out, vec![Effect::AskPassword { uri: "sftp://u@h/home/u".into(), label: "h".into(), password: String::new(), reason: "Enter the password to mount this location.".into(), failed: false }]);
    let mut n = Network::new(true);
    assert_eq!(legs(&n.open("sftp://u@h/home/u", false, "h"))[0].1, vec!["gio".to_string(), "mount".into(), "sftp://u@h/home/u".into()]);
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
            Effect::Leg(generation, leg, argv) => {
                // The product's own pin, plus the stub's PATH: removing the pin reddens this.
                let mut env: Vec<(&str, &str)> = crate::process::GIO_ENV.to_vec();
                env.push(("PATH", path));
                let ran = crate::process::run_within(&argv, &env, std::time::Duration::from_secs(5));
                effects.extend(n.leg_done(generation, leg, ran));
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
    crate::testexec::wait_executable(&stub);
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

#[test]
fn a_password_goes_to_the_helper_on_stdin_and_a_success_completes_the_dialogs_request() {
    let mut n = Network::new(false);
    let out = n.save_location("sftp://u@h/home/u", "h", "s3cret", "network-1");
    let (argv, input) = auth(&out).unwrap();
    assert_eq!(argv, vec![helper(), "sftp://u@h/home/u".into(), "never".into()]);
    assert_eq!(input, "s3cret\n");
    assert!(!argv.iter().any(|a| a.contains("s3cret")));
    let out = n.auth_done(n.generation, ok(""));
    assert_eq!(legs(&out)[0].0, Leg::Info);
    let out = n.leg_done(n.generation, Leg::Info, ok("local path: /run/u/gvfs/sftp:host=h\n"));
    assert!(out.contains(&Effect::Completed { id: "network-1".into(), uri: "sftp://u@h/home/u".into(), success: true, reason: String::new() }));
    assert!(out.contains(&Effect::Opened("/run/u/gvfs/sftp:host=h".into())));
    // Remembered for this window: the next open of the same place does not ask.
    assert!(auth(&n.open("sftp://u@h/home/u", false, "h")).is_some());
}

#[test]
fn an_unknown_host_key_is_a_question_and_the_trust_is_spent_by_one_attempt() {
    let mut n = Network::new(false);
    n.save_location("sftp://u@h/x", "h", "pw", "network-1");
    let asked = "The identity sent by the remote computer is SHA256:k3y. \n[1] Log In Anyway\nChoice: ";
    let out = n.auth_done(n.generation, Ran::Exited(Some(3), asked.into()));
    let reason = "Verify: h has not been connected to from this computer before. It offers SHA256:k3y. Connect anyway?";
    assert!(out.contains(&Effect::Completed { id: "network-1".into(), uri: "sftp://u@h/x".into(), success: false, reason: reason.into() }));
    // The password was right and is kept; the operator answers the question and tries once more.
    n.trust_once("sftp://u@h/x");
    let out = n.save_location("sftp://u@h/x", "h", "pw", "network-2");
    assert_eq!(auth(&out).unwrap().0.last().unwrap(), "trust");
    n.auth_done(n.generation, Ran::Exited(Some(1), String::new()));
    n.leg_done(n.generation, Leg::Info, Ran::Exited(Some(1), String::new()));
    assert_ne!(auth(&n.save_location("sftp://u@h/x", "h", "pw", "network-3")).unwrap().0.last().unwrap(), "trust");
}

#[test]
fn a_refused_password_is_forgotten_but_an_already_mounted_one_opens() {
    let mut n = Network::new(false);
    n.save_location("sftp://u@h/x", "h", "wrong", "network-1");
    n.auth_done(n.generation, Ran::Exited(Some(1), String::new()));
    let out = n.leg_done(n.generation, Leg::Info, Ran::Exited(Some(1), String::new()));
    assert!(out.contains(&Effect::Message("Connect failed: authentication was refused".into(), true)));
    assert!(n.passwords.is_empty());
    // gio mount says "already mounted" and exits 2; gio info resolves it, so it opens.
    n.save_location("sftp://u@h/storage/x", "h", "right", "network-2");
    n.auth_done(n.generation, Ran::Exited(Some(2), String::new()));
    let out = n.leg_done(n.generation, Leg::Info, ok("local path: /run/u/gvfs/x\n"));
    assert!(out.contains(&Effect::Opened("/run/u/gvfs/x".into())));
}

#[test]
fn a_deadline_answers_the_dialog_but_never_reopens_it_for_a_password() {
    let mut n = Network::new(false);
    n.open("smb://nas/data", false, "data");
    assert_eq!(n.leg_done(n.generation, Leg::Mount, Ran::TimedOut), vec![Effect::Message("Connect failed: host did not respond".into(), true)]);
    n.save_location("smb://nas/data", "data", "", "network-9");
    let out = n.leg_done(n.generation, Leg::Mount, Ran::TimedOut);
    assert!(out.contains(&Effect::Completed { id: "network-9".into(), uri: "smb://nas/data".into(), success: false, reason: "Connect failed: host did not respond".into() }));
}

#[test]
fn a_cancelled_request_ignores_its_late_answer_and_frees_the_guard() {
    let mut n = Network::new(false);
    n.save_location("smb://nas/data", "data", "", "network-1");
    let stale = n.generation;
    n.cancel("network-1");
    assert!(n.leg_done(stale, Leg::Mount, ok("")).is_empty());
    assert!(!n.busy());
    let out = n.open("smb://nas/other", false, "other");
    assert!(n.leg_done(stale, Leg::Info, ok("local path: /wrong\n")).is_empty());
    assert_eq!(legs(&out)[0].0, Leg::Mount);
}

#[test]
fn a_helper_that_cannot_run_says_so_and_keeps_the_password() {
    let mut n = Network::new(false);
    n.save_location("sftp://u@h/x", "h", "pw", "network-1");
    n.auth_done(n.generation, Ran::NotStarted);
    let out = n.leg_done(n.generation, Leg::Info, Ran::Exited(Some(1), String::new()));
    assert!(out.contains(&Effect::Message("Connect failed: authentication helper is unavailable".into(), true)));
    assert_eq!(n.passwords.get("sftp://u@h/x").map(String::as_str), Some("pw"));
}

// The Auth effect's argv and input run for real against a stub standing in for tools/flea-gio-auth:
// it records the line it read on stdin and its own argv, so the test sees that the password arrived
// on the pipe and never in an argument another process could read.
#[test]
fn the_password_reaches_the_helper_on_its_stdin_and_never_in_its_argv() {
    let dir = std::env::temp_dir().join(format!("flea-iced-auth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("flea-gio-auth");
    let log = dir.join("log");
    std::fs::write(&stub, format!("#!/bin/sh\nread -r pw\nprintf 'stdin=%s argv=%s\\n' \"$pw\" \"$*\" > {}\nexit 0\n", log.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    crate::testexec::wait_executable(&stub);

    let mut n = Network::new(false);
    let out = n.save_location("sftp://u@h/x", "h", "hunter2", "network-1");
    let (mut argv, input) = auth(&out).unwrap();
    argv[0] = stub.to_str().unwrap().to_string();
    let ran = crate::process::run_with_input(&argv, &crate::process::GIO_ENV, Some(input), std::time::Duration::from_secs(5));
    assert_eq!(ran, Ran::Exited(Some(0), String::new()));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "stdin=hunter2 argv=sftp://u@h/x never\n");
    let _ = std::fs::remove_dir_all(&dir);
}

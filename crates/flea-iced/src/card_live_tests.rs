// The menu's dialog cards against the real `flea --backend`: what actioncard_tests.rs asserts about
// the replies it was handed, this asserts the backend's menuaction, newfile and permissions services
// really send them. It runs only when FLEA_BIN names a built backend, and says so when it skips.
use crate::actioncard::{Card, Kind, Out, Phase};
use crate::live_tests::Live;
use crate::wire::{self, Reply, Request};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

// Carry the card's Outs: sends go to the backend, a refresh re-reads the pane, the rest is kept.
fn carry(live: &mut Live, out: Vec<Out>, kept: &mut Vec<Out>) {
    for o in out {
        match o {
            Out::Send(v) => live.stdin.write_all(Request::Json(v).line().as_bytes()).unwrap(),
            Out::Refresh(_) => {
                let effects = live.pane.refresh_here();
                live.run(effects);
            }
            other => kept.push(other),
        }
    }
}

// Feed the backend's lines to the card and the pane until `done` holds over the card and what it kept.
fn pump(live: &mut Live, card: &mut Card, out: Vec<Out>, what: &str, done: impl Fn(&Card, &[Out]) -> bool) -> Vec<Out> {
    let mut kept = vec![];
    carry(live, out, &mut kept);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(card, &kept) {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = live.lines.recv_timeout(left).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
        let out = match wire::parse(&line) {
            Some(Reply::MenuAction(v)) => card.receive_menu(&v),
            Some(Reply::Permissions(v)) => card.receive_permissions(&v),
            Some(reply) => {
                let effects = live.pane.receive(reply);
                live.run(effects);
                vec![]
            }
            None => vec![],
        };
        carry(live, out, &mut kept);
    }
    kept
}

fn closed(kept: &[Out]) -> bool {
    kept.contains(&Out::Close)
}

#[test]
fn every_dialog_row_round_trips_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the dialog round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-cards-{}", std::process::id()));
    let (dir, dest, state) = (root.join("dir"), root.join("dest"), root.join("state"));
    for d in [&dir, &dest, &state] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(dir.join("b.txt"), b"beta").unwrap();
    std::fs::write(dir.join("c.txt"), b"gamma").unwrap();
    let dir_s = dir.to_str().unwrap().to_string();
    // Permanent deletion keeps its recovery records under XDG_STATE_HOME, so it goes in the sandbox.
    let mut live = Live::start_with(&bin, &[("XDG_STATE_HOME", &state)]);
    let out = live.pane.open(dir_s.clone(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());
    assert_eq!(live.names(), ["a.txt", "b.txt", "c.txt"]);
    let a = format!("{dir_s}/a.txt");

    // Properties: snapshot, then the facts of row 0.
    let (mut card, out) = Card::open(Kind::Properties, 1, &dir_s, &[0], 0, &a);
    pump(&mut live, &mut card, out, "properties", |c, _| c.phase == Phase::Ready);
    assert_eq!((card.facts["kind"].as_str(), card.facts["bytes"].as_u64(), card.facts["path"].as_str()), (Some("File"), Some(5), Some(a.as_str())));
    let out = card.close();
    pump(&mut live, &mut card, out, "the close", |_, k| closed(k));

    // New File: created on disk, said, and the listing re-read.
    let (mut card, out) = Card::open(Kind::NewFile, 2, &dir_s, &[], 0, "");
    pump(&mut live, &mut card, out, "nothing", |_, _| true);
    card.input("new.txt".into());
    let out = card.submit();
    let kept = pump(&mut live, &mut card, out, "the new file", |_, k| closed(k));
    assert!(kept.contains(&Out::Say("File created.".into())));
    assert_eq!(std::fs::read(dir.join("new.txt")).unwrap(), b"");

    // Permissions: inspect the real mode, flip owner execute, apply, and read it off the disk.
    std::fs::set_permissions(&a, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (mut card, out) = Card::open(Kind::Permissions, 3, &dir_s, &[0], 0, &a);
    pump(&mut live, &mut card, out, "the inspection", |c, _| c.phase == Phase::Ready);
    assert_eq!(card.text, "0644");
    card.toggle_bit(0o100);
    let out = card.submit();
    pump(&mut live, &mut card, out, "the apply", |_, k| closed(k));
    assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o7777, 0o744);

    // Copy to: the captured row goes to the destination the field names.
    live.until("the re-read", |p| !p.list_in_flight());
    let (mut card, out) = Card::open(Kind::CopyTo, 4, &dir_s, &[1], 1, &format!("{dir_s}/b.txt"));
    pump(&mut live, &mut card, out, "the snapshot", |c, _| c.phase == Phase::Ready);
    card.input(dest.to_str().unwrap().into());
    let out = card.submit();
    pump(&mut live, &mut card, out, "the validation", |_, k| closed(k));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dest.join("b.txt").exists() && Instant::now() < deadline {
        let _ = live.lines.recv_timeout(Duration::from_millis(100));
    }
    assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"beta");

    // Delete permanently: reviewed, confirmed with its token, gone from disk, and nothing else.
    live.until("the listing", |p| !p.list_in_flight());
    let c_row = live.names().iter().position(|n| n == "c.txt").unwrap();
    let (mut card, out) = Card::open(Kind::Delete, 5, &dir_s, &[c_row], c_row, &format!("{dir_s}/c.txt"));
    pump(&mut live, &mut card, out, "the review", |c, _| c.phase == Phase::Confirm);
    assert_eq!(card.facts["count"].as_u64(), Some(1));
    let out = card.confirm();
    let kept = pump(&mut live, &mut card, out, "the deletion", |_, k| closed(k));
    assert!(kept.contains(&Out::Say("Deleted 1 of 1".into())), "{kept:?}");
    assert!(!dir.join("c.txt").exists() && dir.join("a.txt").exists() && dir.join("b.txt").exists());
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

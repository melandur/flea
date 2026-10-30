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

// Read the backend's lines until one parses to a JSON object `pick` accepts, which it returns.
fn wait_line(live: &mut Live, what: &str, pick: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = live.lines.recv_timeout(left).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if pick(&v) {
                return v;
            }
        }
    }
}

fn send(live: &mut Live, v: serde_json::Value) {
    live.stdin.write_all(Request::Json(v).line().as_bytes()).unwrap();
}

// The snapshot and activate exchange, through activation.rs, to the paths the backend vouched for.
fn activate(live: &mut Live, id: u64, action: &str, rows: &[usize]) -> Vec<String> {
    use crate::activation::{Activation, Step};
    let (mut a, snapshot) = Activation::to(id, action, rows, rows[0], None);
    send(live, snapshot);
    loop {
        let v = wait_line(live, action, |v| v["t"] == "menuaction" && v["id"] == id);
        match a.receive(&v) {
            Step::Send(next) => send(live, next),
            Step::Done { paths, .. } => return paths,
            Step::Refused(why) => panic!("{action} refused: {why}"),
            Step::Nothing => {}
        }
    }
}

#[test]
fn the_job_rows_round_trip_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the job round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-jobs-{}", std::process::id()));
    let dir = root.join("dir");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"alpha").unwrap();
    ::image::RgbImage::from_pixel(8, 8, ::image::Rgb([10, 200, 10])).save(dir.join("b.png")).unwrap();
    let dir_s = dir.to_str().unwrap().to_string();
    let mut live = Live::start_with(&bin, &[]);
    let out = live.pane.open(dir_s.clone(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());
    assert_eq!(live.names(), ["a.txt", "b.png"]);

    // The probe the menu reads.
    send(&mut live, serde_json::json!({"c": "formats", "id": 1}));
    let formats = wait_line(&mut live, "formats", |v| v["t"] == "formats");
    let archive: Vec<String> = formats["archive"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
    assert!(archive.contains(&"zip".to_string()), "no zip writer on this box: {formats}");

    // Compress row 0 under its own name, then extract it beside itself.
    let paths = activate(&mut live, 2, "compress:zip", &[0]);
    assert_eq!(paths, vec![format!("{dir_s}/a.txt")]);
    send(&mut live, serde_json::json!({"c": "archive", "op": "compress", "paths": paths, "dest": format!("{dir_s}/a.zip"), "format": "zip", "menuId": 2}));
    let done = wait_line(&mut live, "the archive", |v| v["t"] == "archivedone");
    assert_eq!(done["ok"], true, "{done}");
    assert!(dir.join("a.zip").is_file());
    let out = live.pane.refresh_here();
    live.run(out);
    live.until("the archive listed", |p| !p.list_in_flight() && p.rows.iter().any(|r| r.n == "a.zip"));
    let zip_row = live.names().iter().position(|n| n == "a.zip").unwrap();
    activate(&mut live, 3, "extract", &[zip_row]);
    send(&mut live, serde_json::json!({"c": "archive", "op": "extract", "path": format!("{dir_s}/a.zip"), "dest": format!("{dir_s}/{}", crate::archives::extract_dir("a.zip")), "menuId": 3}));
    let done = wait_line(&mut live, "the extract", |v| v["t"] == "archivedone");
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(std::fs::read(dir.join("a/a.txt")).unwrap(), b"alpha");

    // Convert: the card's check, then the conversion, through convertcard.rs.
    if formats["convert"] == true {
        let png_row = live.names().iter().position(|n| n == "b.png").unwrap();
        activate(&mut live, 4, "convert", &[png_row]);
        let (mut card, out) = crate::convertcard::Convert::open(&format!("{dir_s}/b.png"), "b.png", 4);
        for v in out {
            send(&mut live, v);
        }
        let checked = wait_line(&mut live, "the check", |v| v["t"] == "convertchecked");
        assert!(card.checked(&checked).is_empty());
        assert!(card.can_convert(), "{checked}");
        for v in card.commit() {
            send(&mut live, v);
        }
        // The wire orders started before done, and the card only believes the operation it saw start.
        let started = wait_line(&mut live, "the start", |v| v["t"] == "convertstarted");
        card.started(&started);
        let done = wait_line(&mut live, "the conversion", |v| v["t"] == "convertdone");
        assert_eq!(card.done(&done), Some(true), "{done}");
        assert!(dir.join("b (converted).jpg").is_file());
    } else {
        eprintln!("live: ImageMagick is not here, so convert was skipped");
    }

    // Open with: the catalogue only. Nothing is launched from a test.
    let (mut card, snapshot) = crate::openwith::OpenWith::open(5, &[0], 0);
    send(&mut live, snapshot);
    while card.busy {
        let v = wait_line(&mut live, "the catalogue", |v| v["t"] == "menuaction" && v["id"] == 5);
        for o in card.receive(&v) {
            if let crate::openwith::Out::Send(next) = o {
                send(&mut live, next);
            }
        }
    }
    assert!(card.error.is_empty(), "{}", card.error);
    assert_eq!(card.name(), "a.txt");
    assert!(!card.installed.is_empty() && !card.kind.is_empty());
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

// Move to Dropbox against the real backend, with a stand-in account folder: the destination is
// captured between the snapshot and the activation, and the move names rows and the menuId.
#[test]
fn move_to_dropbox_captures_its_destination_and_moves_the_rows() {
    use crate::activation::{Activation, Step};
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the Dropbox round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-dropbox-{}", std::process::id()));
    let (dir, account) = (root.join("dir"), root.join("Dropbox"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(dir.join("a.txt"), b"alpha").unwrap();
    let (dir_s, account_s) = (dir.to_str().unwrap().to_string(), account.to_str().unwrap().to_string());
    let mut live = Live::start_with(&bin, &[]);
    let out = live.pane.open(dir_s.clone(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());

    // A destination that is not a directory is refused at the capture, and nothing moves.
    let (mut bad, snapshot) = Activation::to(7, "dropbox", &[0], 0, Some(format!("{dir_s}/a.txt")));
    send(&mut live, snapshot);
    let refused = loop {
        let v = wait_line(&mut live, "the refusal", |v| v["t"] == "menuaction" && v["id"] == 7);
        match bad.receive(&v) {
            Step::Send(next) => send(&mut live, next),
            Step::Refused(why) => break why,
            other => panic!("expected a refusal, got {other:?}"),
        }
    };
    assert_eq!(refused, "Dropbox account folder is not a directory.");

    let (mut a, snapshot) = Activation::to(8, "dropbox", &[0], 0, Some(account_s.clone()));
    send(&mut live, snapshot);
    loop {
        let v = wait_line(&mut live, "the activation", |v| v["t"] == "menuaction" && v["id"] == 8);
        match a.receive(&v) {
            Step::Send(next) => send(&mut live, next),
            Step::Done { action, .. } => {
                assert_eq!(action, "dropbox");
                break;
            }
            other => panic!("expected the activation, got {other:?}"),
        }
    }
    send(&mut live, serde_json::json!({"c": "transfer", "op": "move", "rows": [0], "dest": account_s, "menuId": 8}));
    let done = wait_line(&mut live, "the move", |v| v["t"] == "transferdone");
    assert_eq!(done["ok"], 1, "{done}");
    assert!(!dir.join("a.txt").exists() && account.join("a.txt").is_file());
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

// The writes this port's later stages added, put through the real binary against a state directory
// of the test's own: the panel's favourite remove and move with the records they were read from,
// the header menu's whole column set, and the sweep's day.
#[test]
fn the_favourite_column_and_sweep_writes_are_accepted_by_the_real_updaters() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the favourite and column writes were skipped");
        return;
    };
    let state = std::env::temp_dir().join(format!("flea-iced-favs-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let run = |flag: &str, arg: serde_json::Value| -> serde_json::Value {
        let out = std::process::Command::new(&bin).env("XDG_STATE_HOME", &state).arg(flag).arg(arg.to_string()).output().unwrap();
        assert!(out.status.success(), "{arg}: {}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let favs = |doc: &serde_json::Value| doc["places"]["favourites"].clone();
    run("--favourites", serde_json::json!({"op": "add", "record": {"label": "a", "path": "/tmp/a"}}));
    let doc = run("--favourites", serde_json::json!({"op": "add", "record": {"label": "b", "path": "/tmp/b"}}));
    let doc = run("--favourites", serde_json::json!({"op": "move", "index": 1, "to": 0, "expected": favs(&doc)}));
    assert_eq!(favs(&doc)[0]["label"], "b");
    let doc = run("--favourites", serde_json::json!({"op": "remove", "index": 0, "expected": favs(&doc)}));
    assert_eq!(favs(&doc).as_array().map(|a| a.len()), Some(1));
    // A remove naming records the list no longer holds is refused, which is what guards the panel.
    let stale = std::process::Command::new(&bin).env("XDG_STATE_HOME", &state).arg("--favourites")
        .arg(serde_json::json!({"op": "remove", "index": 0, "expected": []}).to_string()).output().unwrap();
    assert!(!stale.status.success());
    let doc = run("--ui-state", serde_json::json!({"columns": ["name", "size", "date", "mode", "items"]}));
    assert_eq!(doc["columns"], serde_json::json!(["name", "size", "date", "mode", "items"]));
    let doc = run("--ui-state", serde_json::json!({"trashSweptOn": crate::trashsweep::day_number(0).max(1)}));
    assert!(doc["trashSweptOn"].as_i64().unwrap() > 0);
    let _ = std::fs::remove_dir_all(&state);
}

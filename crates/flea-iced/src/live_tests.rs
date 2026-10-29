// The pane against the real `flea --backend`, with no window: what the unit tests assert about the
// wire, this asserts the backend actually answers. It runs only when FLEA_BIN names a built backend,
// because a stale or absent binary would test the wrong thing, and it says so when it skips.
// Trash is not driven here: `gio trash` refuses /tmp, see AGENTS.md "Testing them is tests/ops.sh".
use crate::pane::{Effect, Pane};
use crate::wire::{self, Reply, Request};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

struct Live {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    pane: Pane,
    sent: Vec<Request>,
    settles: Vec<u64>,
    decodes: Vec<String>,
}

impl Live {
    fn start(bin: &str) -> Live {
        Live::start_with(bin, &[])
    }

    fn start_with(bin: &str, env: &[(&str, &std::path::Path)]) -> Live {
        let mut cmd = Command::new(bin);
        cmd.arg("--backend").stdin(Stdio::piped()).stdout(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Live { child, stdin, lines, pane: Pane::new(false, 20), sent: vec![], settles: vec![], decodes: vec![] }
    }

    fn run(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Send(r) => {
                    self.sent.push(r.clone());
                    self.stdin.write_all(r.line().as_bytes()).unwrap();
                }
                // No timer here: the settle fires at once, which is what the timer would do idle.
                Effect::Settle(generation, _) => self.settles.push(generation),
                Effect::DecodeThumb(path) => self.decodes.push(path),
                _ => {}
            }
        }
        while let Some(generation) = self.settles.pop() {
            let out = self.pane.settled(generation);
            self.run(out);
        }
    }

    // Feed replies into the pane until `done` holds, carrying out every Send it asks for.
    fn until(&mut self, what: &str, done: impl Fn(&Pane) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.pane) {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.lines.recv_timeout(left).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
            if let Some(reply) = wire::parse(&line) {
                if let Reply::Error { .. } = reply {
                    eprintln!("live: {line}");
                }
                let out = self.pane.receive(reply);
                self.run(out);
                let owed = self.pane.pay_debt();
                self.run(owed);
            }
        }
    }

    fn names(&self) -> Vec<String> {
        self.pane.rows.iter().map(|r| r.n.clone()).collect()
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(Request::Quit.line().as_bytes());
        let _ = self.child.wait();
    }
}

#[test]
fn the_write_operations_round_trip_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the backend round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-live-{}", std::process::id()));
    let (src, dst) = (root.join("src"), root.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("a.txt"), b"alpha").unwrap();
    let src_s = src.to_str().unwrap().to_string();
    let dst_s = dst.to_str().unwrap().to_string();

    let mut live = Live::start(&bin);
    let out = live.pane.open(src_s.clone(), None);
    live.run(out);
    live.until("the first listing", |p| p.listed_once && !p.list_in_flight());
    assert_eq!(live.names(), vec!["a.txt"]);

    // New folder: the backend picks the name, the cursor lands on it, the editor opens on it.
    let out = live.pane.new_folder();
    live.run(out);
    live.until("the editor on the new folder", |p| p.renaming.is_some());
    assert_eq!(live.pane.renaming.as_ref().unwrap().source, format!("{src_s}/New Folder"));

    live.pane.rename_input("Invoices".into());
    let out = live.pane.commit_rename();
    live.run(out);
    live.until("the renamed folder listed", |p| p.renaming.is_none() && !p.list_in_flight() && p.rows.iter().any(|r| r.n == "Invoices"));
    assert!(src.join("Invoices").is_dir());
    assert_eq!(live.pane.row_at(live.pane.cursor).unwrap().n, "Invoices");

    // Undo twice: the rename goes back, then the empty folder it made is removed.
    let out = live.pane.undo();
    live.run(out);
    live.until("the rename undone", |p| !p.list_in_flight() && p.rows.iter().any(|r| r.n == "New Folder"));
    let out = live.pane.undo();
    live.run(out);
    live.until("the folder undone", |p| !p.list_in_flight() && p.rows.len() == 1);
    assert!(!src.join("New Folder").exists() && !src.join("Invoices").exists());

    // Copy a.txt, paste it into dst.
    let a = live.pane.rows.iter().position(|r| r.n == "a.txt").unwrap();
    let out = live.pane.tap(a, 1, false, false);
    live.run(out);
    let out = live.pane.copy(false);
    live.run(out);
    live.until("the clipboard", |p| p.ops.clipboard.is_some());
    let out = live.pane.open(dst_s.clone(), None);
    live.run(out);
    live.until("dst listed", |p| p.path == dst_s && !p.list_in_flight());
    let out = live.pane.paste();
    live.run(out);
    live.until("the pasted copy listed", |p| p.ops.transfer.is_none() && !p.list_in_flight() && p.rows.iter().any(|r| r.n == "a.txt"));
    assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"alpha");
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_visible_image_is_thumbnailed_decoded_and_nothing_else_is_asked() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the thumbnail round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-thumbs-{}", std::process::id()));
    let (dir, cache) = (root.join("pics"), root.join("cache"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    // 30 rows: a red image on row 0, then text files, then a second image far below the viewport.
    let png = ::image::RgbImage::from_pixel(320, 200, ::image::Rgb([220, 20, 20]));
    png.save(dir.join("a.png")).unwrap();
    for i in 0..28 {
        std::fs::write(dir.join(format!("b{i:02}.txt")), b"x").unwrap();
    }
    png.save(dir.join("z.png")).unwrap();
    let dir_s = dir.to_str().unwrap().to_string();

    // The cache root follows XDG_CACHE_HOME, so the operator's own cache is never written.
    let mut live = Live::start_with(&bin, &[("XDG_CACHE_HOME", &cache)]);
    live.pane = Pane::new(false, 5).with_view("media", false);
    let out = live.pane.open(dir_s.clone(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());
    let out = live.pane.viewport(0, 5, 5);
    live.run(out);
    live.until("row 0's answer", |p| p.thumb_file(0).is_some() || p.thumbs.get(&0) == Some(&crate::pane::Thumb::None));

    let asked: Vec<usize> = live.sent.iter().filter_map(|r| if let Request::Thumb { rows } = r { Some(rows.clone()) } else { None }).flatten().collect();
    assert_eq!(asked, vec![0], "only the visible thumbnailable row is ever named");
    let file = live.pane.thumb_file(0).unwrap_or_else(|| panic!("no thumbnailer answered for a.png; is glycin and bwrap installed?")).to_string();
    assert!(file.starts_with(cache.to_str().unwrap()), "the thumbnail landed outside the sandbox cache: {file}");
    assert_eq!(live.decodes, vec![file.clone()]);
    let decoded = ::image::open(&file).unwrap().thumbnail(46, 46).to_rgba8();
    let px = decoded.get_pixel(decoded.width() / 2, decoded.height() / 2);
    assert!(px[0] > 180 && px[1] < 60 && px[2] < 60, "the decoded thumbnail is not the red image: {px:?}");
    assert!(crate::decode(&file).is_some());
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_preview_reads_an_archive_index_and_renders_an_image_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the preview round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-preview-{}", std::process::id()));
    let dir = root.join("d");
    std::fs::create_dir_all(dir.join("inner")).unwrap();
    std::fs::write(dir.join("inner/a.txt"), b"alpha").unwrap();
    std::fs::write(dir.join("b.txt"), b"beta").unwrap();
    let zip = root.join("pack.zip");
    let made = Command::new("bsdtar").arg("-a").arg("-cf").arg(&zip).arg("-C").arg(&dir).arg("inner").arg("b.txt").status().unwrap();
    assert!(made.success());
    let png = root.join("red.png");
    ::image::RgbImage::from_pixel(64, 40, ::image::Rgb([220, 20, 20])).save(&png).unwrap();

    // The archive: list its folder, ask the one row's index, read the answer the way the shell does.
    let mut live = Live::start(&bin);
    let out = live.pane.open(root.to_str().unwrap().into(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());
    let row = live.pane.rows.iter().position(|r| r.n == "pack.zip").unwrap();
    live.stdin.write_all(Request::MetaArchive { row }.line().as_bytes()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let meta = loop {
        let line = live.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("no meta answer");
        if let Some(Reply::Meta { row: r, entries, names, archive_failed }) = wire::parse(&line) {
            break (r, entries, names, archive_failed);
        }
    };
    assert_eq!(meta.0, row);
    assert!(!meta.3, "the index read failed");
    assert!(meta.1 >= 2 && meta.2.iter().any(|(n, _)| n.trim_end_matches('/') == "b.txt"), "{meta:?}");
    drop(live);

    // The image: --preview-image renders in the thumbnail jail and answers a PNG path.
    let out = Command::new(&bin).args(["--preview-image", png.to_str().unwrap(), "512"]).output().unwrap();
    let rendered = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(out.status.success() && rendered.ends_with(".png"), "{rendered}");
    let decoded = ::image::open(&rendered).unwrap().to_rgb8();
    assert!(decoded.get_pixel(decoded.width() / 2, decoded.height() / 2)[0] > 180);

    // And a table read off disk the way the shell reads one.
    let csv = root.join("t.csv");
    std::fs::write(&csv, "name;n\nwidget;3\n").unwrap();
    let t = crate::previewshell::Table::read(&std::fs::read_to_string(&csv).unwrap(), csv.to_str().unwrap(), false);
    assert_eq!((t.delim, t.records(), t.row(1)), (';', 2, vec!["widget".to_string(), "3".into()]));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_deep_search_walks_ranks_and_returns_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the search round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-search-{}", std::process::id()));
    std::fs::create_dir_all(root.join("docs/deep")).unwrap();
    for f in ["docs/deep/helper.txt", "docs/the_helper_notes.md", "helper", "unrelated.txt"] {
        std::fs::write(root.join(f), b"x").unwrap();
    }
    let root_s = root.to_str().unwrap().to_string();
    let mut live = Live::start(&bin);
    let out = live.pane.open(root_s.clone(), None);
    live.run(out);
    live.until("the listing", |p| p.listed_once && !p.list_in_flight());

    // Ctrl+Shift+F, "helper", the live timer firing at once as it would after 120 ms idle.
    let mut out = live.pane.search_start(true);
    out.extend(live.pane.search_typed("helper"));
    let generation = out.iter().rev().find_map(|e| if let Effect::SearchTimer(g) = e { Some(*g) } else { None }).unwrap();
    let out = live.pane.search_live(generation);
    live.run(out);
    live.until("the ranked results", |p| !p.search.running && p.total == 3 && p.rows.len() == 3);
    // The whole name first, then the name it starts, then the one it starts a later word of.
    assert_eq!(live.names(), vec!["helper", "docs/deep/helper.txt", "docs/the_helper_notes.md"]);
    assert!(live.pane.search_status().starts_with("3 found in "));

    let out = live.pane.search_close();
    live.run(out);
    live.until("the folder again", |p| !p.list_in_flight() && p.total == 3 && p.path == root_s && !p.search_listed());
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

// Every row the settings panel draws writes a patch, and `flea --ui-state` refuses a whole patch that
// names a key or a value it does not know, which in the window would be a toggle that silently never
// lands. So every check, every value of every choice and both menu writers are put through the real
// binary, against a state directory of the test's own so the operator's ui.json is never touched.
#[test]
fn every_settings_row_writes_a_patch_the_real_backend_accepts() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the settings round trip was skipped");
        return;
    };
    use crate::settings::{self, Kind};
    let state = std::env::temp_dir().join(format!("flea-iced-state-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let write = |patch: &serde_json::Value| -> Result<serde_json::Value, String> {
        let out = Command::new(&bin).env("XDG_STATE_HOME", &state).arg("--ui-state").arg(patch.to_string()).output().unwrap();
        if out.status.success() { Ok(serde_json::from_slice(&out.stdout).unwrap()) } else { Err(format!("{patch}: {}", String::from_utf8_lossy(&out.stderr))) }
    };
    let doc = write(&serde_json::json!({})).unwrap();
    let facts = settings::Facts { home: "/home/u".into(), base_size: 12, monitor_scale: 1.0, version: String::new(), handler: String::new(), save_status: String::new(), apps: vec![] };
    let mut tried = 0;
    let mut refused = vec![];
    for (section, _, _) in settings::SECTIONS.iter().chain([("columns", "", "")].iter()) {
        for r in settings::rows(section, &doc, &facts) {
            let patches: Vec<serde_json::Value> = match r.kind {
                Kind::Check if r.id.starts_with("column:") || r.id == "keyHints" => continue,
                Kind::Check if *section == "menus" => vec![crate::places_rows::menu_patch(&doc, &[Box::leak(r.id.clone().into_boxed_str())], false)],
                Kind::Check => vec![settings::patch(&r.id, serde_json::json!(!r.on)), settings::patch(&r.id, serde_json::json!(r.on))],
                Kind::Choice if !r.values.is_empty() => r.values.iter().map(|v| settings::patch(&r.id, v.clone())).collect(),
                Kind::Group if r.master && r.id != "shelf.enabled" => vec![crate::places_rows::menu_patch(&doc, &r.ids, true)],
                _ => continue,
            };
            for p in patches {
                tried += 1;
                if let Err(e) = write(&p) {
                    refused.push(e);
                }
            }
        }
    }
    // The writers that are not plain rows: the text size's two shapes, the columns set, the hints.
    for p in [
        serde_json::json!({ "display": { "textSize": { "mode": 16 } } }),
        serde_json::json!({ "display": { "textSize": { "mode": "system" } } }),
        serde_json::json!({ "columns": ["name", "size", "date", "kind"] }),
        serde_json::json!({ "keyHints": true }),
        serde_json::json!({ "shelf": { "enabled": true } }),
        serde_json::json!({ "startFolder": "/home/u/Work" }),
    ] {
        tried += 1;
        if let Err(e) = write(&p) {
            refused.push(e);
        }
    }
    let _ = std::fs::remove_dir_all(&state);
    assert!(tried > 150, "only {tried} patches were tried");
    assert!(refused.is_empty(), "{} of {tried} refused:\n{}", refused.len(), refused.join("\n"));
}

#[test]
fn the_installed_catalogue_answers_and_a_rule_from_it_is_written_and_read_back() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the File types round trip was skipped");
        return;
    };
    let mut live = Live::start(&bin);
    live.stdin.write_all(Request::MenuInstalled { id: 41 }.line().as_bytes()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let apps = loop {
        let line = live.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())).expect("no catalogue answer");
        if let Some(Reply::Installed { id, apps }) = wire::parse(&line) {
            assert_eq!(id, 41, "an answer for another request");
            break apps.expect("the catalogue was refused");
        }
    };
    drop(live);
    assert!(!apps.is_empty(), "a desktop box lists no launchable application");
    assert!(apps.iter().all(|(id, _)| crate::openrules::is_app(id)), "an id the rule table would refuse");

    // A rule built from the first entry, through the real validator, read back as a rule.
    let state = std::env::temp_dir().join(format!("flea-iced-rules-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let patch = crate::openrules::with_rule(&serde_json::json!({}), "NII.GZ", &apps[0].0).unwrap();
    let out = Command::new(&bin).env("XDG_STATE_HOME", &state).arg("--ui-state").arg(patch.to_string()).output().unwrap();
    let _ = std::fs::remove_dir_all(&state);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(crate::openrules::rules(&doc), vec![(".nii.gz".to_string(), apps[0].0.clone())]);
}

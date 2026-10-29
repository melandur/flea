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

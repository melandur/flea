// The chooser against the real `flea --backend`, with no window: the listing narrowed by a chip, a
// mark the backend reviews, a save probe over a file that exists, and Recent listed from a history
// file of its own. Skipped, and says so, when FLEA_BIN names no built backend.
use super::state::{Chooser, Effect};
use super::{recent, request};
use crate::wire::{self, Reply};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

struct Live {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    c: Chooser,
    answer: Option<String>,
    history: std::path::PathBuf,
}

impl Live {
    fn start(bin: &str, asked: &str, history: std::path::PathBuf) -> Live {
        let mut child = Command::new(bin).arg("--backend").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
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
        Live { child, stdin, lines, c: Chooser::new(request(asked), 10), answer: None, history }
    }

    fn run(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Send(r) => self.stdin.write_all(r.line().as_bytes()).unwrap(),
                Effect::ReadRecent => {
                    let paths = recent::read(&self.history);
                    let more = self.c.recent_read(paths);
                    self.run(more);
                }
                Effect::Finish(text) => self.answer = Some(text),
                _ => {}
            }
        }
    }

    // Feed replies to the chooser the way the shell does, until `done` holds.
    fn until(&mut self, what: &str, done: impl Fn(&Live) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(self) {
            let line = self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
            let out = match wire::parse(&line) {
                Some(Reply::Listed { n }) => {
                    self.c.listed(n);
                    vec![]
                }
                Some(Reply::Rows { start, rows, .. }) => {
                    self.c.rows_arrived(start, rows);
                    vec![]
                }
                Some(Reply::Error { r#where, msg, .. }) => self.c.failed(&r#where, &msg),
                Some(Reply::Picker(answer)) => self.c.checked(answer),
                _ => vec![],
            };
            self.run(out);
        }
    }

    fn names(&self) -> Vec<String> {
        self.c.rows.iter().map(|r| r.n.clone()).collect()
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(crate::wire::Request::Quit.line().as_bytes());
        let _ = self.child.wait();
    }
}

#[test]
fn a_chooser_round_trip_through_the_real_backend() {
    let Some(bin) = std::env::var("FLEA_BIN").ok().filter(|b| !b.is_empty()) else {
        eprintln!("live: FLEA_BIN is not set, so the chooser round trip was skipped");
        return;
    };
    let root = std::env::temp_dir().join(format!("flea-iced-picker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("a #.png"), b"12345").unwrap();
    std::fs::write(root.join("b.txt"), b"x").unwrap();
    let dir = root.to_str().unwrap().to_string();
    let history = root.join("sub").join("recently-used.xbel");
    std::fs::write(&history, format!(r#"<?xml version="1.0"?><xbel version="1.0">
<bookmark href="file://{dir}/a%20%23.png" visited="2026-10-01T00:00:00Z"/>
<bookmark href="file://{dir}/gone.png" visited="2026-10-02T00:00:00Z"/>
<bookmark href="smb://nas/x.png" visited="2026-10-03T00:00:00Z"/></xbel>"#)).unwrap();

    // The Images chip keeps the folder and the png, and drops the text file.
    let asked = format!(r#"{{"multiple":true,"folder":"{dir}","filters":[{{"label":"Images","globs":["*.png"],"mimes":[]}}]}}"#);
    let mut live = Live::start(&bin, &asked, history.clone());
    let e = live.c.start("/");
    live.run(e);
    live.until("the filtered listing", |l| !l.c.rows.is_empty());
    assert_eq!(live.names(), vec!["sub", "a #.png"]);
    let e = live.c.toggle_mark(1);
    live.run(e);
    live.until("the mark", |l| l.c.mark_request == 0);
    assert_eq!(live.c.marks.iter().map(|m| (m.path.clone(), m.bytes)).collect::<Vec<_>>(), vec![(format!("{dir}/a #.png"), 5)]);
    // All files lists the text file again, and the mark survives the re-listing.
    let e = live.c.set_filter(-1);
    live.run(e);
    live.until("the unfiltered listing", |l| l.c.rows.len() == 3 && l.c.mark_request == 0);
    assert_eq!(live.c.marks.len(), 1);
    // Recent lists what is still there, in the history's own order, and nothing remote.
    let e = live.c.open(super::RECENT);
    live.run(e);
    live.until("the Recent listing", |l| !l.c.rows.is_empty());
    assert_eq!(live.names(), vec![format!("{}/a #.png", dir.trim_start_matches('/'))]);
    let e = live.c.accept(false);
    live.run(e);
    live.until("the answer", |l| l.answer.is_some());
    let uri = super::file_uri(&format!("{dir}/a #.png"));
    assert_eq!(live.answer.take().unwrap(), format!(r#"{{"response":0,"uris":["{uri}"],"filter":-1}}"#));
    drop(live);

    // A save over a file that exists asks first, and the reviewed answer names that file.
    let mut live = Live::start(&bin, &format!(r#"{{"mode":"save","name":"b.txt","folder":"{dir}"}}"#), history);
    let e = live.c.start("/");
    live.run(e);
    live.until("the save probe", |l| l.c.save_ready());
    assert!(live.c.save_collision());
    assert_eq!(live.c.accept(false), vec![Effect::FocusCancel]);
    let e = live.c.accept(true);
    live.run(e);
    live.until("the reviewed save", |l| l.answer.is_some());
    assert!(live.answer.as_deref().unwrap().contains(&super::file_uri(&format!("{dir}/b.txt"))));
    drop(live);
    let _ = std::fs::remove_dir_all(&root);
}

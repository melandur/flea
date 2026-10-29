// ui/TrashMonitor.qml's two processes: `gio monitor --dir=trash:///` for as long as the Trash row
// or the view is up, and `gio info` for the count, Trash on other mounted volumes included. The
// monitor is tied to this window twice over, as TrashMonitor ties it: setpriv's parent-death signal,
// and a check that the shell's parent is still us, for a window that died before the signal was set.
use crate::trashview::Msg;
use crate::Message;
use iced::futures::{SinkExt, Stream, StreamExt};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// What the monitor reports: any change (the shell settles them), or that it stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum Watch {
    Changed,
    Failed,
}

fn owned(args: &[&str]) -> Command {
    let mut c = Command::new("setpriv");
    c.args(["--pdeathsig", "TERM", "--", "sh", "-c", r#"[ "$PPID" = "$1" ] || exit 1; shift; exec "$@""#, "flea-trash"])
        .arg(std::process::id().to_string())
        .args(args);
    c
}

// Dropping the subscription drops the stream, and this with it, which ends the monitor.
struct Kill(Arc<Mutex<Option<Child>>>);

impl Drop for Kill {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.lock().ok().and_then(|mut g| g.take()) {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn monitor() -> impl Stream<Item = Message> {
    iced::stream::channel(16, async move |mut out| {
        let spawned = owned(&["gio", "monitor", "--dir=trash:///"]).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null()).spawn();
        let Ok(mut child) = spawned else {
            let _ = out.send(Message::Trash(Msg::Watched(Watch::Failed))).await;
            return;
        };
        let stdout = child.stdout.take();
        let slot = Arc::new(Mutex::new(Some(child)));
        let _kill = Kill(slot.clone());
        let (tx, mut rx) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines() {
                    if line.is_err() || tx.unbounded_send(Watch::Changed).is_err() {
                        return;
                    }
                }
            }
            // The stream ended: the monitor exited, which is a failure only when it was not killed.
            // Polled with the lock let go between looks, so a drop that kills it is never kept waiting.
            for _ in 0..100 {
                let status = match slot.lock().ok().and_then(|mut g| g.as_mut().map(|c| c.try_wait())) {
                    None => return,
                    Some(Ok(s)) => s,
                    Some(Err(_)) => return,
                };
                if let Some(s) = status {
                    if !s.success() {
                        let _ = tx.unbounded_send(Watch::Failed);
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        while let Some(w) = rx.next().await {
            if out.send(Message::Trash(Msg::Watched(w))).await.is_err() {
                return;
            }
        }
    })
}

// TrashMonitor's query, bounded at 10 s; GIO prints "  trash::item-count: 12".
pub fn count() -> Option<usize> {
    let mut child = Command::new("gio")
        .args(["info", "--attributes=trash::item-count", "trash:///"])
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().ok()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().ok()?;
    out.status.success().then_some(())?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.trim().strip_prefix("trash::item-count:")).and_then(|n| n.trim().parse().ok())
}

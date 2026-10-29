// The Rust child, spawned exactly as ui/Backend.qml spawns it: `$FLEA_BIN --backend`, falling back
// to the bare `flea` a packaged install puts on PATH. It stays a separate process on purpose: its
// mallopt pin, its thumbnail threads and its drain-on-quit were all measured in a process of their
// own, and none of them was ever measured inside a GUI's allocator.
use crate::wire::{self, Reply, Request};
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone)]
pub enum Event {
    Reply(Reply),
    // A line this build cannot read, which ui/Backend.qml reports rather than drops.
    Unreadable,
    Exited(Option<i32>),
}

pub struct Backend {
    stdin: Mutex<Option<ChildStdin>>,
}

static EVENTS: OnceLock<Mutex<Option<mpsc::UnboundedReceiver<Event>>>> = OnceLock::new();

pub fn binary() -> String {
    std::env::var("FLEA_BIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "flea".into())
}

impl Backend {
    pub fn spawn() -> Result<Arc<Backend>, String> {
        let mut child: Child = Command::new(binary())
            .arg("--backend")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("the backend could not be started: {e}"))?;
        let stdout = child.stdout.take().ok_or("the backend has no stdout")?;
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::unbounded();
        EVENTS.get_or_init(|| Mutex::new(None)).lock().unwrap().replace(rx);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.is_empty() {
                    continue;
                }
                let event = wire::parse(&line).map(Event::Reply).unwrap_or(Event::Unreadable);
                if tx.unbounded_send(event).is_err() {
                    break;
                }
            }
            let code = child.wait().ok().and_then(|s| s.code());
            let _ = tx.unbounded_send(Event::Exited(code));
        });
        Ok(Arc::new(Backend { stdin: Mutex::new(stdin) }))
    }

    // Every request goes through here, so the protocol has exactly one author.
    pub fn send(&self, request: &Request) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap();
        let stdin = guard.as_mut().ok_or("the backend is not running")?;
        // FLEA_ICED_TRACE: every request on stderr, never stdout, so a test can count the wire.
        if std::env::var_os("FLEA_ICED_TRACE").is_some_and(|v| !v.is_empty()) {
            eprint!("flea-iced: sent {}", request.line());
        }
        stdin
            .write_all(request.line().as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|_| "the backend is not running".to_string())
    }
}

// Subscription::run takes a plain fn, so the receiver is handed over through the static.
pub fn events() -> impl Stream<Item = Event> {
    iced::stream::channel(64, async |mut out| {
        let rx = EVENTS.get().and_then(|m| m.lock().unwrap().take());
        let Some(mut rx) = rx else { return };
        while let Some(event) = rx.next().await {
            if out.send(event).await.is_err() {
                return;
            }
        }
    })
}

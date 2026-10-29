// The menu's send rows in the window, as ui/PaneMenuActions.qml, ui/Taildrop.qml, ui/LocalSend.qml,
// ui/ShareLink.qml and ui/NetworkMounts.qml's Dropbox half do it: a file menu opening asks every
// service again (the backend's providers, `tailscale status`, `dropbox-cli status`, LocalSend's
// devices when the list has gone cold), the rows are greyed until they answer and rebuilt as each
// one does, and a chosen row goes through activation.rs before it sends anything.
use crate::providers::{self, Peer, Providers};
use crate::{blocking, Flea, Message};
use iced::Task;
use serde_json::{json, Value};
use std::process::Command;
use std::time::{Duration, Instant};

// LocalSend.warmMs: discovery announces this box on the LAN, so a list is kept this long.
const LOCALSEND_WARM: Duration = Duration::from_secs(15);

#[derive(Debug, Default)]
pub struct Sends {
    pub providers: Providers,
    // A file menu opened: the providers block has been asked for and has not answered yet.
    pub formats_pending: bool,
    pub taildrop_peers: Vec<Peer>,
    pub taildrop_reason: String,
    pub taildrop_checking: bool,
    pub dropbox_path: String,
    pub dropbox_reason: String,
    pub dropbox_checking: bool,
    pub localsend_peers: Vec<(String, String)>,
    pub localsend_checking: bool,
    localsend_asked: Option<Instant>,
    pub sharing: bool,
}

impl Sends {
    pub fn refreshing(&self) -> bool {
        self.formats_pending || self.taildrop_checking || self.dropbox_checking
    }

    pub fn dropbox_ready(&self) -> bool {
        !self.dropbox_path.is_empty() && self.dropbox_reason.is_empty() && !self.dropbox_checking
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Taildrop(Option<i32>, String, String),
    Dropbox(Option<i32>, String, String),
    Shelf(Result<usize, String>),
    ShareLink(Result<(), String>),
}

// A helper run under coreutils' timeout, as the QML runs it: (status, stdout, stderr).
fn bounded(seconds: u32, program: String, args: Vec<&'static str>) -> (Option<i32>, String, String) {
    match Command::new("timeout").arg("--signal=KILL").arg(seconds.to_string()).arg(&program).args(args).output() {
        Ok(o) => (o.status.code(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into()),
        Err(_) => (None, String::new(), "the status helper could not start".into()),
    }
}

impl Flea {
    // ContextMenu's onSnapshotRequested for a file menu: everything a send row reads, asked again.
    pub(crate) fn refresh_sends(&mut self) -> Task<Message> {
        self.sends.formats_pending = true;
        let mut tasks = vec![self.ask_formats()];
        let hidden = self.effective_hidden();
        let s = &self.sends;
        let warm = s.localsend_asked.is_some_and(|at| at.elapsed() < LOCALSEND_WARM);
        // Discovery announces this box, so a row hidden in Settings never starts one.
        if s.providers.localsend_installed && !crate::menu::is_hidden(&hidden, "localsend") && !s.localsend_checking && !warm {
            self.sends.localsend_checking = true;
            let ask = json!({"c": "localsend", "op": "peers", "peer": "", "paths": [], "id": 1});
            tasks.push(self.run(vec![crate::pane::Effect::Send(crate::wire::Request::Json(ask))]));
        }
        Task::batch(tasks)
    }

    // ViewState.menuHidden: the stored set, with the shelf's row following shelf.enabled instead.
    pub(crate) fn effective_hidden(&self) -> Vec<String> {
        let mut hidden: Vec<String> = self.prefs.menu_hidden.iter().filter(|h| *h != "shelf").cloned().collect();
        if !self.prefs.shelf_enabled {
            hidden.push("shelf".into());
        }
        hidden
    }

    // The providers block of a `formats` answer: the account folder at once, then the two status
    // helpers, each only where its command exists.
    pub(crate) fn providers_answered(&mut self, v: &Value) -> Task<Message> {
        let p = providers::parse(v);
        let s = &mut self.sends;
        // Only a menu's own ask starts the helpers; the startup probe reads the facts and no more.
        let asked = std::mem::take(&mut s.formats_pending);
        let (path, reason) = providers::dropbox_account(p.dropbox_info.as_deref(), &p.dropbox_error);
        s.dropbox_path = path;
        s.dropbox_reason = if !p.dropbox_reason.is_empty() { p.dropbox_reason.clone() } else { reason };
        let mut tasks = vec![];
        if !asked {
            s.providers = p;
            return Task::none();
        }
        if !s.taildrop_checking && !p.taildrop_command.is_empty() && !p.send_command.is_empty() {
            s.taildrop_checking = true;
            let cmd = p.taildrop_command.clone();
            tasks.push(Task::perform(blocking(move || bounded(15, cmd, vec!["status", "--json"])), |(c, o, e)| Message::Send(Msg::Taildrop(c, o, e))));
        } else if p.taildrop_command.is_empty() || p.send_command.is_empty() {
            s.taildrop_peers.clear();
            s.taildrop_reason = [p.taildrop_reason.as_str(), "unavailable"].into_iter().find(|r| !r.is_empty()).unwrap().into();
        }
        if !s.dropbox_checking && !p.dropbox_command.is_empty() && !s.dropbox_path.is_empty() && s.dropbox_reason.is_empty() {
            s.dropbox_checking = true;
            let cmd = p.dropbox_command.clone();
            tasks.push(Task::perform(blocking(move || bounded(4, cmd, vec!["status"])), |(c, o, e)| Message::Send(Msg::Dropbox(c, o, e))));
        }
        s.providers = p;
        self.rebuild_menu();
        Task::batch(tasks)
    }

    pub(crate) fn localsend_answered(&mut self, v: &Value) -> Task<Message> {
        match v["t"].as_str() {
            Some("localsendpeers") => {
                self.sends.localsend_asked = Some(Instant::now());
                self.sends.localsend_checking = false;
                self.sends.localsend_peers = providers::localsend_peers(v);
                self.rebuild_menu();
                Task::none()
            }
            Some("localsendsent") => {
                let ok = v["ok"] == Value::Bool(true);
                self.show(providers::localsend_verdict(ok, v["reason"].as_str().unwrap_or("")), !ok)
            }
            _ => Task::none(),
        }
    }

    pub(crate) fn send_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Taildrop(code, out, err) => {
                let (peers, reason) = providers::taildrop_status(&out, code, &err);
                self.sends.taildrop_peers = peers;
                self.sends.taildrop_reason = reason;
                self.sends.taildrop_checking = false;
                self.rebuild_menu();
                Task::none()
            }
            Msg::Dropbox(code, out, err) => {
                self.sends.dropbox_reason = providers::dropbox_status(&out, code, &err);
                self.sends.dropbox_checking = false;
                self.rebuild_menu();
                Task::none()
            }
            Msg::Shelf(Ok(n)) => self.say(if n == 1 { "Added it to the shelf.".into() } else { format!("Added {n} items to the shelf.") }),
            Msg::Shelf(Err(line)) => self.show(line, true),
            Msg::ShareLink(result) => {
                self.sends.sharing = false;
                match result {
                    Ok(()) => self.say("Share link copied to the clipboard.".into()),
                    Err(line) => self.show(line, true),
                }
            }
        }
    }

    // A send row the backend vouched for: the paths it captured, or the cursor row when it captured
    // none (PaneMenuActions.targets), which is what the send rows have always meant without a selection.
    pub(crate) fn send_activated(&mut self, action: &str, menu_id: u64, paths: Vec<String>) -> Task<Message> {
        let cursor = self.pane.row_at(self.pane.cursor).cloned();
        let targets = if paths.is_empty() { cursor.iter().map(|r| crate::pane::join(&self.pane.path, &r.n)).collect() } else { paths.clone() };
        if action == "addToShelf" {
            if targets.is_empty() {
                return self.show("There is nothing to put on the shelf.".into(), true);
            }
            let bin = crate::backend::binary();
            return Task::perform(blocking(move || shelve(&bin, &targets)), |r| Message::Send(Msg::Shelf(r)));
        }
        if let Some(peer) = action.strip_prefix("localsend:") {
            if targets.is_empty() {
                return self.show("There is nothing to send.".into(), true);
            }
            let line = providers::localsend_sending(providers::peer_name(peer), &targets);
            let request = json!({"c": "localsend", "op": "send", "peer": peer, "paths": targets, "id": 2});
            let sent = self.run(vec![crate::pane::Effect::Send(crate::wire::Request::Json(request))]);
            return Task::batch([sent, self.say(line)]);
        }
        if let Some(peer_id) = action.strip_prefix("taildrop:") {
            // Taildrop sends the one cursor file, and the backend answered with exactly its path.
            let Some(peer) = self.sends.taildrop_peers.iter().find(|p| p.id == peer_id).cloned() else {
                return self.show("That Taildrop peer is no longer available.".into(), true);
            };
            let Some(path) = paths.first().filter(|p| p.starts_with('/') && cursor.as_ref().is_some_and(|r| !r.d)).cloned() else {
                return self.show("Cursor source was not validated; reopen the menu.".into(), true);
            };
            // argv-direct and detached: the OEM script owns its own success or failure notification.
            let started = Command::new(&self.sends.providers.send_command).arg(&peer.address).arg(&path)
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
            return match started {
                Ok(_) => self.say(format!("Sending {} to {}.", path.rsplit('/').next().unwrap_or(&path), peer.label)),
                Err(_) => self.show(format!("Taildrop · {}", if self.sends.taildrop_reason.is_empty() { "the send helper could not start" } else { &self.sends.taildrop_reason }), true),
            };
        }
        match action {
            // Ops.moveToDropbox: rows, not paths, because a selection reaches past the held window.
            "dropbox" => {
                let rows: Vec<usize> = self.menu_targets();
                let dest = self.sends.dropbox_path.clone();
                self.pane.sticky = format!("Moving {} to Dropbox", if rows.len() == 1 { "1 item".to_string() } else { format!("{} items", rows.len()) });
                self.run(vec![crate::pane::Effect::Send(crate::wire::Request::Json(json!({"c": "transfer", "op": "move", "rows": rows, "dest": dest, "menuId": menu_id})))])
            }
            "sharelink" => {
                if self.sends.sharing {
                    return self.show("A share link is still being copied; try again when it finishes.".into(), true);
                }
                let Some(path) = paths.first().filter(|p| p.starts_with('/') && !p.contains('\0')).cloned() else {
                    return self.show("Share link needs an absolute file path.".into(), true);
                };
                self.sends.sharing = true;
                Task::perform(blocking(move || share_link(&path)), |r| Message::Send(Msg::ShareLink(r)))
            }
            _ => Task::none(),
        }
    }
}

// The shelf's own CLI, the same call a drop onto the card makes; its last stderr line is the refusal.
fn shelve(bin: &str, paths: &[String]) -> Result<usize, String> {
    let out = Command::new(bin).arg("shelf").arg("add").args(paths).output().map_err(|_| "The shelf did not take that.".to_string())?;
    if out.status.success() {
        return Ok(paths.len());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let said = err.trim().lines().last().unwrap_or("").trim_start_matches("flea: ").to_string();
    Err(if said.is_empty() { "The shelf did not take that.".into() } else { said })
}

// ShareLink.qml: dropbox-cli prints the URL, and exit-zero error text is not one, so the shape decides.
pub(crate) fn share_link(path: &str) -> Result<(), String> {
    let out = Command::new("dropbox-cli").arg("sharelink").arg(path).output().map_err(|_| "The Dropbox share link helper could not start.".to_string())?;
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || !is_url(&url) {
        return Err("Dropbox could not make a share link for that file.".into());
    }
    match Command::new("wl-copy").arg("--").arg(&url).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err("The share link could not be copied to the clipboard.".into()),
        Err(_) => Err("The clipboard helper could not start; the share link was not copied.".into()),
    }
}

// ^https?://[^/\s?#]+(?:[/?#][^\s]*)?$
pub(crate) fn is_url(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("https://").or_else(|| text.strip_prefix("http://")) else { return false };
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..host_end];
    !host.is_empty() && !host.chars().any(char::is_whitespace) && !rest.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::is_url;

    #[test]
    fn only_a_url_shaped_answer_is_a_share_link() {
        assert!(is_url("https://www.dropbox.com/s/example/file.txt?dl=0"));
        assert!(is_url("http://host"));
        assert!(!is_url("Dropbox isn't running!") && !is_url("https://") && !is_url("https://a b/c") && !is_url("ftp://x"));
    }
}

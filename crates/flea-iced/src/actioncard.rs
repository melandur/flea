// The menu's dialog rows, as ui/MenuActionDialog.qml, ui/TrashConfirm.qml and ui/PermissionsDialog.qml
// hold them: Properties, New File, Move to, Copy to, Delete permanently and Permissions. Toolkit free:
// every input is a method and every output an Out the shell carries out, so the whole exchange with
// the backend's menuaction and permissions services is unit tested here rather than only driven.
//
// Every row but New File and Permissions acts on a captured selection: the card first sends
// `menuaction snapshot` naming the rows, and only its answer lets the card go on. A selection that
// changed in between is refused by the backend by identity, not by name (docs/protocol.md).
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Properties,
    NewFile,
    MoveTo,
    CopyTo,
    Delete,
    Permissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    // Waiting on the snapshot's answer.
    Snapshot,
    // A request is out and the card says so.
    Busy,
    Ready,
    // TrashConfirm: the destructive strip, Cancel focused first.
    Confirm,
    // The deletion was confirmed and is running; the card is not drawn and takes no keys.
    Committing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Send(Value),
    // The card is gone; the menuaction or permissions close went out with it where one was owed.
    Close,
    Say(String),
    Alert(String),
    // Re-read the listing and land on this name, or stay where the cursor is when it is empty.
    Refresh(String),
    // Ask again whether the confirmed set is still the set on disk, after CHECK_MS.
    Check(u64),
}

// TrashConfirm's identity-check cadence while the destructive strip is visible.
pub const CHECK_MS: u64 = 2500;

#[derive(Debug, Clone)]
pub struct Card {
    pub id: u64,
    pub kind: Kind,
    pub phase: Phase,
    pub text: String,
    pub error: String,
    pub facts: Value,
    // Confirm's buttons: 0 Cancel, 1 Delete.
    pub focus: u8,
    pub folder: String,
    pub path: String,
    check_gen: u64,
}

pub fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Properties => "Properties",
        Kind::NewFile => "New File",
        Kind::MoveTo => "Move to",
        Kind::CopyTo => "Copy to",
        Kind::Delete => "Delete permanently",
        Kind::Permissions => "Permissions",
    }
}

// Permissions.parse: three octal digits, or four with a leading zero; anything else waits.
pub fn parse_mode(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('0').filter(|d| d.len() == 3).unwrap_or(text);
    (digits.len() == 3 && digits.bytes().all(|b| (b'0'..=b'7').contains(&b))).then(|| u32::from_str_radix(digits, 8).ok()).flatten()
}

pub fn octal(mode: u32) -> String {
    format!("{mode:04o}")
}

fn leaf(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string()
}

impl Card {
    // `rows` are the targets the menu was raised over and `cursor` its cursor row; `path` is the
    // cursor row's absolute path, which Permissions inspects directly.
    pub fn open(kind: Kind, id: u64, folder: &str, rows: &[usize], cursor: usize, path: &str) -> (Card, Vec<Out>) {
        let mut card = Card {
            id,
            kind,
            phase: Phase::Ready,
            text: String::new(),
            error: String::new(),
            facts: Value::Null,
            focus: 0,
            folder: folder.to_string(),
            path: path.to_string(),
            check_gen: 0,
        };
        let out = match kind {
            Kind::NewFile => {
                card.text = "New File".into();
                vec![]
            }
            Kind::Permissions => {
                card.phase = Phase::Busy;
                vec![Out::Send(json!({"c": "permissions", "op": "inspect", "id": id, "path": path}))]
            }
            _ => {
                if matches!(kind, Kind::MoveTo | Kind::CopyTo) {
                    card.text = folder.to_string();
                }
                card.phase = Phase::Snapshot;
                vec![Out::Send(json!({"c": "menuaction", "op": "snapshot", "id": id, "rows": rows, "cursor": cursor}))]
            }
        };
        (card, out)
    }

    pub fn takes_text(&self) -> bool {
        matches!(self.kind, Kind::NewFile | Kind::MoveTo | Kind::CopyTo | Kind::Permissions)
    }

    pub fn visible(&self) -> bool {
        self.phase != Phase::Committing
    }

    pub fn can_submit(&self) -> bool {
        match self.kind {
            Kind::Permissions => self.phase == Phase::Ready && self.facts.get("ok") == Some(&json!(true)) && self.reason().is_empty() && parse_mode(&self.text).is_some(),
            Kind::NewFile | Kind::MoveTo | Kind::CopyTo => self.phase == Phase::Ready && !self.text.is_empty(),
            _ => false,
        }
    }

    pub fn reason(&self) -> String {
        self.facts.get("reason").and_then(Value::as_str).unwrap_or("").to_string()
    }

    pub fn input(&mut self, text: String) {
        if self.phase == Phase::Ready {
            self.text = text;
        }
    }

    // A check box: flips one bit of the mode being typed, and leaves text that does not parse alone.
    pub fn toggle_bit(&mut self, bit: u32) {
        if self.can_edit_mode() {
            if let Some(mode) = parse_mode(&self.text) {
                self.text = octal(mode ^ bit);
            }
        }
    }

    pub fn can_edit_mode(&self) -> bool {
        self.kind == Kind::Permissions && self.phase == Phase::Ready && self.facts.get("ok") == Some(&json!(true)) && self.reason().is_empty()
    }

    fn finish(&mut self) -> Vec<Out> {
        let close = if self.kind == Kind::Permissions {
            json!({"c": "permissions", "op": "close", "id": self.id})
        } else {
            json!({"c": "menuaction", "op": "close", "id": self.id})
        };
        vec![Out::Send(close), Out::Close]
    }

    // Escape, Cancel, Close and the backdrop. A file being created and a mode being applied are
    // not things a dismissal can take back, so the card holds until they answer.
    pub fn close(&mut self) -> Vec<Out> {
        // New File is only ever busy once created; Permissions is applying once it has facts.
        let committing = self.phase == Phase::Busy && (self.kind == Kind::NewFile || (self.kind == Kind::Permissions && !self.facts.is_null()));
        if committing {
            return vec![];
        }
        self.finish()
    }

    pub fn submit(&mut self) -> Vec<Out> {
        if !self.can_submit() {
            return vec![];
        }
        let text = self.text.clone();
        let request = match self.kind {
            Kind::NewFile => {
                if text == "." || text == ".." || text.contains('/') {
                    self.error = "Enter one filename without a path separator.".into();
                    return vec![];
                }
                json!({"c": "newfile", "op": "newFile", "id": self.id, "path": self.folder, "name": text})
            }
            Kind::MoveTo | Kind::CopyTo => {
                if !text.starts_with('/') {
                    self.error = "Enter an absolute destination folder.".into();
                    return vec![];
                }
                let action = if self.kind == Kind::MoveTo { "moveTo" } else { "copyTo" };
                json!({"c": "menuaction", "op": "validate", "id": self.id, "action": action, "dest": text})
            }
            Kind::Permissions => json!({"c": "permissions", "op": "apply", "id": self.id, "mode": text}),
            _ => return vec![],
        };
        self.error.clear();
        self.phase = Phase::Busy;
        vec![Out::Send(request)]
    }

    // The destructive strip's Delete, which only a deliberate move off Cancel reaches.
    pub fn confirm(&mut self) -> Vec<Out> {
        let token = self.facts.get("token").and_then(Value::as_u64).unwrap_or(0);
        if self.phase != Phase::Confirm || token == 0 {
            return vec![];
        }
        self.phase = Phase::Committing;
        vec![Out::Send(json!({"c": "menuaction", "op": "delete", "id": self.id, "token": token}))]
    }

    pub fn check_due(&mut self, generation: u64) -> Vec<Out> {
        let token = self.facts.get("token").and_then(Value::as_u64).unwrap_or(0);
        if self.phase != Phase::Confirm || generation != self.check_gen {
            return vec![];
        }
        vec![Out::Send(json!({"c": "menuaction", "op": "checkDelete", "id": self.id, "token": token}))]
    }

    fn refresh_deletion(&mut self) -> Vec<Out> {
        self.phase = Phase::Busy;
        self.error.clear();
        self.focus = 0;
        vec![Out::Send(json!({"c": "menuaction", "op": "refreshDelete", "id": self.id}))]
    }

    fn arm_check(&mut self) -> Out {
        self.check_gen += 1;
        Out::Check(self.check_gen)
    }

    pub fn receive_menu(&mut self, v: &Value) -> Vec<Out> {
        let op = v.get("op").and_then(Value::as_str).unwrap_or("");
        if v.get("id").and_then(Value::as_u64) != Some(self.id) || op == "close" || self.kind == Kind::Permissions {
            return vec![];
        }
        let ok = v.get("ok") == Some(&json!(true));
        let error = v.get("error").and_then(Value::as_str).unwrap_or("").to_string();
        if self.phase == Phase::Committing && op != "delete" {
            return vec![];
        }
        if op == "snapshot" {
            if !ok {
                let line = if error.is_empty() { "Selected items changed; reopen the menu.".into() } else { error };
                return vec![Out::Alert(line), Out::Close];
            }
            return match self.kind {
                Kind::Properties => {
                    self.phase = Phase::Busy;
                    vec![Out::Send(json!({"c": "menuaction", "op": "properties", "id": self.id}))]
                }
                Kind::Delete => {
                    self.phase = Phase::Busy;
                    vec![Out::Send(json!({"c": "menuaction", "op": "prepareDelete", "id": self.id}))]
                }
                _ => {
                    self.phase = Phase::Ready;
                    vec![]
                }
            };
        }
        if !ok {
            if op == "delete" {
                // The deletion never started; the operator is told why rather than shown a card again.
                let mut out = vec![Out::Alert(if error.is_empty() { "The requested action failed.".into() } else { error })];
                out.extend(self.finish());
                return out;
            }
            self.phase = Phase::Ready;
            self.error = if error.is_empty() { "The requested action failed.".into() } else { error };
            return vec![];
        }
        let stale = v.get("stale") == Some(&json!(true));
        match op {
            "checkDelete" if v.get("valid") == Some(&json!(true)) => vec![self.arm_check()],
            "checkDelete" => self.refresh_deletion(),
            "delete" if stale => {
                self.facts = Value::Null;
                self.refresh_deletion()
            }
            "prepareDelete" | "refreshDelete" => {
                if v.get("count").and_then(Value::as_u64) == Some(0) {
                    let mut out = vec![Out::Say("Nothing selected is left to delete.".into())];
                    out.extend(self.finish());
                    return out;
                }
                self.facts = v.clone();
                self.phase = Phase::Confirm;
                self.focus = 0;
                vec![self.arm_check()]
            }
            "delete" => {
                let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
                let count = self.facts.get("count").and_then(Value::as_u64).unwrap_or(0);
                let mut line = format!("Deleted {} of {count}", n("deleted"));
                if n("failed") > 0 {
                    line += &format!(" · {} failed", n("failed"));
                }
                if v.get("cancelled") == Some(&json!(true)) {
                    line += " · cancelled";
                }
                let failed = n("failed") > 0;
                if failed && !error.is_empty() {
                    line += &format!(" · {error}");
                }
                let mut out = vec![if failed { Out::Alert(line) } else { Out::Say(line) }, Out::Refresh(String::new())];
                out.extend(self.finish());
                out
            }
            "properties" => {
                self.facts = v.clone();
                self.phase = Phase::Ready;
                vec![]
            }
            // The captured sources move or copy straight from the snapshot, before close expires it.
            "validate" => {
                let moving = self.kind == Kind::MoveTo;
                let dest = v.get("dest").and_then(Value::as_str).unwrap_or(&self.text).to_string();
                let mut out = vec![Out::Send(json!({"c": "transfer", "op": if moving { "move" } else { "copy" }, "menuId": self.id, "dest": dest}))];
                out.extend(self.finish());
                out
            }
            "newFile" => {
                let path = v.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                let mut out = vec![Out::Say("File created.".into()), Out::Refresh(leaf(&path))];
                out.extend(self.finish());
                out
            }
            _ => vec![],
        }
    }

    pub fn receive_permissions(&mut self, v: &Value) -> Vec<Out> {
        let op = v.get("op").and_then(Value::as_str).unwrap_or("");
        if self.kind != Kind::Permissions || v.get("id").and_then(Value::as_u64) != Some(self.id) || op == "close" {
            return vec![];
        }
        self.phase = Phase::Ready;
        if v.get("ok") != Some(&json!(true)) {
            self.error = v.get("error").and_then(Value::as_str).filter(|e| !e.is_empty()).unwrap_or("Could not change permissions.").to_string();
            return vec![];
        }
        if op == "apply" {
            let mut out = vec![Out::Say("Permissions changed.".into()), Out::Refresh(String::new())];
            out.extend(self.finish());
            return out;
        }
        self.facts = v.clone();
        self.text = v.get("mode").and_then(Value::as_str).unwrap_or("").to_string();
        vec![]
    }
}

#[cfg(test)]
#[path = "actioncard_tests.rs"]
mod tests;

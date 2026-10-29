// The Trash view, ui/TrashView.qml: the backend's one trashbrowse session listed a window at a time,
// a selection held by URI and the identity each row was listed with, restore, and permanent deletion
// that only ever goes through a prepared confirmation the backend reviewed. One request is in flight
// at a time and a reply is taken only when it answers that request. Toolkit free; trashview.rs
// draws it and trashshell.rs runs its effects. ui/js/TrashDates.js's two readers are at the end.
use serde_json::{json, Value};
use std::collections::BTreeMap;

// The most rows the backend answers a window with.
pub const MAX_WINDOW: usize = 350;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub uri: String,
    pub original: String,
    pub size: u64,
    pub deleted: String,
    pub directory: bool,
    pub icon: String,
    pub identity: String,
    pub bulk_selected: bool,
}

fn row(v: &Value) -> Row {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Row {
        uri: s("uri"),
        original: s("original"),
        size: v.get("size").and_then(Value::as_u64).unwrap_or(0),
        deleted: s("deleted"),
        directory: v.get("directory").and_then(Value::as_bool).unwrap_or(false),
        icon: s("icon"),
        identity: s("identity"),
        bulk_selected: v.get("bulkSelected").and_then(Value::as_bool).unwrap_or(false),
    }
}

// TrashConfirm's snapshot: the token the backend reviewed, whether it is the whole Trash, its count
// and bytes; and which button the keyboard is on, Cancel until the destructive one is reached.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub token: u64,
    pub all: bool,
    pub count: u64,
    pub bytes: u64,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Send(Value),
    Status(String, bool),
    // An operation's headline, its detail lines, and whether it failed; the rail's count follows it.
    Result(String, String, bool),
    // The view closed; the keyboard goes back to the listing.
    Closed,
    // Put the row in view.
    Reveal(usize),
}

// A plain press replaces the selection, a Ctrl press or Space toggles one row. TrashView's third
// form, the shift-arrow extend, and its dd pair have no key since the key cut, so neither is here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Extend {
    No,
    Toggle,
}

#[derive(Debug, Default)]
pub struct Trash {
    pub opened: bool,
    pub total: usize,
    pub total_bytes: u64,
    pub bytes_ready: bool,
    pub bytes_partial: bool,
    pub first: usize,
    pub rows: Vec<Row>,
    // uri -> true for a marked row, false for one taken out of a select-all.
    selected: BTreeMap<String, bool>,
    identities: BTreeMap<String, String>,
    all_selected: bool,
    selection_token: u64,
    selection_count: usize,
    pub cursor: usize,
    // TrashMonitor's count, which the rail's Trash menu greys its rows on.
    pub count: Option<usize>,
    request_id: u64,
    pending_op: String,
    operation_active: bool,
    refresh_pending: bool,
    confirming: bool,
    initial_action: String,
    pending_selection: Option<(usize, Extend)>,
    pub error_text: String,
    // What the confirmation being prepared was asked over, so a changed Trash can re-prepare it.
    confirming_ask: Value,
    // The token the last prepare answered; a re-prepare names it, so the backend keeps that URI set.
    confirming_token: u64,
    pub confirmation: Option<Confirm>,
    // The rows the list shows, and how far it is scrolled, in rows; the window follows them.
    pub window_rows: usize,
    pub top: usize,
    pub visible: usize,
}

impl Trash {
    pub fn busy(&self) -> bool {
        !self.pending_op.is_empty()
    }

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    fn send(&mut self, op: &str, mut fields: Value) -> Out {
        self.request_id += 1;
        fields["c"] = json!("trashbrowse");
        fields["op"] = json!(op);
        fields["id"] = json!(self.request_id);
        self.pending_op = op.to_string();
        if op == "restore" || op == "delete" {
            self.operation_active = true;
        }
        Out::Send(fields)
    }

    // TrashHost.open: an action asked for from outside runs once the first listing lands.
    pub fn open(&mut self, action: &str) -> Vec<Out> {
        if self.operation_active {
            self.opened = true;
            return vec![];
        }
        let rows = self.window_rows.max(1);
        *self = Trash { opened: true, initial_action: action.to_string(), window_rows: rows, visible: self.visible, request_id: self.request_id, count: self.count, ..Default::default() };
        vec![self.send("list", json!({"start": 0, "count": rows, "recover": true}))]
    }

    pub fn close(&mut self) -> Vec<Out> {
        self.opened = false;
        self.confirming = false;
        self.confirmation = None;
        self.initial_action.clear();
        let mut out = vec![];
        if !self.operation_active {
            out.push(self.send("cancel", json!({"clearSelection": true})));
        }
        out.push(Out::Closed);
        out
    }

    pub fn refresh(&mut self) -> Vec<Out> {
        if !self.opened {
            return vec![];
        }
        if self.busy() {
            self.refresh_pending = true;
            return vec![];
        }
        if self.confirming {
            return self.source_changed();
        }
        self.refresh_pending = false;
        self.bytes_ready = false;
        let uris: Vec<&String> = self.selected.keys().collect();
        let ask = json!({"start": self.first, "count": self.window_rows, "uris": uris});
        vec![self.send("list", ask)]
    }

    // The Trash changed under the view: a confirmation is prepared again over the same ask, so the
    // backend reviews what is there now, and anything else re-lists.
    pub fn source_changed(&mut self) -> Vec<Out> {
        if !self.opened {
            return vec![];
        }
        self.confirmation = None;
        if self.busy() {
            self.refresh_pending = true;
            return vec![];
        }
        self.refresh_pending = false;
        if self.confirming {
            let mut ask = self.confirming_ask.clone();
            ask["refreshToken"] = json!(self.confirming_token);
            return vec![self.send("prepare", ask)];
        }
        self.refresh()
    }

    // ensureWindow: a window is asked for only when the rows on screen run past the one held.
    pub fn ensure_window(&mut self) -> Vec<Out> {
        if !self.opened || self.total == 0 || self.busy() || self.confirmation.is_some() {
            return vec![];
        }
        let end = self.total.min(self.top + self.visible.max(1));
        if self.top < self.first || end > self.first + self.rows.len() {
            return vec![self.send("window", json!({"start": self.top, "count": self.window_rows}))];
        }
        vec![]
    }

    pub fn row_at(&self, index: usize) -> Option<&Row> {
        index.checked_sub(self.first).and_then(|i| self.rows.get(i))
    }

    fn selected_uris(&self) -> Vec<String> {
        self.selected.iter().filter(|(_, v)| **v).map(|(k, _)| k.clone()).collect()
    }

    fn excluded_uris(&self) -> Vec<String> {
        self.selected.iter().filter(|(_, v)| !**v).map(|(k, _)| k.clone()).collect()
    }

    pub fn is_selected(&self, uri: &str) -> bool {
        match self.selected.get(uri) {
            Some(v) => *v,
            None => self.all_selected && self.rows.iter().any(|r| r.uri == uri && r.bulk_selected),
        }
    }

    pub fn selected_count(&self) -> usize {
        let marked = self.selected_uris().len();
        if self.all_selected {
            self.selection_count.saturating_sub(self.excluded_uris().len()) + marked
        } else {
            marked
        }
    }

    // Ctrl+A: the backend takes a snapshot of the whole Trash and answers its token.
    pub fn select_all(&mut self) -> Vec<Out> {
        if self.operation_active || self.confirming || self.pending_op == "select" || self.total == 0 {
            return vec![];
        }
        self.all_selected = false;
        self.selection_token = 0;
        self.selection_count = 0;
        self.selected.clear();
        self.identities.clear();
        vec![self.send("select", json!({}))]
    }

    pub fn choose(&mut self, index: isize, extend: Extend) -> Vec<Out> {
        if self.total == 0 || self.confirming || self.operation_active {
            return vec![];
        }
        let mut out = vec![];
        if self.pending_op == "select" {
            out.push(self.send("cancel", json!({"clearSelection": true})));
        }
        self.cursor = index.clamp(0, self.total as isize - 1) as usize;
        out.push(Out::Reveal(self.cursor));
        let Some(item) = self.row_at(self.cursor).cloned() else {
            self.pending_selection = Some((self.cursor, extend));
            out.extend(self.ensure_window());
            return out;
        };
        self.pending_selection = None;
        if extend == Extend::No {
            self.selected.clear();
            self.identities.clear();
        }
        let on = extend == Extend::No || !self.is_selected(&item.uri);
        self.selected.insert(item.uri.clone(), on);
        if on {
            self.identities.insert(item.uri.clone(), item.identity.clone());
        } else {
            self.identities.remove(&item.uri);
        }
        if self.all_selected && extend != Extend::No && on && item.bulk_selected {
            self.selected.remove(&item.uri);
            self.identities.remove(&item.uri);
        }
        if extend == Extend::No {
            self.all_selected = false;
            self.selection_token = 0;
            self.selection_count = 0;
        }
        out
    }

    fn identities_of(&self, uris: &[String]) -> Vec<String> {
        uris.iter().map(|u| self.identities.get(u).cloned().unwrap_or_default()).collect()
    }

    pub fn restore(&mut self, all: bool) -> Vec<Out> {
        if self.busy() || if all { self.total == 0 } else { self.selected_count() == 0 } {
            return vec![];
        }
        self.confirming = false;
        let uris = self.selected_uris();
        let token = if !all && self.all_selected { self.selection_token } else { 0 };
        let exclude = if all { vec![] } else { self.excluded_uris() };
        let ids = self.identities_of(&uris);
        vec![self.send("restore", json!({"all": all, "selectionToken": token, "uris": uris, "exclude": exclude, "identities": ids}))]
    }

    pub fn prepare(&mut self, all: bool) -> Vec<Out> {
        if self.busy() || if all { self.total == 0 } else { self.selected_count() == 0 } {
            return vec![];
        }
        self.confirming = true;
        self.confirming_token = 0;
        let uris = self.selected_uris();
        let token = if !all && self.all_selected { self.selection_token } else { 0 };
        let exclude = if all { vec![] } else { self.excluded_uris() };
        let ids = self.identities_of(&uris);
        self.confirming_ask = json!({"all": all, "emptyTrash": all, "uris": uris, "exclude": exclude, "identities": ids, "selectionToken": token});
        let ask = self.confirming_ask.clone();
        vec![self.send("prepare", ask)]
    }

    // TrashConfirm: Enter on Cancel cancels, so a reflexive Enter never deletes.
    pub fn confirm_activate(&mut self) -> Vec<Out> {
        let Some(c) = self.confirmation.take() else { return vec![] };
        self.confirming = false;
        if c.destructive {
            vec![self.send("delete", json!({"token": c.token}))]
        } else {
            vec![self.send("cancel", json!({}))]
        }
    }

    pub fn confirm_cancel(&mut self) -> Vec<Out> {
        if self.confirmation.take().is_none() {
            return vec![];
        }
        self.confirming = false;
        vec![self.send("cancel", json!({}))]
    }

    // GIO's monitor reports top-level changes; an open confirmation also checks nested identities.
    pub fn check_tick(&mut self) -> Vec<Out> {
        match &self.confirmation {
            Some(c) if self.opened && !self.busy() => {
                let token = c.token;
                vec![self.send("check", json!({"token": token}))]
            }
            _ => vec![],
        }
    }
}

#[path = "trashreceive.rs"]
mod receive;

// TrashDates.location: the folder an item came from, home spelled ~.
pub fn location(original: &str, home: &str) -> String {
    let Some(cut) = original.rfind('/') else { return "Unknown".into() };
    let folder = if cut == 0 { "/" } else { &original[..cut] };
    if !home.is_empty() && (folder == home || folder.starts_with(&format!("{home}/"))) {
        return format!("~{}", &folder[home.len()..]);
    }
    folder.to_string()
}

// TrashDates.deleted: GIO's trash::deletion-date, "2026-09-08T10:00:00" in local time, in the
// product's one date form; anything unreadable is Unknown and never a day in 1970.
pub fn deleted(text: &str) -> String {
    match chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") {
        Ok(t) => t.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => "Unknown".into(),
    }
}

pub fn name(original: &str) -> &str {
    original.rsplit('/').next().unwrap_or(original)
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;

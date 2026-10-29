// The Open with card, as ui/OpenWithDialog.qml and ui/js/OpenWith.js hold it: two groups under
// eyebrows, the type's own registry first with the desktop's default at its head, then every
// installed application, a search that filters both, the "always" box that is the one place a
// default is written, and the `menuaction openWith` launch. Toolkit free.
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub id: String,
    pub label: String,
    pub default: bool,
}

// One drawn row: an eyebrow caption, or an application at its cursor seat.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Eyebrow(String),
    App(App, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Send(Value),
    Close,
}

#[derive(Debug, Clone)]
pub struct OpenWith {
    pub id: u64,
    pub path: String,
    pub kind: String,
    pub handlers: Vec<App>,
    pub installed: Vec<App>,
    pub query: String,
    pub cursor: usize,
    pub always: bool,
    // 0 the search, 1 the list, 2 the always box, 3 Cancel, 4 Open.
    pub focus: u8,
    pub busy: bool,
    pub committing: bool,
    pub error: String,
    snapshotted: bool,
}

fn apps(v: &Value, key: &str) -> Vec<App> {
    v.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|a| {
        Some(App { id: a.get("id")?.as_str()?.to_string(), label: a.get("label")?.as_str()?.to_string(), default: a.get("default") == Some(&json!(true)) })
    }).collect()
}

pub fn rows(handlers: &[App], installed: &[App], kind: &str, query: &str) -> Vec<Row> {
    let needle = query.trim().to_lowercase();
    let matching = |list: &[App]| -> Vec<App> { list.iter().filter(|a| needle.is_empty() || a.label.to_lowercase().contains(&needle)).cloned().collect() };
    let mut out = vec![];
    let mut seat = 0;
    for (title, list) in [(format!("Registered for {kind}"), matching(handlers)), ("All applications".to_string(), matching(installed))] {
        if list.is_empty() {
            continue;
        }
        out.push(Row::Eyebrow(title));
        for a in list {
            out.push(Row::App(a, seat));
            seat += 1;
        }
    }
    out
}

pub fn no_match(query: &str) -> String {
    format!("No application matches “{}”. Clear the search to see every installed application.", query.trim())
}

impl OpenWith {
    pub fn open(id: u64, rows: &[usize], cursor: usize) -> (OpenWith, Value) {
        let card = OpenWith {
            id,
            path: String::new(),
            kind: String::new(),
            handlers: vec![],
            installed: vec![],
            query: String::new(),
            cursor: 0,
            always: false,
            focus: 1,
            busy: true,
            committing: false,
            error: String::new(),
            snapshotted: false,
        };
        (card, json!({"c": "menuaction", "op": "snapshot", "id": id, "rows": rows, "cursor": cursor}))
    }

    pub fn rows(&self) -> Vec<Row> {
        rows(&self.handlers, &self.installed, &self.kind, &self.query)
    }

    pub fn applications(&self) -> Vec<App> {
        self.rows().into_iter().filter_map(|r| if let Row::App(a, _) = r { Some(a) } else { None }).collect()
    }

    pub fn chosen(&self) -> Option<App> {
        self.applications().into_iter().nth(self.cursor)
    }

    pub fn can_submit(&self) -> bool {
        !self.busy && self.chosen().is_some()
    }

    pub fn name(&self) -> String {
        self.path.rsplit('/').next().unwrap_or("").to_string()
    }

    pub fn search(&mut self, query: String) {
        self.query = query;
        self.cursor = 0;
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.applications().len();
        if count == 0 {
            return;
        }
        self.focus = 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, count as isize - 1) as usize;
    }

    pub fn step_focus(&mut self, back: bool) {
        if !self.busy {
            self.focus = (self.focus + if back { 4 } else { 1 }) % 5;
        }
    }

    pub fn receive(&mut self, v: &Value) -> Vec<Out> {
        if v.get("id").and_then(Value::as_u64) != Some(self.id) {
            return vec![];
        }
        let ok = v.get("ok") == Some(&json!(true));
        let error = v.get("error").and_then(Value::as_str).unwrap_or("").to_string();
        match v.get("op").and_then(Value::as_str).unwrap_or("") {
            "snapshot" if !self.snapshotted => {
                if !ok {
                    self.busy = false;
                    self.error = if error.is_empty() { "Selected items changed; reopen the menu.".into() } else { error };
                    return vec![];
                }
                self.snapshotted = true;
                // installed: the whole catalogue, which only this card draws.
                vec![Out::Send(json!({"c": "menuaction", "op": "applications", "id": self.id, "installed": true}))]
            }
            "applications" => {
                self.busy = false;
                if !ok {
                    self.error = if error.is_empty() { "The registry could not be read.".into() } else { error };
                    return vec![];
                }
                let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                self.path = s("path");
                self.kind = s("kind");
                self.handlers = apps(v, "applications");
                self.installed = apps(v, "installed");
                self.cursor = 0;
                vec![]
            }
            "openWith" => {
                self.busy = false;
                self.committing = false;
                if ok {
                    return self.close();
                }
                if v.get("cancelled") != Some(&json!(true)) {
                    self.error = if error.is_empty() { "The application could not be opened.".into() } else { error };
                }
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn commit(&mut self) -> Vec<Out> {
        let Some(app) = self.chosen().filter(|_| self.can_submit()) else { return vec![] };
        self.error.clear();
        self.busy = true;
        self.committing = true;
        vec![Out::Send(json!({"c": "menuaction", "op": "openWith", "id": self.id, "application": app.id, "always": self.always}))]
    }

    // The close op expires the snapshot and cancels the owned launcher, so Cancel during a launch
    // ends the child rather than leaving it running behind a card that is already gone.
    pub fn close(&mut self) -> Vec<Out> {
        vec![Out::Send(json!({"c": "menuaction", "op": "close", "id": self.id})), Out::Close]
    }
}

#[cfg(test)]
#[path = "openwith_tests.rs"]
mod tests;

// A menu row that acts at once on its captured selection, as ui/PaneMenuActions.qml activate() and
// validateActivation() do it: `menuaction snapshot` naming the rows, then `activate` naming the
// action, and only its answer, which carries the captured paths where the action reads paths,
// lets the action run. The archive and convert requests then carry the id as their menuId, so the
// backend checks the sources against the snapshot by identity. No card: nothing is drawn meanwhile.
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Activation {
    pub id: u64,
    pub action: String,
    // Move to Dropbox and Copy Share Link name the account folder, which the backend captures with
    // `providerDestination` between the snapshot and the activation and checks again at both.
    dest: Option<String>,
    snapshotted: bool,
    placed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Nothing,
    Send(Value),
    // The backend vouched for the selection: run `action`, with the paths it captured.
    Done { action: String, paths: Vec<String> },
    Refused(String),
}

impl Activation {
    pub fn to(id: u64, action: &str, rows: &[usize], cursor: usize, dest: Option<String>) -> (Activation, Value) {
        let snapshot = json!({"c": "menuaction", "op": "snapshot", "id": id, "rows": rows, "cursor": cursor});
        (Activation { id, action: action.to_string(), placed: dest.is_none(), dest, snapshotted: false }, snapshot)
    }

    fn activate(&self) -> Value {
        match &self.dest {
            Some(dest) => json!({"c": "menuaction", "op": "activate", "id": self.id, "action": self.action, "dest": dest}),
            None => json!({"c": "menuaction", "op": "activate", "id": self.id, "action": self.action}),
        }
    }

    pub fn receive(&mut self, v: &Value) -> Step {
        let op = v.get("op").and_then(Value::as_str).unwrap_or("");
        if v.get("id").and_then(Value::as_u64) != Some(self.id) {
            return Step::Nothing;
        }
        let ok = v.get("ok") == Some(&json!(true));
        let error = v.get("error").and_then(Value::as_str).filter(|e| !e.is_empty()).unwrap_or("Selected items changed; reopen the menu.").to_string();
        match (op, self.snapshotted) {
            ("snapshot", false) if ok => {
                self.snapshotted = true;
                match &self.dest {
                    Some(dest) if !self.placed => Step::Send(json!({"c": "menuaction", "op": "providerDestination", "id": self.id, "dest": dest})),
                    _ => Step::Send(self.activate()),
                }
            }
            ("providerDestination", true) if ok && !self.placed => {
                self.placed = true;
                Step::Send(self.activate())
            }
            ("providerDestination", true) if !self.placed => Step::Refused(error),
            ("activate", true) if ok => {
                let paths = v.get("paths").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).collect();
                Step::Done { action: self.action.clone(), paths }
            }
            ("snapshot", false) | ("activate", true) => Step::Refused(error),
            _ => Step::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_then_an_activation_hands_over_the_captured_paths() {
        let (mut a, snapshot) = Activation::to(9, "compress:zip", &[1, 2], 1, None);
        assert_eq!(snapshot, json!({"c": "menuaction", "op": "snapshot", "id": 9, "rows": [1, 2], "cursor": 1}));
        assert_eq!(a.receive(&json!({"id": 8, "op": "snapshot", "ok": true})), Step::Nothing);
        assert_eq!(a.receive(&json!({"id": 9, "op": "snapshot", "ok": true, "count": 2})), Step::Send(json!({"c": "menuaction", "op": "activate", "id": 9, "action": "compress:zip"})));
        let done = a.receive(&json!({"id": 9, "op": "activate", "ok": true, "action": "compress:zip", "paths": ["/d/a", "/d/b"]}));
        assert_eq!(done, Step::Done { action: "compress:zip".into(), paths: vec!["/d/a".into(), "/d/b".into()] });
    }

    #[test]
    fn a_provider_destination_is_captured_between_the_snapshot_and_the_activation() {
        let (mut a, _) = Activation::to(5, "dropbox", &[0], 0, Some("/home/u/Dropbox".into()));
        assert_eq!(a.receive(&json!({"id": 5, "op": "snapshot", "ok": true})), Step::Send(json!({"c": "menuaction", "op": "providerDestination", "id": 5, "dest": "/home/u/Dropbox"})));
        assert_eq!(a.receive(&json!({"id": 5, "op": "providerDestination", "ok": true})), Step::Send(json!({"c": "menuaction", "op": "activate", "id": 5, "action": "dropbox", "dest": "/home/u/Dropbox"})));
        assert!(matches!(a.receive(&json!({"id": 5, "op": "activate", "ok": true, "paths": [], "dest": "/home/u/Dropbox"})), Step::Done { .. }));
        let (mut b, _) = Activation::to(6, "sharelink", &[0], 0, Some("/x".into()));
        b.receive(&json!({"id": 6, "op": "snapshot", "ok": true}));
        assert_eq!(b.receive(&json!({"id": 6, "op": "providerDestination", "ok": false, "error": "Dropbox account folder is not a directory."})), Step::Refused("Dropbox account folder is not a directory.".into()));
    }

    #[test]
    fn a_refusal_at_either_step_is_said_and_nothing_runs() {
        let (mut a, _) = Activation::to(9, "extract", &[0], 0, None);
        assert_eq!(a.receive(&json!({"id": 9, "op": "activate", "ok": true})), Step::Nothing);
        assert_eq!(a.receive(&json!({"id": 9, "op": "snapshot", "ok": false})), Step::Refused("Selected items changed; reopen the menu.".into()));
        let (mut a, _) = Activation::to(9, "extract", &[0], 0, None);
        a.receive(&json!({"id": 9, "op": "snapshot", "ok": true}));
        assert_eq!(a.receive(&json!({"id": 9, "op": "activate", "ok": false, "error": "Menu selection expired; reopen the menu."})), Step::Refused("Menu selection expired; reopen the menu.".into()));
    }
}

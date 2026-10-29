// TrashView.receive: a reply is taken only when it answers the one request in flight, and what each
// op's answer changes, including the selection a finished operation leaves on the items that failed.
use super::{row, Confirm, Out, Trash};
use serde_json::{json, Value};

impl Trash {
    pub fn receive(&mut self, m: &Value) -> Vec<Out> {
        let op = m.get("op").and_then(Value::as_str).unwrap_or("");
        let id = m.get("id").and_then(Value::as_u64).unwrap_or(0);
        if (!self.opened && !self.operation_active) || id != self.request_id || op != self.pending_op {
            return vec![];
        }
        self.pending_op.clear();
        let num = |k: &str| m.get(k).and_then(Value::as_u64).unwrap_or(0);
        let flag = |k: &str| m.get(k).and_then(Value::as_bool).unwrap_or(false);
        if !flag("ok") {
            // A delete whose confirmation went stale is reviewed again rather than failed.
            if flag("stale") && self.opened {
                self.operation_active = false;
                self.confirming = true;
                return self.source_changed();
            }
            self.error_text = m.get("error").and_then(Value::as_str).unwrap_or("Trash operation failed.").to_string();
            self.confirming = false;
            self.confirmation = None;
            self.operation_active = false;
            return vec![Out::Status(self.error_text.clone(), true)];
        }
        let mut out = vec![];
        match op {
            "list" | "window" => {
                self.total = num("total") as usize;
                self.first = num("start") as usize;
                self.rows = m.get("rows").and_then(Value::as_array).map(|a| a.iter().map(row).collect()).unwrap_or_default();
                if self.all_selected {
                    self.selection_count = num("selectionCount") as usize;
                    if flag("selectionStale") || num("selectionToken") != self.selection_token {
                        self.clear_selection();
                        out.push(Out::Status("Trash selection changed; select the items again.".into(), true));
                    }
                }
                self.cursor = self.cursor.min(self.total.saturating_sub(1));
                if op == "list" {
                    out.extend(self.listed(m));
                }
                if let Some((at, extend)) = self.pending_selection {
                    if self.row_at(at).is_some() {
                        out.extend(self.choose(at as isize, extend));
                    }
                }
            }
            "select" => {
                self.selection_token = num("selectionToken");
                self.selection_count = num("selectionCount") as usize;
                self.all_selected = self.selection_token > 0;
                let (first, rows) = (self.first, self.window_rows);
                out.push(self.send("window", json!({"start": first, "count": rows})));
            }
            "summary" => {
                self.total_bytes = num("bytes");
                self.bytes_partial = flag("partial");
                self.bytes_ready = true;
            }
            "prepare" if self.confirming => {
                self.confirming_token = num("token");
                if !self.refresh_pending {
                    self.confirmation = Some(Confirm { token: num("token"), all: flag("all"), count: num("count"), bytes: num("bytes"), destructive: false });
                }
            }
            "check" if !flag("valid") => out.extend(self.source_changed()),
            "cancel" => out.extend(self.refresh()),
            "restore" | "delete" => {
                let failures: Vec<&Value> = m.get("failures").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
                self.clear_selection();
                for f in &failures {
                    let uri = f.get("uri").and_then(Value::as_str).unwrap_or("").to_string();
                    if let Some(identity) = f.get("identity").and_then(Value::as_str).filter(|i| !i.is_empty()) {
                        self.identities.insert(uri.clone(), identity.to_string());
                    }
                    self.selected.insert(uri, true);
                }
                let (done, failed) = (num("done"), num("failed"));
                let mut text = format!("{} {done} of {}", if op == "restore" { "Restored" } else { "Deleted" }, done + failed);
                if failed > 0 {
                    text.push_str(&format!(" · {failed} failed"));
                }
                let s = |f: &Value, k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let detail: Vec<String> = failures
                    .iter()
                    .map(|f| format!("{} failed: {}", Some(s(f, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| s(f, "uri")), s(f, "error")))
                    .collect();
                out.push(Out::Result(text, detail.join("\n"), failed > 0));
                out.extend(self.refresh());
                self.operation_active = false;
            }
            _ => {}
        }
        if self.refresh_pending && !self.busy() {
            out.extend(self.source_changed());
        } else if !self.busy() {
            out.extend(self.ensure_window());
        }
        out
    }

    fn clear_selection(&mut self) {
        self.all_selected = false;
        self.selection_token = 0;
        self.selection_count = 0;
        self.selected.clear();
        self.identities.clear();
    }

    // A fresh listing: what the backend recovered, the selection kept only where its items are still
    // there, and the action the view was opened for.
    fn listed(&mut self, m: &Value) -> Vec<Out> {
        let mut out = vec![];
        let errors: Vec<&str> = m.get("recoveryErrors").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let recovered = m.get("recoveredCount").and_then(Value::as_u64).unwrap_or(0);
        if !errors.is_empty() {
            out.push(Out::Result("Interrupted file operation needs attention".into(), errors.join("\n"), true));
        } else if recovered > 0 {
            out.push(Out::Result(format!("Recovered {recovered} interrupted file operations"), String::new(), false));
        }
        self.error_text.clear();
        let present: Vec<String> = m.get("present").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        self.selected.retain(|uri, _| present.contains(uri));
        self.identities.retain(|uri, _| present.contains(uri));
        if self.total == 0 {
            self.all_selected = false;
        }
        match std::mem::take(&mut self.initial_action).as_str() {
            "emptyTrash" => out.extend(self.prepare(true)),
            "restoreAll" => out.extend(self.restore(true)),
            _ => out.push(self.send("summary", json!({}))),
        }
        out
    }
}

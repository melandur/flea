// The 30 day sweep, ui/TrashHost.qml and the half of ui/js/TrashDates.js it reads. GM's ruling of
// 2026-09-11: off by default and opt in, because permanent deletion is outside the undo journal and
// nobody opted into it by installing an update. It runs the same three requests the window runs, in
// the same order, so prepare still reviews every item against the identity the listing reported
// and delete still refuses a token the Trash has moved under. The backend keeps ONE Trash session,
// so the sweep and the view are never in flight together: the view reaching for the Trash stops it.
// Toolkit free.
use crate::trash::Row;
use chrono::{Local, NaiveDateTime, TimeZone};
use serde_json::{json, Value};

pub const DAYS: i64 = 30;
// One window of the listing, which is also the most the backend answers with; a bigger Trash is
// swept over as many days as it takes, which is slower and never wrong.
pub const WINDOW: usize = 350;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, PartialEq)]
pub enum Out {
    Send(Value),
    // The sweep ran to its end today: record the day, and the Trash may have changed.
    Finished(i64, bool),
}

#[derive(Debug, Default)]
pub struct Sweep {
    id: u64,
    stage: String,
}

// TrashDates.expired: an item whose date cannot be read is NEVER swept, the one direction a wrong
// answer here can be taken in; GIO's date is "2026-09-08T10:00:00" in local time.
pub fn expired(text: &str, now_ms: i64, days: i64) -> bool {
    let Ok(naive) = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") else { return false };
    let Some(at) = Local.from_local_datetime(&naive).earliest() else { return false };
    now_ms - at.timestamp_millis() >= days * DAY_MS
}

// TrashDates.dayNumber: whole days since the epoch at local midnight, so the sweep runs once a day
// rather than once per launch.
pub fn day_number(now_ms: i64) -> i64 {
    let now = Local.timestamp_millis_opt(now_ms).single().unwrap_or_else(Local::now);
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).and_then(|m| Local.from_local_datetime(&m).earliest()).unwrap_or(now);
    midnight.timestamp_millis().div_euclid(DAY_MS)
}

impl Sweep {
    pub fn sweeping(&self) -> bool {
        !self.stage.is_empty()
    }

    fn send(&mut self, op: &str, mut fields: Value) -> Out {
        self.id += 1;
        fields["c"] = json!("trashbrowse");
        fields["op"] = json!(op);
        fields["id"] = json!(self.id);
        self.stage = op.to_string();
        Out::Send(fields)
    }

    // TrashHost.sweep: refused when off, when it already ran today, and once the view was ever used.
    pub fn start(&mut self, enabled: bool, swept_on: i64, view_used: bool, now_ms: i64) -> Vec<Out> {
        if self.sweeping() || view_used || !enabled || day_number(now_ms) == swept_on {
            return vec![];
        }
        vec![self.send("list", json!({"start": 0, "count": WINDOW, "recover": false}))]
    }

    // The operator reaching for the Trash outranks a sweep nobody asked to watch; cancelling drops
    // the prepared confirmation, so the view's own list starts from a clean session.
    pub fn stop(&mut self) -> Vec<Out> {
        if !self.sweeping() {
            return vec![];
        }
        self.stage.clear();
        self.id += 1;
        vec![Out::Send(json!({"c": "trashbrowse", "op": "cancel", "id": self.id, "clearSelection": true}))]
    }

    // Every reply while a sweep is in flight is the sweep's. A failure at any stage ends it and
    // records nothing, so the next launch tries again.
    pub fn receive(&mut self, m: &Value, now_ms: i64) -> Vec<Out> {
        if !self.sweeping() {
            return vec![];
        }
        if m.get("ok").and_then(Value::as_bool) != Some(true) {
            self.stage.clear();
            return vec![];
        }
        let finished = |s: &mut Sweep, changed: bool| {
            s.stage.clear();
            vec![Out::Finished(day_number(now_ms), changed)]
        };
        match m.get("op").and_then(Value::as_str).unwrap_or("") {
            "list" => {
                let rows: Vec<Row> = m.get("rows").and_then(Value::as_array).map(|a| a.iter().map(crate::trash::row).collect()).unwrap_or_default();
                let doomed: Vec<&Row> = rows.iter().filter(|r| expired(&r.deleted, now_ms, DAYS)).collect();
                if doomed.is_empty() {
                    return finished(self, false);
                }
                let uris: Vec<&str> = doomed.iter().map(|r| r.uri.as_str()).collect();
                let identities: Vec<&str> = doomed.iter().map(|r| r.identity.as_str()).collect();
                vec![self.send("prepare", json!({"all": false, "emptyTrash": false, "uris": uris, "identities": identities}))]
            }
            // A prepare that reviewed nothing has nothing to delete, and asking anyway would spend a
            // token on an empty confirmation.
            "prepare" => match m.get("count").and_then(Value::as_u64) {
                Some(n) if n > 0 => {
                    let token = m.get("token").and_then(Value::as_u64).unwrap_or(0);
                    vec![self.send("delete", json!({"token": token}))]
                }
                _ => finished(self, false),
            },
            "delete" => finished(self, true),
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> i64 {
        Local.from_local_datetime(&NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").unwrap()).earliest().unwrap().timestamp_millis()
    }

    fn row(uri: &str, deleted: &str) -> Value {
        json!({"uri": uri, "original": "/o", "size": 1, "deleted": deleted, "identity": format!("id-{uri}")})
    }

    #[test]
    fn an_unreadable_date_is_never_expired_and_thirty_days_is_the_line() {
        // July, so no clock change sits inside the thirty days: across one, Date's arithmetic in
        // milliseconds makes it thirty days and an hour, here as in the JavaScript.
        let now = at("2026-07-31T12:00:00");
        assert!(expired("2026-07-01T12:00:00", now, DAYS));
        assert!(!expired("2026-07-01T12:00:01", now, DAYS));
        assert!(!expired("", now, DAYS));
        assert!(!expired("yesterday", now, DAYS));
        assert_eq!(day_number(at("2026-10-29T00:00:01")), day_number(at("2026-10-29T23:59:59")));
        assert_eq!(day_number(at("2026-10-30T00:00:01")) - day_number(at("2026-10-29T23:59:59")), 1);
    }

    #[test]
    fn it_runs_once_a_day_only_when_asked_and_never_beside_the_view() {
        let now = at("2026-10-29T12:00:00");
        let mut s = Sweep::default();
        assert!(s.start(false, 0, false, now).is_empty(), "off by default");
        assert!(s.start(true, day_number(now), false, now).is_empty(), "already ran today");
        assert!(s.start(true, 0, true, now).is_empty(), "the view was used");
        let out = s.start(true, 0, false, now);
        assert!(matches!(&out[..], [Out::Send(v)] if v["op"] == "list" && v["recover"] == false && v["count"] == WINDOW));
        let cancel = s.stop();
        assert!(matches!(&cancel[..], [Out::Send(v)] if v["op"] == "cancel" && v["clearSelection"] == true));
        assert!(!s.sweeping());
    }

    #[test]
    fn only_the_expired_are_prepared_and_deleted_and_a_failure_records_nothing() {
        let now = at("2026-10-29T12:00:00");
        let mut s = Sweep::default();
        s.start(true, 0, false, now);
        let rows = json!([row("trash:///old", "2026-09-01T10:00:00"), row("trash:///new", "2026-10-28T10:00:00"), row("trash:///odd", "")]);
        let out = s.receive(&json!({"ok": true, "op": "list", "rows": rows}), now);
        let Out::Send(prep) = &out[0] else { panic!() };
        assert_eq!((prep["uris"].clone(), prep["identities"].clone()), (json!(["trash:///old"]), json!(["id-trash:///old"])));
        let out = s.receive(&json!({"ok": true, "op": "prepare", "count": 1, "token": 9}), now);
        assert!(matches!(&out[..], [Out::Send(v)] if v["op"] == "delete" && v["token"] == 9));
        assert_eq!(s.receive(&json!({"ok": true, "op": "delete", "done": 1}), now), vec![Out::Finished(day_number(now), true)]);
        // Nothing expired is a finished day too; a refusal is not.
        s.start(true, 0, false, now);
        assert_eq!(s.receive(&json!({"ok": true, "op": "list", "rows": [row("trash:///new", "2026-10-28T10:00:00")]}), now), vec![Out::Finished(day_number(now), false)]);
        s.start(true, 0, false, now);
        assert!(s.receive(&json!({"ok": false, "op": "list", "error": "x"}), now).is_empty());
        assert!(!s.sweeping());
    }
}

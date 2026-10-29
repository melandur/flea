// The chooser's half that answers the backend: a listing landing, a window of rows, a failure, an
// outside change, and the picker checks src/backend/picker.rs runs, as ui/picker.qml's Backend
// handlers and onPickerResult took them.
use crate::picker;
use crate::picker::state::{Chooser, Effect, Listing, Review};
use crate::wire::{PickerReply, Request, Row};

impl Chooser {
    pub fn listed(&mut self, n: usize) {
        if self.backend_unavailable {
            return;
        }
        self.pending_listings = self.pending_listings.saturating_sub(1);
        self.receiving_latest = self.pending_listings == 0;
        if self.receiving_latest {
            self.total = n;
            self.state = if n == 0 { Listing::Empty } else { Listing::Ready };
            self.cursor = self.cursor.min(n.saturating_sub(1));
        }
    }

    pub fn rows_arrived(&mut self, start: usize, rows: Vec<Row>) {
        if self.receiving_latest {
            self.held = start;
            self.rows = rows;
        }
    }

    pub fn failed(&mut self, place: &str, msg: &str) -> Vec<Effect> {
        if place == "backend" {
            self.backend_unavailable = true;
            self.pending_listings = 0;
            self.receiving_latest = false;
            (self.mark_request, self.save_request, self.review_request) = (0, 0, 0);
            (self.accept_marks, self.marks_dirty) = (false, false);
            self.review = None;
            self.save_error = msg.to_string();
            return self.say(&format!("{msg}; cancel and reopen this request."), true);
        }
        if place == "scan" || place == "sort" {
            self.pending_listings = self.pending_listings.saturating_sub(1);
            if self.pending_listings > 0 {
                return vec![];
            }
        }
        self.state = Listing::Empty;
        self.say(msg, true)
    }

    pub fn changed(&mut self, path: &str) -> Vec<Effect> {
        if path == self.path && !self.submitting() { self.open_without_history(path) } else { vec![] }
    }

    // The listing is a window around the viewport, so scrolling past what is held refetches it.
    pub fn drifted(&self, first_visible: usize) -> Option<Effect> {
        if self.backend_unavailable || self.total == 0 || self.pending_listings > 0 {
            return None;
        }
        let held_end = self.held + self.rows.len();
        let outside = self.rows.is_empty() || (first_visible < self.held && self.held > 0)
            || (first_visible + self.visible_rows > held_end && held_end < self.total);
        outside.then(|| Effect::Send(Request::Window { start: first_visible.saturating_sub(self.window_size() / 4), count: self.window_size() }))
    }

    pub fn checked(&mut self, m: PickerReply) -> Vec<Effect> {
        if self.answered || self.backend_unavailable {
            return vec![];
        }
        if m.id == self.mark_request {
            self.mark_request = 0;
            let accepting = std::mem::take(&mut self.accept_marks);
            if !m.ok {
                return self.say(&m.error, true);
            }
            self.marks = picker::reviewed_marks(&self.marks, &m.marks);
            if m.removed > 0 {
                let text = if m.removed == 1 {
                    "1 selected item moved or changed; select it again.".to_string()
                } else {
                    format!("{} selected items moved or changed; select them again.", m.removed)
                };
                return self.say(&text, true);
            }
            if accepting && !self.marks.is_empty() {
                let uris = self.marks.iter().map(|m| m.uri.clone()).collect();
                return self.finish(picker::RESPONSE_OK, uris);
            }
            if std::mem::take(&mut self.marks_dirty) {
                return self.validate_marks(false);
            }
        } else if m.id == self.save_request {
            self.save_request = 0;
            if self.probe_key != self.save_key() {
                return self.probe_save();
            }
            if !m.ok {
                self.save_error = m.error.clone();
                return self.say(&m.error, true);
            }
            self.review = Some(Review { key: self.save_key(), review: m.review, path: m.path, collision: m.collision });
        } else if m.id == self.review_request {
            self.review_request = 0;
            if !m.ok || !self.save_ready() {
                let text = if m.error.is_empty() { "The save location changed; review it again.".to_string() } else { m.error };
                let mut out = self.say(&text, true);
                out.extend(self.invalidate_save());
                return out;
            }
            return self.finish(picker::RESPONSE_OK, vec![picker::file_uri(&m.path)]);
        }
        vec![]
    }
}

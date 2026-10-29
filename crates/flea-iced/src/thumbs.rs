// Thumbnail requests, as ui/js/Thumbs.js and ui/Pane.qml's settle timer make them; see AGENTS.md
// "Thumbnail requests in the GUI". The pane's own half: which rows to name and when, never how a
// picture is decoded, which is the shell's.
use crate::pane::{Effect, Pane};
use crate::wire::{Request, Row};

// AGENTS.md "Thumbnail requests in the GUI": the first screen's settle outlasts the compositor's
// resize, every later one is the fling debounce, and the map is capped at seven screens of rows.
pub const FIRST_SETTLE_MS: u64 = 70;
pub const SETTLE_MS: u64 = 120;
pub const THUMB_CAP: usize = 240;

// ui/js/Thumbs.js: absent is unknown, Asked is waiting, and the two answers are terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum Thumb {
    Asked,
    File(String),
    None,
}

impl Pane {
    pub(crate) fn restart_settle(&mut self) -> Effect {
        self.settle_gen += 1;
        if self.settle_ms == 0 {
            self.settle_ms = FIRST_SETTLE_MS;
        }
        Effect::Settle(self.settle_gen, self.settle_ms)
    }

    fn allowed(&self, row: Option<&Row>) -> bool {
        match row {
            Some(r) if r.t && self.thumb_mode != "off" => self.thumb_mode != "images" || r.i == "image-x-generic",
            _ => false,
        }
    }

    // Thumbs.js plan and applied: ask for visible rows not yet in the map, cancel rows still waiting
    // that left the viewport. Nothing outside the viewport is ever named (rule 1).
    pub fn settled(&mut self, generation: u64) -> Vec<Effect> {
        if generation != self.settle_gen || self.total == 0 || self.list_in_flight() {
            return vec![];
        }
        let first = self.first_visible.min(self.total - 1);
        let last = self.last_partial.max(first).min(self.total - 1);
        let ask: Vec<usize> = (first..=last)
            .filter(|i| !self.thumbs.contains_key(i) && self.allowed(self.row_at(*i)))
            .collect();
        let mut drop: Vec<usize> = self
            .thumbs
            .iter()
            .filter(|(i, t)| **t == Thumb::Asked && (**i < first || **i > last))
            .map(|(i, _)| *i)
            .collect();
        drop.sort_unstable();
        for i in &drop {
            self.thumbs.remove(i);
            self.thumb_order.retain(|r| r != i);
        }
        for i in &ask {
            self.thumbs.insert(*i, Thumb::Asked);
            self.thumb_order.push_back(*i);
        }
        let mut out = vec![];
        if !drop.is_empty() {
            out.push(Effect::Send(Request::ThumbCancel { rows: drop }));
        }
        if !ask.is_empty() {
            out.push(Effect::Send(Request::Thumb { rows: ask }));
            self.settle_ms = SETTLE_MS;
        }
        out
    }

    pub(crate) fn on_thumbed(&mut self, row: usize, file: String) -> Vec<Effect> {
        // Every thumbed for a superseded listing precedes the new listed line, so drop them here.
        if self.list_in_flight() {
            return vec![];
        }
        if !self.thumbs.contains_key(&row) {
            self.thumb_order.push_back(row);
        }
        let out = if file.is_empty() { vec![] } else { vec![Effect::DecodeThumb(file.clone())] };
        self.thumbs.insert(row, if file.is_empty() { Thumb::None } else { Thumb::File(file) });
        while self.thumb_order.len() > THUMB_CAP {
            if let Some(old) = self.thumb_order.pop_front() {
                self.thumbs.remove(&old);
            }
        }
        out
    }

    pub fn thumb_file(&self, row: usize) -> Option<&str> {
        match self.thumbs.get(&row) {
            Some(Thumb::File(f)) => Some(f),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "thumbs_tests.rs"]
mod tests;

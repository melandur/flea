// Search and sort, as ui/js/Search.js, ui/js/SearchKeys.js and ui/js/Sort.js run them over the pane.
//
// A search replaces the listing with a ranked walk of the folder the pane stands in (Ctrl+F) or that
// folder and every one below (Ctrl+Shift+F); its rows are paths relative to that folder, so every
// per-row facility keeps working on them. The walk is live: typing restarts a 120 ms timer and the
// query that settles is the one walked. Every walk's `listed` is its first line, so a counter drops
// every line that belongs to a walk a newer one superseded.
use crate::pane::{Effect, Pane};
use crate::wire::Request;

pub const LIVE_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Mode {
    #[default]
    Off,
    Typing,
    Results,
}

#[derive(Debug, Default)]
pub struct Search {
    pub mode: Mode,
    pub query: String,
    pub deep: bool,
    // The folder the walk was started from, and what Escape returns to.
    pub from: String,
    walked: String,
    pub(crate) pending: u32,
    pub running: bool,
    pub scanned: u64,
    pub ms: f64,
    pub cancelled: bool,
    live_gen: u64,
}

impl Search {
    fn key(&self) -> String {
        format!("{}{}", if self.deep { "deep:" } else { "here:" }, self.query)
    }
}

// Sort.js FIRST_DESC: a chord's first press orders names up and sizes and dates down.
fn first_desc(key: &str) -> bool {
    key != "name"
}

impl Pane {
    // Search.listed: whether the rows on screen are a walk's rather than the folder's.
    pub fn search_listed(&self) -> bool {
        self.search.mode == Mode::Results || (self.search.mode == Mode::Typing && !self.search.from.is_empty())
    }

    fn schedule(&mut self) -> Vec<Effect> {
        self.search.live_gen += 1;
        vec![Effect::SearchTimer(self.search.live_gen)]
    }

    pub fn search_start(&mut self, deep: bool) -> Vec<Effect> {
        // Focus.act "search": the search replaces the listing, so the filter over it goes first.
        self.filter_close();
        let changed = self.search.deep != deep;
        self.search.deep = deep;
        self.search.mode = Mode::Typing;
        if changed { self.schedule() } else { vec![] }
    }

    pub fn search_typed(&mut self, text: &str) -> Vec<Effect> {
        self.search.query.push_str(text);
        self.schedule()
    }

    pub fn search_backspace(&mut self) -> Vec<Effect> {
        self.search.query.pop();
        self.schedule()
    }

    pub fn search_toggle_depth(&mut self) -> Vec<Effect> {
        self.search.deep = !self.search.deep;
        self.schedule()
    }

    // Search.live: the settled query is walked, an emptied one goes back to the folder.
    pub fn search_live(&mut self, generation: u64) -> Vec<Effect> {
        if generation != self.search.live_gen || self.search.mode != Mode::Typing {
            return vec![];
        }
        if self.list_in_flight() {
            return self.schedule();
        }
        if self.search.query.is_empty() {
            if self.search_listed() {
                let back = std::mem::take(&mut self.search.from);
                self.search_reset();
                return self.open(back, None);
            }
            return vec![];
        }
        if self.search.walked != self.search.key() { self.walk() } else { vec![] }
    }

    fn walk(&mut self) -> Vec<Effect> {
        if self.search.from.is_empty() {
            self.search.from = self.path.clone();
        }
        self.search.walked = self.search.key();
        self.search.pending += 1;
        self.search.running = true;
        self.search.scanned = 0;
        self.search.cancelled = false;
        self.total = 0;
        self.held = 0;
        self.rows.clear();
        self.cursor = 0;
        self.selection.clear();
        self.thumbs.clear();
        vec![
            Effect::ScrollTo(0),
            Effect::Send(Request::Search { path: self.path.clone(), query: self.search.query.clone(), hidden: self.hidden, shallow: !self.search.deep }),
        ]
    }

    // Enter on the query line: walk now if the settled walk is not this one, and show the results.
    pub fn search_run(&mut self) -> Vec<Effect> {
        self.search.live_gen += 1;
        if self.search.query.is_empty() {
            return self.search_close();
        }
        if self.list_in_flight() {
            return self.schedule();
        }
        let mut out = vec![];
        if !self.search_listed() || self.search.walked != self.search.key() {
            out = self.walk();
        }
        self.search.mode = Mode::Results;
        out
    }

    // SearchKeys.commitFor: Right on the line waits for the walk of this exact query, because until
    // it is ranked row 0 is not the best match.
    pub fn search_commit_for_right(&mut self) -> Option<Vec<Effect>> {
        if !self.search.query.is_empty() && (self.search.running || self.search.walked != self.search.key() || !self.search_listed()) {
            return Some(vec![Effect::Message("Still searching; Right opens a result once the search is done.".into())]);
        }
        let out = self.search_run();
        if out.is_empty() { None } else { Some(out) }
    }

    pub fn search_cancel(&mut self) -> Vec<Effect> {
        if self.search.running {
            return vec![Effect::Send(Request::SearchCancel)];
        }
        self.search_close()
    }

    pub fn search_close(&mut self) -> Vec<Effect> {
        let relist = self.search_listed();
        let back = if self.search.from.is_empty() { self.path.clone() } else { self.search.from.clone() };
        self.search.live_gen += 1;
        self.search.mode = Mode::Off;
        self.search.query.clear();
        self.search_reset();
        if relist { self.open(back, None) } else { vec![] }
    }

    // Search.leave: a navigation out of the results ends the search without listing its folder again.
    pub(crate) fn search_leave(&mut self) {
        if self.search.mode != Mode::Off {
            self.search.live_gen += 1;
            self.search.mode = Mode::Off;
            self.search.query.clear();
            self.search_reset();
        }
    }

    fn search_reset(&mut self) {
        self.search.running = false;
        self.search.scanned = 0;
        self.search.from.clear();
        self.search.walked.clear();
    }

    // The walk's own listed: dropped for a superseded walk, the count reset for the newest.
    pub(crate) fn search_listed_line(&mut self, n: usize) -> bool {
        if self.search.pending == 0 {
            return false;
        }
        self.search.pending -= 1;
        if self.search.pending == 0 && self.search_listed() {
            self.total = n;
        }
        true
    }

    pub(crate) fn on_searching(&mut self, n: usize, scanned: u64, ms: f64) -> Vec<Effect> {
        if !self.search_listed() || self.search.pending > 0 {
            return vec![];
        }
        self.total = n;
        self.search.scanned = scanned;
        self.search.ms = ms;
        self.window_drifted()
    }

    // Search.ranked: the rows were ranked just before this line, so every held index is stale.
    pub(crate) fn on_searched(&mut self, n: usize, scanned: u64, ms: f64, cancelled: bool) -> Vec<Effect> {
        if !self.search_listed() || self.search.pending > 0 {
            return vec![];
        }
        self.search.running = false;
        self.search.cancelled = cancelled;
        self.search.scanned = scanned;
        self.search.ms = ms;
        self.total = n;
        self.reset_window()
    }

    // Search.reveal: the result's own folder, with the cursor on it.
    pub fn search_reveal(&mut self) -> Vec<Effect> {
        let Some(row) = self.row_at(self.cursor) else { return vec![] };
        let full = crate::pane::join(&self.path, &row.n);
        let Some((dir, leaf)) = crate::pane::parent(&full) else { return vec![] };
        self.search_leave();
        self.open(dir, Some((leaf, 0)))
    }

    // The status bar's line while results show (Search.statusLine).
    pub fn search_status(&self) -> String {
        let found = format!("{} found", crate::format::count(self.total));
        if self.search.running {
            format!("{found} · Searching, {} scanned", crate::format::count(self.search.scanned as usize))
        } else {
            format!("{found} in {:.1} s", self.search.ms / 1000.0)
        }
    }

    // Sort.chord: the same key again reverses, a new key starts in its first direction.
    pub fn sort_chord(&mut self, key: &str) -> Vec<Effect> {
        let desc = if self.order.by == key { !self.order.desc } else { first_desc(key) };
        self.resort(key, desc)
    }

    // Sort.next: the next order the backend produces, always ascending, the grid's sort button.
    pub fn sort_next(&mut self) -> Vec<Effect> {
        const ORDERS: [&str; 4] = ["name", "size", "mtime", "kind"];
        let at = ORDERS.iter().position(|k| *k == self.order.by).map_or(0, |i| i + 1);
        self.resort(ORDERS[at % ORDERS.len()], false)
    }

    // Sort.column: a header click, which starts every key ascending.
    pub fn sort_column(&mut self, key: &str) -> Vec<Effect> {
        let desc = self.order.by == key && !self.order.desc;
        self.resort(key, desc)
    }

    fn resort(&mut self, key: &str, desc: bool) -> Vec<Effect> {
        if self.order.by == key && self.order.desc == desc {
            return vec![];
        }
        // The order chosen rides every later listing this session, sessionSort in QML.
        self.order.by = key.to_string();
        self.order.desc = desc;
        let mut out = vec![Effect::Send(Request::Sort { order: self.order.clone() })];
        out.extend(self.reset_window());
        out
    }

    // After a sort or a ranking every index names another file: the selection, the thumbnails and
    // the held window go, the cursor returns to row 0, and the first window is asked for again.
    fn reset_window(&mut self) -> Vec<Effect> {
        self.selection.clear();
        self.thumbs.clear();
        self.cursor = 0;
        self.rows.clear();
        self.held = 0;
        let mut out = vec![Effect::ScrollTo(0)];
        out.extend(self.window_drifted());
        out
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

// The columns view's state and its moves, as ui/ColumnsArea.qml and ui/js/Tap.js hold them: the
// parent and the child are read with `peek`, which never touches the pane's own listing, so the
// middle column is that listing and the cursor, the selection and every per-row facility keep working
// exactly as in the list. Peeks are cached by path and dropped whenever the pane moves, because a
// stale column is worse than an empty one.
use crate::pane::{join, parent, Effect, Pane};
use crate::wire::{Request, Row};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct Columns {
    // The directory the cache belongs to; a pane that moved empties it.
    pub at: String,
    pub hidden: bool,
    pub peeked: HashMap<String, Vec<Row>>,
    // A peek that came back unreadable: zero rows, drawn as nothing rather than as empty.
    pub denied: HashSet<String>,
    asked: HashSet<String>,
}

// Nav.parentOf: "" at the root, which has no parent column to draw.
pub fn parent_path(path: &str) -> String {
    parent(path).map(|(d, _)| d).unwrap_or_default()
}

impl Columns {
    // refreshNeighbours: both neighbours on every move; one already answered or asked is not asked again.
    pub fn follow(&mut self, pane: &Pane) -> Vec<Request> {
        if self.at != pane.path || self.hidden != pane.hidden {
            *self = Columns { at: pane.path.clone(), hidden: pane.hidden, ..Default::default() };
        }
        let mut out = vec![];
        for path in [parent_path(&pane.path), child_path(pane)] {
            if !path.is_empty() && self.asked.insert(path.clone()) {
                out.push(Request::Peek { path, first: pane.window_size(), hidden: pane.hidden });
            }
        }
        out
    }

    // True when the answer was this view's; the path bar's own Tab peek is told apart by not being asked here.
    pub fn answered(&mut self, path: &str, hidden: bool, rows: Vec<Row>, failed: bool) -> bool {
        if hidden != self.hidden || !self.asked.contains(path) || self.peeked.contains_key(path) {
            return false;
        }
        if failed {
            self.denied.insert(path.to_string());
        }
        self.peeked.insert(path.to_string(), rows);
        true
    }

    pub fn rows(&self, path: &str) -> &[Row] {
        self.peeked.get(path).map(Vec::as_slice).unwrap_or(&[])
    }

    // An answer has landed and it was neither denied nor empty of rows: the empty tile waits for both.
    pub fn draws_empty(&self, path: &str) -> bool {
        self.peeked.get(path).is_some_and(Vec::is_empty) && !self.denied.contains(path)
    }
}

// The child column: the cursor row's folder, or nothing for a file.
pub fn child_path(pane: &Pane) -> String {
    match pane.row_at(pane.cursor) {
        Some(r) if r.d => join(&pane.path, &r.n),
        _ => String::new(),
    }
}

// Tap.tappedColumn, a neighbour's row: a folder reveals on one tap, a file opens on the second,
// and a right click is the pane's menu on that row once its folder is listed (menuOnNeighbour).
pub fn tapped_neighbour(base: &str, row: &Row, taps: u8) -> Option<Effect> {
    let target = join(base, &row.n);
    match (row.d, taps) {
        (true, 1) => Some(Effect::OpenDir(target)),
        (false, 2) => Some(Effect::OpenFile(target)),
        _ => None,
    }
}

impl Pane {
    // Tap.tappedMiddle: a plain tap on a folder in the middle column goes into it on the first tap
    // (the 2026-09-11 ruling); a file, a chord, and a search result keep the list's own tap.
    pub fn tap_middle(&mut self, index: usize, taps: u8, ctrl: bool, shift: bool) -> Vec<Effect> {
        let dir = self.row_at(index).is_some_and(|r| r.d);
        if !dir || ctrl || shift || self.search_listed() {
            return self.tap(index, taps, ctrl, shift);
        }
        if taps != 1 {
            return vec![];
        }
        let mut out = vec![];
        if self.renaming.as_ref().is_some_and(|r| r.index != index) {
            out.extend(self.commit_rename());
        }
        self.selection = [index].into();
        self.select_anchor = index;
        out.extend(self.place_cursor(index));
        out.extend(self.activate());
        out
    }
}

#[cfg(test)]
#[path = "columns_tests.rs"]
mod tests;

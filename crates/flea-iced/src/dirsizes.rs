// Folder sizes and child counts, ui/js/DirSizes.js and List.qml's requestDirSizes: the backend's
// `dirsize` walk is asked for the visible directory rows only, once a row, when the list settles,
// and only while folder sizes are on or the Items column is drawn; a scroll cancels whatever is
// still waiting. Keyed by row index, the weakest key there is, so a new listing empties the map,
// except a re-read of the same folder owed to an attrib-only watch burst, which cannot have moved a
// row (AGENTS.md "An attrib-only burst keeps the folder sizes"). Toolkit free.
use crate::pane::{Effect, Pane};
use crate::wire::Request;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirSize {
    pub bytes: u64,
    pub partial: bool,
    // -1 off the wire is a directory whose entries could not be read, kept apart from an empty one.
    pub entries: Option<u64>,
}

#[derive(Debug, Default)]
pub struct DirSizes {
    // None is asked and waiting; an answer is terminal for this listing.
    map: HashMap<usize, Option<DirSize>>,
    order: VecDeque<usize>,
    // A structural watch burst since the last listing: the next re-read empties the map.
    pub stale: bool,
    // Folder sizes on, or the Items column drawn, in the list view; nothing is asked otherwise.
    pub wanted: bool,
}

impl DirSizes {
    pub fn get(&self, row: usize) -> Option<DirSize> {
        self.map.get(&row).copied().flatten()
    }

    fn pending(&self) -> bool {
        self.map.values().any(Option::is_none)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

impl Pane {
    // DirSizes.plan: the visible rows the backend called directories that were never asked.
    pub(crate) fn dir_plan(&mut self, visible: &[usize]) -> Vec<Effect> {
        if !self.dirs.wanted {
            return vec![];
        }
        let ask: Vec<usize> = visible.iter().copied().filter(|i| !self.dirs.map.contains_key(i) && self.row_at(*i).is_some_and(|r| r.d)).collect();
        if ask.is_empty() {
            return vec![];
        }
        for i in &ask {
            self.dirs.map.insert(*i, None);
            self.dirs.order.push_back(*i);
        }
        vec![Effect::Send(Request::DirSize { rows: ask })]
    }

    // dirsizecancel carries no rows and always means everything queued, so only the waiting go.
    pub(crate) fn dir_cancel(&mut self) -> Vec<Effect> {
        if !self.dirs.pending() {
            return vec![];
        }
        self.dirs.map.retain(|_, v| v.is_some());
        let map = &self.dirs.map;
        self.dirs.order.retain(|i| map.contains_key(i));
        vec![Effect::Send(Request::DirSizeCancel)]
    }

    // A dirsized line for the previous listing is still in the pipe while the new one is in flight.
    pub(crate) fn on_dirsized(&mut self, row: usize, bytes: u64, partial: bool, entries: i64) {
        if self.list_in_flight() {
            return;
        }
        if !self.dirs.map.contains_key(&row) {
            self.dirs.order.push_back(row);
        }
        self.dirs.map.insert(row, Some(DirSize { bytes, partial, entries: u64::try_from(entries).ok() }));
        while self.dirs.order.len() > crate::thumbs::THUMB_CAP {
            if let Some(old) = self.dirs.order.pop_front() {
                self.dirs.map.remove(&old);
            }
        }
    }

    // Nav.open: kept only for a re-read of the same folder that no structural burst has spoiled.
    pub(crate) fn dirs_listed(&mut self, path: &str) {
        if self.dirs.stale || path != self.path {
            self.dirs.clear();
        }
        self.dirs.stale = false;
    }
}

// Row.sizeText for a folder: · until answered or while folder sizes are off, a floor marked >.
pub fn size_text(d: Option<DirSize>, folder_sizes: bool) -> String {
    match d {
        Some(d) if folder_sizes => format!("{}{}", if d.partial { ">" } else { "" }, crate::format::size(d.bytes)),
        _ => "·".into(),
    }
}

// Row.itemsText: a file has none, a folder · until counted or when its entries could not be read.
pub fn items_text(is_dir: bool, link: bool, d: Option<DirSize>) -> String {
    match (is_dir && !link, d.and_then(|d| d.entries)) {
        (false, _) => String::new(),
        (true, Some(n)) => n.to_string(),
        (true, None) => "·".into(),
    }
}

#[cfg(test)]
#[path = "dirsizes_tests.rs"]
mod tests;

// One directory view, as ui/Pane.qml, ui/PaneWire.qml and ui/js/Nav.js hold it, with no toolkit
// in it: every input is a method, every output is an Effect the shell carries out. That is what
// lets the rules below be unit tested rather than only driven through a window.
//
// Rule 1: the pane holds [held, held + rows.len()) and nothing else; only rows that window names
// are ever stat'd. Rule 2: the view is a count, and only visible rows are built. Rule 3: `list`
// answers `listed` and its first screenful as a pair, and the pair is accepted together.
use crate::ops::Ops;
pub use crate::sentence::sentence;
pub use crate::thumbs::Thumb;
use crate::thumbs::FIRST_SETTLE_MS;
use crate::wire::{Order, Reply, Request, Row};
use std::collections::{BTreeSet, HashMap, VecDeque};

pub const BUFFER: usize = 150;
pub const REFETCH_MARGIN: usize = 25;
pub const MESSAGE_MS: u64 = 4000;
// One burst is one re-read, absorbed rather than restarted; see AGENTS.md "The open directory is watched".
pub const WATCH_MS: u64 = 400;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(Request),
    ScrollTo(usize),
    OpenFile(String),
    WatchTimer,
    Message(String),
    // The same slot, drawn in the error role; Escape dismisses it first.
    Alert(String),
    SaveHidden(bool),
    FocusRename,
    // Right on a file row: the Quick Look on the cursor row.
    Preview,
    // Restart the settle timer: a Settle message carrying this generation after that many ms.
    Settle(u64, u64),
    // Decode this cache PNG for the icon slot; a fresh answer always replaces an older picture.
    DecodeThumb(String),
    // Restart the search's live timer: a SearchLive carrying this generation after 120 ms.
    SearchTimer(u64),
}


#[derive(Debug)]
pub(crate) struct InFlight {
    pub(crate) path: String,
    // Some once `listed` arrived: the count waiting for the rows it rides with.
    total: Option<usize>,
}

// Who the cursor should land on once a listing commits: a name, or failing that an old index.
#[derive(Debug, Clone)]
pub(crate) struct Anchor {
    pub(crate) name: String,
    pub(crate) fallback: usize,
    // A folder just made opens its rename field once the cursor has landed on it.
    pub(crate) rename: bool,
}

// The inline editor: which row, what it renames, the draft and the one sentence a refusal leaves.
#[derive(Debug, Clone, PartialEq)]
pub struct Rename {
    pub index: usize,
    pub source: String,
    pub draft: String,
    pub error: String,
    // The destination a sent request names, so a late reply cannot finish another rename.
    pub pending: Option<String>,
}

#[derive(Debug, Default)]
pub struct Pane {
    pub path: String,
    pub hidden: bool,
    pub order: Order,
    pub wrap_at_ends: bool,
    pub total: usize,
    pub held: usize,
    pub rows: Vec<Row>,
    pub kinds: Vec<String>,
    pub cursor: usize,
    pub first_visible: usize,
    pub visible_rows: usize,
    pub error: Option<String>,
    pub listed_once: bool,
    // ui/js/Selection.js: row indices, which is why a new listing clears them.
    pub selection: BTreeSet<usize>,
    pub(crate) select_anchor: usize,
    pub renaming: Option<Rename>,
    // The context menu is up over this pane: its rows were built from these indices (watch_busy).
    pub menu_open: bool,
    // filter.rs: the query, whether its line has the caret, and the listing rows it leaves standing.
    pub filter_query: String,
    pub filter_typing: bool,
    pub(crate) filter_shown: Option<Vec<usize>>,
    // A watched re-read narrows the same directory again, so it alone keeps the filter (Anchor.js).
    pub(crate) keep_filter: bool,
    // Nav.js history: the directories left, and those a back press left, newest last.
    pub history: Vec<String>,
    pub forward: Vec<String>,
    pub(crate) without_history: bool,
    // The status bar's sticky line, what a running transfer says while it runs.
    pub sticky: String,
    pub(crate) ops: Ops,
    pub search: crate::search::Search,
    // preview.thumbnails: "media" asks for every row carrying t, "images" for images, "off" none.
    pub thumb_mode: String,
    pub file_type_colours: bool,
    pub thumbs: HashMap<usize, Thumb>,
    pub(crate) thumb_order: VecDeque<usize>,
    // The last row with any pixel on screen, which is what a thumbnail request names.
    pub(crate) last_partial: usize,
    pub(crate) settle_gen: u64,
    pub(crate) settle_ms: u64,
    pub(crate) in_flight: Option<InFlight>,
    pub(crate) window_pending: Option<(usize, usize)>,
    pub(crate) anchor: Option<Anchor>,
    locating: Option<(String, String, Anchor)>,
    pub(crate) watch_timer: bool,
    pub(crate) watch_debt: bool,
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub(crate) fn parent(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let (dir, leaf) = trimmed.rsplit_once('/')?;
    Some((if dir.is_empty() { "/".into() } else { dir.into() }, leaf.into()))
}

impl Pane {
    pub fn new(hidden: bool, visible_rows: usize) -> Self {
        // last_partial starts at the guess too: a settle before the first viewport report must
        // still name the first screen, which a 0 here shrank to row 0 alone, measured live.
        let visible_rows = visible_rows.max(1);
        Pane { hidden, visible_rows, last_partial: visible_rows - 1, ..Default::default() }
    }

    pub fn with_view(mut self, thumb_mode: &str, file_type_colours: bool) -> Self {
        self.thumb_mode = thumb_mode.to_string();
        self.file_type_colours = file_type_colours;
        self
    }

    pub fn with_order(mut self, order: Order, wrap_at_ends: bool) -> Self {
        self.order = order;
        self.wrap_at_ends = wrap_at_ends;
        self
    }

    pub fn window_size(&self) -> usize {
        self.visible_rows + 2 * BUFFER
    }

    pub fn row_at(&self, index: usize) -> Option<&Row> {
        index.checked_sub(self.held).and_then(|o| self.rows.get(o))
    }

    pub fn list_in_flight(&self) -> bool {
        self.in_flight.is_some()
    }

    // A navigation: the cursor starts at row 0 unless an anchor says otherwise.
    pub fn open(&mut self, path: String, anchor: Option<(String, usize)>) -> Vec<Effect> {
        self.open_anchored(path, anchor.map(|(name, fallback)| Anchor { name, fallback, rename: false }))
    }

    pub(crate) fn open_anchored(&mut self, path: String, anchor: Option<Anchor>) -> Vec<Effect> {
        // The watch debt does not travel: it belonged to the directory being left.
        if path != self.path {
            self.watch_debt = false;
            // Nav.open: going somewhere pushes where it left and forgets what was ahead.
            if !self.path.is_empty() && !std::mem::take(&mut self.without_history) {
                self.history.push(self.path.clone());
                self.forward.clear();
            }
        }
        self.without_history = false;
        // A filter narrows the rows already listed, so a new listing is exactly what forgets it.
        if !std::mem::take(&mut self.keep_filter) {
            self.filter_close();
        }
        self.anchor = anchor;
        self.locating = None;
        self.window_pending = None;
        self.in_flight = Some(InFlight { path: path.clone(), total: None });
        vec![Effect::Send(Request::List { path, first: self.window_size(), hidden: self.hidden, order: self.order.clone() })]
    }

    // F5 and the watched re-read: same directory, cursor kept on the name it was on.
    pub fn reread(&mut self) -> Vec<Effect> {
        let anchor = self.row_at(self.cursor).map(|r| (r.n.clone(), self.cursor));
        self.open(self.path.clone(), anchor)
    }

    pub fn receive(&mut self, reply: Reply) -> Vec<Effect> {
        match reply {
            Reply::Listed { n } => {
                // A walk's first line, or a superseded walk's, is the search's to count.
                if self.search_listed_line(n) {
                    return vec![];
                }
                if let Some(f) = self.in_flight.as_mut() {
                    f.total = Some(n);
                }
                vec![]
            }
            Reply::Rows { start, rows, kinds } => self.on_rows(start, rows, kinds),
            Reply::Error { r#where, path, msg } => self.on_error(&r#where, &path, &msg),
            Reply::Changed { path } => self.on_changed(&path),
            Reply::Located { directory, path, index } => self.on_located(&directory, &path, index),
            Reply::Thumbed { row, file } => self.on_thumbed(row, file),
            Reply::Searching { n, scanned, ms } => self.on_searching(n, scanned, ms),
            Reply::Searched { n, scanned, ms, cancelled } => self.on_searched(n, scanned, ms, cancelled),
            Reply::Other => vec![],
            other => self.receive_op(other),
        }
    }

    fn on_rows(&mut self, start: usize, rows: Vec<Row>, kinds: Vec<String>) -> Vec<Effect> {
        let mut out = vec![];
        match self.in_flight.as_ref().and_then(|f| f.total.map(|t| (f.path.clone(), t))) {
            // The listing's own first screenful: commit the pair.
            Some((path, total)) => {
                self.in_flight = None;
                self.path = path;
                self.total = total;
                self.held = start;
                self.rows = rows;
                self.kinds = kinds;
                self.error = None;
                self.listed_once = true;
                self.cursor = 0;
                self.first_visible = 0;
                self.selection.clear();
                self.select_anchor = 0;
                self.renaming = None;
                // An index names a different file after a new listing, so the whole map goes.
                self.thumbs.clear();
                self.thumb_order.clear();
                self.settle_ms = FIRST_SETTLE_MS;
                self.refilter();
                out.push(self.restart_settle());
                out.push(Effect::ScrollTo(0));
                out.extend(self.apply_anchor());
                out.extend(self.pay_debt());
            }
            // A window reply that landed before the new `listed` belongs to the listing being left.
            // And one that landed before a walk's own `listed` belongs to the rows that walk replaced.
            None if self.in_flight.is_some() || self.search.pending > 0 => {}
            None => {
                if self.window_pending.map(|(s, _)| s) == Some(start) {
                    self.window_pending = None;
                }
                self.held = start;
                self.rows = rows;
                self.kinds = kinds;
                self.refilter();
                out.push(self.restart_settle());
                out.extend(self.request_if_drifted());
            }
        }
        out
    }

    fn apply_anchor(&mut self) -> Vec<Effect> {
        let Some(anchor) = self.anchor.take() else { return vec![] };
        if self.total == 0 {
            return vec![];
        }
        if let Some(offset) = self.rows.iter().position(|r| r.n == anchor.name) {
            return self.land(self.held + offset, anchor.rename);
        }
        // Not in the first screenful: ask the name arena rather than walking windows for it.
        let path = join(&self.path, &anchor.name);
        self.locating = Some((self.path.clone(), path.clone(), anchor));
        vec![Effect::Send(Request::Locate { path })]
    }

    fn on_located(&mut self, directory: &str, path: &str, index: i64) -> Vec<Effect> {
        let Some((dir, wanted, anchor)) = self.locating.clone() else { return vec![] };
        if self.in_flight.is_some() || directory != dir || path != wanted || dir != self.path {
            return vec![];
        }
        self.locating = None;
        if index >= 0 {
            return self.land(index as usize, anchor.rename);
        }
        // A name gone from the listing keeps the view where the user left it, clamped.
        self.place_cursor(anchor.fallback.min(self.total.saturating_sub(1)))
    }

    fn land(&mut self, index: usize, rename: bool) -> Vec<Effect> {
        let mut out = self.place_cursor(index);
        if rename {
            out.extend(self.start_rename_at(index));
        }
        out
    }

    fn on_error(&mut self, r#where: &str, path: &str, msg: &str) -> Vec<Effect> {
        if let Some(out) = self.op_error(r#where, path, msg) {
            return out;
        }
        let sentence = sentence(r#where, msg, path);
        // A failed list leaves the directory on screen in place; see docs/protocol.md "list".
        if r#where == "scan" && self.in_flight.as_ref().map(|f| f.path.as_str()) == Some(path) {
            self.in_flight = None;
            self.anchor = None;
            if !self.listed_once {
                self.path = path.to_string();
                self.error = Some(sentence.clone());
                return vec![];
            }
        }
        vec![Effect::Alert(sentence)]
    }

    // The scrollable reports which rows are on screen: `visible` fully, `last_partial` the last with
    // any pixel showing. The held window follows only on drift; every report restarts the settle.
    pub fn viewport(&mut self, first: usize, visible: usize, last_partial: usize) -> Vec<Effect> {
        self.first_visible = first;
        self.visible_rows = visible.max(1);
        self.last_partial = last_partial;
        let mut out = vec![self.restart_settle()];
        out.extend(self.request_if_drifted());
        out
    }

    pub(crate) fn window_drifted(&mut self) -> Vec<Effect> {
        self.window_pending = None;
        self.request_if_drifted()
    }

    fn request_if_drifted(&mut self) -> Vec<Effect> {
        // A filter tests the rows the pane holds and says so in its summary; moving the window under
        // it would change which rows it is testing while the operator scrolls through its answer.
        if self.total == 0 || self.in_flight.is_some() || self.filtering() {
            return vec![];
        }
        let first = self.first_visible.min(self.total - 1);
        let last = (first + self.visible_rows - 1).min(self.total - 1);
        let held_end = self.held + self.rows.len();
        let drifted = self.rows.is_empty()
            || (first < self.held + REFETCH_MARGIN && self.held > 0)
            || (held_end < last + REFETCH_MARGIN && held_end < self.total);
        if !drifted {
            return vec![];
        }
        let want = (first.saturating_sub(BUFFER), self.window_size());
        if self.window_pending == Some(want) {
            return vec![];
        }
        self.window_pending = Some(want);
        vec![Effect::Send(Request::Window { start: want.0, count: want.1 })]
    }

    // The scroll is in view positions, which are listing rows unless a filter stands.
    pub(crate) fn place_cursor(&mut self, index: usize) -> Vec<Effect> {
        if self.total == 0 {
            return vec![];
        }
        self.cursor = index.min(self.total - 1);
        let Some(view) = self.view_of(self.cursor) else { return vec![] };
        let mut out = vec![];
        if view < self.first_visible {
            out.push(Effect::ScrollTo(view));
        } else if view >= self.first_visible + self.visible_rows {
            out.push(Effect::ScrollTo(view + 1 - self.visible_rows));
        }
        out
    }

}

#[cfg(test)]
#[path = "pane_tests.rs"]
pub(crate) mod tests;

// One directory view, as ui/Pane.qml, ui/PaneWire.qml and ui/js/Nav.js hold it, with no toolkit
// in it: every input is a method, every output is an Effect the shell carries out. That is what
// lets the rules below be unit tested rather than only driven through a window.
//
// Rule 1: the pane holds [held, held + rows.len()) and nothing else; only rows that window names
// are ever stat'd. Rule 2: the view is a count, and only visible rows are built. Rule 3: `list`
// answers `listed` and its first screenful as a pair, and the pair is accepted together.
use crate::ops::Ops;
use crate::wire::{Order, Reply, Request, Row};
use std::collections::{BTreeSet, HashMap, VecDeque};

pub const BUFFER: usize = 150;
pub const REFETCH_MARGIN: usize = 25;
pub const MESSAGE_MS: u64 = 4000;
// One burst is one re-read, absorbed rather than restarted; see AGENTS.md "The open directory is watched".
pub const WATCH_MS: u64 = 400;
// AGENTS.md "Thumbnail requests in the GUI": the first screen's settle outlasts the compositor's
// resize, every later one is the fling debounce, and the map is capped at seven screens of rows.
pub const FIRST_SETTLE_MS: u64 = 70;
pub const SETTLE_MS: u64 = 120;
pub const THUMB_CAP: usize = 240;

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
    // Restart the settle timer: a Settle message carrying this generation after that many ms.
    Settle(u64, u64),
    // Decode this cache PNG for the icon slot; a fresh answer always replaces an older picture.
    DecodeThumb(String),
}

// ui/js/Thumbs.js: absent is unknown, Asked is waiting, and the two answers are terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum Thumb {
    Asked,
    File(String),
    None,
}

#[derive(Debug)]
struct InFlight {
    path: String,
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
    // The status bar's sticky line, what a running transfer says while it runs.
    pub sticky: String,
    pub(crate) ops: Ops,
    // preview.thumbnails: "media" asks for every row carrying t, "images" for images, "off" none.
    pub thumb_mode: String,
    pub file_type_colours: bool,
    pub thumbs: HashMap<usize, Thumb>,
    thumb_order: VecDeque<usize>,
    // The last row with any pixel on screen, which is what a thumbnail request names.
    last_partial: usize,
    settle_gen: u64,
    settle_ms: u64,
    in_flight: Option<InFlight>,
    window_pending: Option<(usize, usize)>,
    pub(crate) anchor: Option<Anchor>,
    locating: Option<(String, String, Anchor)>,
    watch_timer: bool,
    watch_debt: bool,
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

    // PaneWire's watchBusy: a re-read renumbers every row, so it waits while anything holds an index.
    fn watch_busy(&self) -> bool {
        self.in_flight.is_some() || !self.selection.is_empty() || self.renaming.is_some()
    }

    // onWatchBusyChanged: the debt is paid the moment the last thing holding it clears. The shell
    // calls this after every input, so no path that clears a selection has to remember to.
    pub fn pay_debt(&mut self) -> Vec<Effect> {
        if self.watch_debt && !self.watch_busy() {
            self.watch_debt = false;
            return self.reread();
        }
        vec![]
    }

    // A navigation: the cursor starts at row 0 unless an anchor says otherwise.
    pub fn open(&mut self, path: String, anchor: Option<(String, usize)>) -> Vec<Effect> {
        self.open_anchored(path, anchor.map(|(name, fallback)| Anchor { name, fallback, rename: false }))
    }

    pub(crate) fn open_anchored(&mut self, path: String, anchor: Option<Anchor>) -> Vec<Effect> {
        // The watch debt does not travel: it belonged to the directory being left.
        if path != self.path {
            self.watch_debt = false;
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
                out.push(self.restart_settle());
                out.push(Effect::ScrollTo(0));
                out.extend(self.apply_anchor());
                out.extend(self.pay_debt());
            }
            // A window reply that landed before the new `listed` belongs to the listing being left.
            None if self.in_flight.is_some() => {}
            None => {
                if self.window_pending.map(|(s, _)| s) == Some(start) {
                    self.window_pending = None;
                }
                self.held = start;
                self.rows = rows;
                self.kinds = kinds;
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

    fn on_changed(&mut self, path: &str) -> Vec<Effect> {
        let listing = self.in_flight.as_ref().map(|f| f.path.as_str());
        // A list in flight for this same directory is busy, and the debt is paid when it lands.
        if listing == Some(path) {
            self.watch_debt = true;
            return vec![];
        }
        if listing.is_some() || path != self.path {
            return vec![];
        }
        // A selection or an open editor holds row indices, so the re-read is owed rather than run.
        if self.watch_busy() {
            self.watch_debt = true;
            return vec![];
        }
        if self.watch_timer {
            return vec![];
        }
        self.watch_timer = true;
        vec![Effect::WatchTimer]
    }

    pub fn watch_fired(&mut self) -> Vec<Effect> {
        self.watch_timer = false;
        if self.watch_busy() {
            self.watch_debt = true;
            return vec![];
        }
        self.reread()
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

    fn restart_settle(&mut self) -> Effect {
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
        if generation != self.settle_gen || self.total == 0 || self.in_flight.is_some() {
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

    fn on_thumbed(&mut self, row: usize, file: String) -> Vec<Effect> {
        // Every thumbed for a superseded listing precedes the new listed line, so drop them here.
        if self.in_flight.is_some() {
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

    fn request_if_drifted(&mut self) -> Vec<Effect> {
        if self.total == 0 || self.in_flight.is_some() {
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

    pub(crate) fn place_cursor(&mut self, index: usize) -> Vec<Effect> {
        if self.total == 0 {
            return vec![];
        }
        self.cursor = index.min(self.total - 1);
        let mut out = vec![];
        if self.cursor < self.first_visible {
            out.push(Effect::ScrollTo(self.cursor));
        } else if self.cursor >= self.first_visible + self.visible_rows {
            out.push(Effect::ScrollTo(self.cursor + 1 - self.visible_rows));
        }
        out
    }

    pub fn step(&mut self, delta: isize) -> Vec<Effect> {
        if self.total == 0 {
            return vec![];
        }
        // ui/js/Focus.js step: off, a step past an end clamps; on, a step taken FROM an end comes round.
        let last = self.total as isize - 1;
        let here = self.cursor as isize;
        let next = if self.wrap_at_ends && delta.abs() == 1 && here + delta < 0 && here == 0 {
            last
        } else if self.wrap_at_ends && delta.abs() == 1 && here + delta > last && here == last {
            0
        } else {
            (here + delta).clamp(0, last)
        } as usize;
        self.place_cursor(next)
    }

    // Enter, and a double click: a directory row navigates, every other row goes to `flea --open`.
    pub fn activate(&mut self) -> Vec<Effect> {
        let Some(row) = self.row_at(self.cursor).cloned() else { return vec![] };
        let path = join(&self.path, &row.n);
        if row.d {
            self.open(path, None)
        } else {
            vec![Effect::OpenFile(path)]
        }
    }

    // Right: a directory enters; a file would open the preview, which this front end lacks yet.
    pub fn forward(&mut self) -> Vec<Effect> {
        match self.row_at(self.cursor) {
            Some(r) if r.d => self.activate(),
            Some(_) => vec![Effect::Message("Preview is not in this build yet.".into())],
            None => vec![],
        }
    }

    pub fn up(&mut self) -> Vec<Effect> {
        match parent(&self.path) {
            Some((dir, leaf)) => self.open(dir, Some((leaf, 0))),
            None => vec![],
        }
    }

    pub fn toggle_hidden(&mut self) -> Vec<Effect> {
        self.hidden = !self.hidden;
        let mut out = vec![Effect::SaveHidden(self.hidden)];
        let anchor = self.row_at(self.cursor).map(|r| (r.n.clone(), self.cursor));
        out.extend(self.open(self.path.clone(), anchor));
        out
    }
}

// ui/js/Errors.js sentence, for the `where`s this front end can produce.
pub fn sentence(r#where: &str, msg: &str, path: &str) -> String {
    let named = parent(path).map(|(_, leaf)| leaf).unwrap_or_default();
    match r#where {
        "scan" if msg.to_lowercase().contains("permission denied") => {
            if named.is_empty() { "Permission denied".into() } else { format!("Permission denied on {named}") }
        }
        "scan" => {
            if named.is_empty() {
                "That directory could not be read.".into()
            } else {
                format!("That directory could not be read: {named}")
            }
        }
        "sort" => "Sorting by that column is not available.".into(),
        "read" => "The backend stopped responding; reopen Flea and try again.".into(),
        "backend" => format!("The backend is not available: {msg}"),
        "undo" | "redo" => capitalised(msg),
        "rename" if exists(msg) => "A file with that name is already here.".into(),
        "rename" => "That file could not be renamed.".into(),
        "rename-kept" => "Copied, but the old name was only partly removed. Check it.".into(),
        "mkdir" if exists(msg) => "A folder or file with that name is already here.".into(),
        "mkdir" => "That folder could not be created.".into(),
        "duplicate" => "That file could not be duplicated.".into(),
        "trash" => "That could not be moved to Trash.".into(),
        "transfer" | "archive" | "convert" => capitalised(msg),
        _ => "That action could not be completed; try again.".into(),
    }
}

fn exists(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("file exists") || m.contains("already exists")
}

fn capitalised(msg: &str) -> String {
    let mut chars = msg.chars();
    let Some(first) = chars.next() else { return "That action could not be completed; try again.".into() };
    let out: String = first.to_uppercase().chain(chars).collect();
    if out.ends_with('.') { out } else { format!("{out}.") }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn row(n: &str, d: bool) -> Row {
        Row { n: n.into(), d, s: 0, m: 0, p: 0o100644, i: String::new(), t: false, k: 0, l: None }
    }

    pub(crate) fn rows(start: usize, count: usize) -> Vec<Row> {
        (start..start + count).map(|i| row(&format!("f{i:06}"), false)).collect()
    }

    pub(crate) fn listed(p: &mut Pane, path: &str, total: usize) -> Vec<Effect> {
        let first = p.window_size().min(total);
        let mut out = p.receive(Reply::Listed { n: total });
        out.extend(p.receive(Reply::Rows { start: 0, rows: rows(0, first), kinds: vec![] }));
        assert_eq!(p.path, path);
        out
    }

    pub(crate) fn sends(effects: &[Effect]) -> Vec<Request> {
        effects.iter().filter_map(|e| if let Effect::Send(r) = e { Some(r.clone()) } else { None }).collect()
    }

    #[test]
    fn list_asks_for_the_first_screenful_with_it() {
        let mut p = Pane::new(false, 36);
        let out = p.open("/a".into(), None);
        assert_eq!(sends(&out), vec![Request::List { path: "/a".into(), first: 36 + 2 * BUFFER, hidden: false, order: Order::default() }]);
    }

    #[test]
    fn a_window_reply_racing_the_new_listing_is_dropped() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 1000);
        p.open("/b".into(), None);
        // An old window answered before /b's listed line.
        p.receive(Reply::Rows { start: 500, rows: rows(500, 10), kinds: vec![] });
        assert_eq!(p.path, "/a");
        assert_eq!(p.held, 0);
        listed(&mut p, "/b", 5);
        assert_eq!((p.total, p.held, p.rows.len()), (5, 0, 5));
    }

    #[test]
    fn scrolling_inside_the_window_asks_nothing_and_drifting_asks_once() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 100_000);
        assert!(sends(&p.viewport(100, 10, 10)).is_empty());
        let drift = sends(&p.viewport(5000, 10, 5010));
        assert_eq!(drift, vec![Request::Window { start: 5000 - BUFFER, count: 10 + 2 * BUFFER }]);
        // The same position again while that window is out costs no second request.
        assert!(sends(&p.viewport(5000, 10, 5010)).is_empty());
        p.receive(Reply::Rows { start: 5000 - BUFFER, rows: rows(5000 - BUFFER, 310), kinds: vec![] });
        assert_eq!(p.row_at(5000).unwrap().n, "f005000");
        assert!(sends(&p.viewport(5000, 10, 5010)).is_empty());
    }

    #[test]
    fn a_failed_list_leaves_the_directory_on_screen() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 3);
        p.open("/root".into(), None);
        let out = p.receive(Reply::Error { r#where: "scan".into(), path: "/root".into(), msg: "Permission denied (os error 13)".into() });
        assert_eq!(out, vec![Effect::Alert("Permission denied on root".into())]);
        assert_eq!(p.path, "/a");
        assert!(!p.list_in_flight());
    }

    #[test]
    fn parent_lands_on_the_directory_it_left_even_past_the_first_screen() {
        let mut p = Pane::new(false, 10);
        p.open("/a/b".into(), None);
        listed(&mut p, "/a/b", 3);
        let out = p.up();
        assert_eq!(sends(&out)[0], Request::List { path: "/a".into(), first: 310, hidden: false, order: Order::default() });
        let out = listed(&mut p, "/a", 1000);
        assert_eq!(sends(&out), vec![Request::Locate { path: "/a/b".into() }]);
        let out = p.receive(Reply::Located { directory: "/a".into(), path: "/a/b".into(), index: 700 });
        assert_eq!(p.cursor, 700);
        assert_eq!(out, vec![Effect::ScrollTo(691)]);
    }

    #[test]
    fn a_watch_burst_is_one_timer_and_one_reread_on_the_name() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 20);
        p.step(3);
        assert_eq!(p.receive(Reply::Changed { path: "/a".into() }), vec![Effect::WatchTimer]);
        // Absorbed, not restarted.
        assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
        // A change for a directory we are not on says nothing.
        assert!(p.receive(Reply::Changed { path: "/elsewhere".into() }).is_empty());
        let out = p.watch_fired();
        assert_eq!(sends(&out)[0], Request::List { path: "/a".into(), first: 310, hidden: false, order: Order::default() });
        // A file created above the cursor shifts it down by one; the anchor follows the name.
        let first = Reply::Listed { n: 21 };
        p.receive(first);
        let mut shifted = vec![row("aaa-new", false)];
        shifted.extend(rows(0, 20));
        p.receive(Reply::Rows { start: 0, rows: shifted, kinds: vec![] });
        assert_eq!(p.row_at(p.cursor).unwrap().n, "f000003");
    }

    #[test]
    fn a_change_during_a_navigation_away_is_not_paid_in_the_new_directory() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 5);
        p.open("/b".into(), None);
        assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
        let out = listed(&mut p, "/b", 5);
        assert!(sends(&out).is_empty());
    }

    #[test]
    fn a_change_during_the_same_directorys_list_is_paid_when_it_lands() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 5);
        p.reread();
        assert!(p.receive(Reply::Changed { path: "/a".into() }).is_empty());
        let out = listed(&mut p, "/a", 6);
        assert_eq!(sends(&out).len(), 1);
    }

    #[test]
    fn steps_clamp_at_both_ends_and_scroll_the_cursor_into_view() {
        let mut p = Pane::new(false, 10);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 30);
        assert!(p.step(-1).is_empty());
        assert_eq!(p.cursor, 0);
        assert_eq!(p.step(12), vec![Effect::ScrollTo(3)]);
        p.step(1000);
        assert_eq!(p.cursor, 29);
    }

    #[test]
    fn wrap_at_ends_comes_round_only_from_an_end() {
        let mut p = Pane::new(false, 10).with_order(Order::default(), true);
        p.open("/a".into(), None);
        listed(&mut p, "/a", 30);
        p.step(-1);
        assert_eq!(p.cursor, 29);
        p.step(1);
        assert_eq!(p.cursor, 0);
        p.step(5);
        p.step(-10);
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn enter_navigates_a_directory_and_hands_a_file_to_the_opener() {
        let mut p = Pane::new(false, 10);
        p.open("/".into(), None);
        p.receive(Reply::Listed { n: 2 });
        p.receive(Reply::Rows { start: 0, rows: vec![row("etc", true), row("x.txt", false)], kinds: vec![] });
        assert_eq!(sends(&p.activate())[0], Request::List { path: "/etc".into(), first: 310, hidden: false, order: Order::default() });
        let mut p2 = Pane::new(false, 10);
        p2.open("/".into(), None);
        p2.receive(Reply::Listed { n: 2 });
        p2.receive(Reply::Rows { start: 0, rows: vec![row("etc", true), row("x.txt", false)], kinds: vec![] });
        p2.step(1);
        assert_eq!(p2.activate(), vec![Effect::OpenFile("/x.txt".into())]);
    }

    #[test]
    fn root_has_no_parent() {
        assert_eq!(parent("/"), None);
        assert_eq!(parent("/a"), Some(("/".into(), "a".into())));
        assert_eq!(parent("/a/b/"), Some(("/a".into(), "b".into())));
    }

    fn trow(n: &str, t: bool, icon: &str) -> Row {
        Row { t, i: icon.into(), ..row(n, false) }
    }

    fn settle(p: &mut Pane) -> Vec<Effect> {
        let generation = p.settle_gen;
        p.settled(generation)
    }

    #[test]
    fn a_settle_asks_only_visible_thumbnailable_rows_and_never_asks_twice() {
        let mut p = Pane::new(false, 3).with_view("media", false);
        p.open("/m".into(), None);
        p.receive(Reply::Listed { n: 6 });
        let rows = vec![trow("a.jpg", true, "image-x-generic"), trow("b.txt", false, "text-x-generic"),
                        trow("c.mp4", true, "video-x-generic"), trow("d.jpg", true, "image-x-generic"),
                        trow("e.jpg", true, "image-x-generic"), trow("f.jpg", true, "image-x-generic")];
        let out = p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
        assert!(out.contains(&Effect::Settle(p.settle_gen, FIRST_SETTLE_MS)));
        p.viewport(0, 3, 3);
        assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 2, 3] }]);
        // The next settle is the fling debounce, and a row already asked is not asked again.
        assert_eq!(p.restart_settle(), Effect::Settle(p.settle_gen, SETTLE_MS));
        assert!(settle(&mut p).is_empty());
    }

    #[test]
    fn a_settle_before_any_viewport_report_names_the_first_screen() {
        let mut p = Pane::new(false, 3).with_view("media", false);
        p.open("/m".into(), None);
        p.receive(Reply::Listed { n: 5 });
        let rows = (0..5).map(|i| trow(&format!("{i}.jpg"), true, "image-x-generic")).collect();
        p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
        assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 1, 2] }]);
    }

    #[test]
    fn a_stale_settle_does_nothing_and_a_row_left_waiting_is_cancelled_then_asked_again() {
        let mut p = Pane::new(false, 2).with_view("media", false);
        p.open("/m".into(), None);
        p.receive(Reply::Listed { n: 4 });
        let rows = (0..4).map(|i| trow(&format!("{i}.jpg"), true, "image-x-generic")).collect();
        p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
        p.viewport(0, 2, 1);
        let stale = p.settle_gen;
        p.viewport(0, 2, 1);
        assert!(p.settled(stale).is_empty());
        assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 1] }]);
        p.receive(Reply::Thumbed { row: 0, file: "/c/0.png".into() });
        p.viewport(2, 2, 3);
        assert_eq!(sends(&settle(&mut p)), vec![Request::ThumbCancel { rows: vec![1] }, Request::Thumb { rows: vec![2, 3] }]);
        p.viewport(0, 2, 1);
        // 0 is answered and terminal; 1 was cancelled back to absent, so it is asked again.
        let out = sends(&settle(&mut p));
        assert_eq!(out, vec![Request::ThumbCancel { rows: vec![2, 3] }, Request::Thumb { rows: vec![1] }]);
    }

    #[test]
    fn images_mode_skips_video_and_off_asks_nothing() {
        let mut p = Pane::new(false, 2).with_view("images", false);
        p.open("/m".into(), None);
        p.receive(Reply::Listed { n: 2 });
        p.receive(Reply::Rows { start: 0, rows: vec![trow("a.mp4", true, "video-x-generic"), trow("b.png", true, "image-x-generic")], kinds: vec![] });
        p.viewport(0, 2, 1);
        assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![1] }]);
        p.thumb_mode = "off".into();
        p.thumbs.clear();
        assert!(settle(&mut p).is_empty());
    }

    #[test]
    fn an_answer_decodes_and_a_late_one_for_a_superseded_listing_is_dropped() {
        let mut p = Pane::new(false, 2).with_view("media", false);
        p.open("/m".into(), None);
        listed(&mut p, "/m", 3);
        assert_eq!(p.receive(Reply::Thumbed { row: 1, file: "/c/x.png".into() }), vec![Effect::DecodeThumb("/c/x.png".into())]);
        assert_eq!(p.thumb_file(1), Some("/c/x.png"));
        p.open("/n".into(), None);
        assert!(p.receive(Reply::Thumbed { row: 2, file: "/c/y.png".into() }).is_empty());
        listed(&mut p, "/n", 3);
        assert_eq!(p.thumb_file(1), None);
    }

    #[test]
    fn the_map_is_capped_in_answer_order() {
        let mut p = Pane::new(false, 2).with_view("media", false);
        p.open("/m".into(), None);
        listed(&mut p, "/m", 400);
        for i in 0..THUMB_CAP + 5 {
            p.receive(Reply::Thumbed { row: i, file: String::new() });
        }
        assert_eq!(p.thumbs.len(), THUMB_CAP);
        assert!(!p.thumbs.contains_key(&4) && p.thumbs.contains_key(&5));
    }
}

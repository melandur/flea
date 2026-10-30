// The chooser window's state and every rule ui/picker.qml held between a key and a backend check:
// where the list stands, what is marked, the save probe and its review, and the one answer that
// leaves. Toolkit-free; the shell runs the effects.
use crate::picker::{self, Mark, Req, RECENT};
use crate::wire::{Request, Row};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Listing {
    Loading,
    Empty,
    Ready,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(Request),
    ReadRecent,
    // The reply file's whole text; the shell writes it and only then quits the backend.
    Finish(String),
    FocusCancel,
    // A transient message to clear after its 4 s, unless a newer one replaced it.
    Expire(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Review {
    pub(crate) key: (String, String),
    pub(crate) review: u64,
    pub(crate) path: String,
    pub(crate) collision: bool,
}

pub struct Chooser {
    pub req: Req,
    pub path: String,
    pub total: usize,
    pub held: usize,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub state: Listing,
    pub pending_listings: usize,
    pub(crate) receiving_latest: bool,
    pub backend_unavailable: bool,
    pub message: Option<(String, bool)>,
    pub(crate) message_gen: u64,
    pub marks: Vec<Mark>,
    pub filter_index: i64,
    pub history: Vec<String>,
    pub save_name: String,
    pub(crate) next_check: u64,
    pub mark_request: u64,
    pub(crate) accept_marks: bool,
    pub(crate) marks_dirty: bool,
    pub(crate) save_request: u64,
    pub(crate) review_request: u64,
    pub(crate) probe_key: (String, String),
    pub(crate) review: Option<Review>,
    pub save_error: String,
    pub answered: bool,
    pub show_hidden: bool,
    pub visible_rows: usize,
}

impl Chooser {
    pub fn new(req: Req, visible_rows: usize) -> Self {
        // A separator in the caller's suggestion would put a path outside this folder in the field.
        let save_name = if picker::valid_name(&req.name) { req.name.clone() } else { String::new() };
        Chooser {
            filter_index: picker::current_chip(&req),
            req,
            path: String::new(),
            total: 0,
            held: 0,
            rows: vec![],
            cursor: 0,
            state: Listing::Loading,
            pending_listings: 0,
            receiving_latest: false,
            backend_unavailable: false,
            message: None,
            message_gen: 0,
            marks: vec![],
            history: vec![],
            save_name,
            next_check: 0,
            mark_request: 0,
            accept_marks: false,
            marks_dirty: false,
            save_request: 0,
            review_request: 0,
            probe_key: (String::new(), String::new()),
            review: None,
            save_error: String::new(),
            answered: false,
            show_hidden: false,
            visible_rows,
        }
    }

    // Only an absolute path is a folder, so a caller cannot name the Recent token as its start.
    pub fn start(&mut self, home: &str) -> Vec<Effect> {
        let start = if self.req.folder.starts_with('/') { self.req.folder.clone() } else { home.to_string() };
        self.open_without_history(&start)
    }

    pub fn recent(&self) -> bool {
        self.path == RECENT
    }
    pub(crate) fn window_size(&self) -> usize {
        self.visible_rows + 60
    }
    pub(crate) fn save_key(&self) -> (String, String) {
        (self.path.clone(), self.save_name.clone())
    }
    pub fn save_ready(&self) -> bool {
        self.review.as_ref().is_some_and(|r| r.key == self.save_key())
    }
    pub fn save_collision(&self) -> bool {
        self.save_ready() && self.review.as_ref().is_some_and(|r| r.collision)
    }
    pub fn submitting(&self) -> bool {
        self.accept_marks || self.review_request > 0
    }
    pub fn can_accept(&self) -> bool {
        !self.backend_unavailable && !self.submitting() && self.mark_request == 0 && if self.req.saving() {
            self.save_ready()
        } else {
            !self.marks.is_empty() || (self.req.folder_mode() && !self.recent() && self.state != Listing::Loading)
        }
    }
    pub fn row_for(&self, index: usize) -> Option<&Row> {
        index.checked_sub(self.held).and_then(|at| self.rows.get(at))
    }
    pub fn marked(&self, row: &Row) -> bool {
        let path = picker::row_path(&self.path, &row.n);
        self.marks.iter().any(|m| m.path == path)
    }
    pub fn markable(&self, row: &Row) -> bool {
        picker::is_directory(row) == self.req.folder_mode()
    }

    pub fn say(&mut self, text: &str, error: bool) -> Vec<Effect> {
        self.message = Some((text.to_string(), error));
        self.message_gen += 1;
        if error { vec![] } else { vec![Effect::Expire(self.message_gen)] }
    }
    pub fn expire(&mut self, generation: u64) {
        if generation == self.message_gen {
            self.message = None;
        }
    }

    pub fn open(&mut self, next: &str) -> Vec<Effect> {
        if self.submitting() || next == self.path {
            return vec![];
        }
        if !self.path.is_empty() {
            self.history.push(self.path.clone());
        }
        self.open_without_history(next)
    }

    pub fn open_without_history(&mut self, next: &str) -> Vec<Effect> {
        if self.backend_unavailable {
            return vec![];
        }
        self.path = next.to_string();
        self.total = 0;
        self.held = 0;
        self.rows.clear();
        self.cursor = 0;
        self.state = Listing::Loading;
        self.receiving_latest = false;
        let mut out = self.invalidate_save();
        out.extend(self.validate_marks(false));
        if self.recent() {
            out.push(Effect::ReadRecent);
            return out;
        }
        let r = Request::PickerList { path: next.to_string(), first: self.window_size(), hidden: self.show_hidden, filter: self.filter() };
        out.push(self.listing(r));
        out
    }

    // The history arrived; it lists only while Recent is still the location standing.
    pub fn recent_read(&mut self, paths: Vec<String>) -> Vec<Effect> {
        if !self.recent() || self.backend_unavailable {
            return vec![];
        }
        let r = Request::ListPaths { paths, first: self.window_size(), filter: self.filter() };
        vec![self.listing(r)]
    }

    pub(crate) fn listing(&mut self, r: Request) -> Effect {
        self.pending_listings += 1;
        Effect::Send(r)
    }

    pub(crate) fn filter(&self) -> (Vec<String>, Vec<String>) {
        match usize::try_from(self.filter_index).ok().and_then(|i| self.req.filters.get(i)) {
            Some(f) => (f.globs.clone(), f.mimes.clone()),
            None => (vec![], vec![]),
        }
    }

    pub fn set_filter(&mut self, index: i64) -> Vec<Effect> {
        if index == self.filter_index {
            return vec![];
        }
        self.filter_index = index;
        if self.path.is_empty() { vec![] } else { self.open_without_history(&self.path.clone()) }
    }

    pub fn toggle_hidden(&mut self) -> Vec<Effect> {
        self.show_hidden = !self.show_hidden;
        if self.path.is_empty() { vec![] } else { self.open_without_history(&self.path.clone()) }
    }

    pub(crate) fn check(&mut self, make: impl FnOnce(u64) -> Request) -> (u64, Effect) {
        self.next_check += 1;
        (self.next_check, Effect::Send(make(self.next_check)))
    }

    pub(crate) fn validate_marks(&mut self, accepting: bool) -> Vec<Effect> {
        if self.backend_unavailable || (self.marks.is_empty() && self.mark_request == 0) {
            return vec![];
        }
        if self.mark_request != 0 {
            self.marks_dirty = true;
            return vec![];
        }
        self.accept_marks = accepting;
        let (id, e) = self.check(|id| Request::PickerValidate { id });
        self.mark_request = id;
        vec![e]
    }

    pub(crate) fn invalidate_save(&mut self) -> Vec<Effect> {
        self.review = None;
        self.save_error.clear();
        self.probe_save()
    }

    pub(crate) fn probe_save(&mut self) -> Vec<Effect> {
        if self.backend_unavailable || !self.req.saving() || self.save_request != 0 || self.path.is_empty() || !picker::valid_name(&self.save_name) {
            return vec![];
        }
        self.probe_key = self.save_key();
        let (folder, name) = self.save_key();
        let (id, e) = self.check(|id| Request::PickerSave { id, folder, name });
        self.save_request = id;
        vec![e]
    }

    pub fn set_save_name(&mut self, name: String) -> Vec<Effect> {
        if name == self.save_name {
            return vec![];
        }
        self.save_name = name;
        self.invalidate_save()
    }

    pub fn go_back(&mut self) -> Vec<Effect> {
        if self.submitting() {
            return vec![];
        }
        match self.history.pop() {
            Some(target) => self.open_without_history(&target),
            None => vec![],
        }
    }

    // The board's own rule: Parent is unavailable in Recent, because a history has no parent.
    pub fn go_up(&mut self) -> Vec<Effect> {
        if self.recent() {
            return vec![];
        }
        let up = picker::parent_of(&self.path);
        if up == self.path { vec![] } else { self.open(&up) }
    }

    pub fn move_cursor(&mut self, delta: i64) {
        if self.total > 0 {
            self.cursor = (self.cursor as i64 + delta).clamp(0, self.total as i64 - 1) as usize;
        }
    }

    // Space. A directory is markable only when the request asked for one, and a file only when not.
    pub fn toggle_mark(&mut self, index: usize) -> Vec<Effect> {
        if self.backend_unavailable {
            return vec![];
        }
        if self.mark_request != 0 || self.submitting() {
            return self.say("Selection is still being checked.", false);
        }
        let Some(row) = self.row_for(index) else { return vec![] };
        if picker::is_directory(row) != self.req.folder_mode() {
            return vec![];
        }
        let (path, directory, multiple) = (picker::row_path(&self.path, &row.n), self.req.folder_mode(), self.req.multiple);
        let (id, e) = self.check(|id| Request::PickerMark { id, path, directory, multiple });
        self.mark_request = id;
        vec![e]
    }

    // Enter. Whatever is marked is sent. With nothing marked, a row the caller can receive is marked
    // and sent in one press, and any other directory is walked into. A save keeps its own rule.
    pub fn activate(&mut self, index: usize) -> Vec<Effect> {
        let Some(row) = self.row_for(index) else { return vec![] };
        let (directory, markable) = (picker::is_directory(row), self.markable(row));
        if self.req.saving() || !self.marks.is_empty() || (!directory && !markable) {
            return if self.req.saving() && directory { self.enter(index) } else { self.accept(false) };
        }
        if !markable {
            return self.enter(index);
        }
        let idle = self.mark_request == 0;
        let out = self.toggle_mark(index);
        if idle && self.mark_request != 0 {
            self.accept_marks = true;
        }
        out
    }

    // Right: a directory is walked into, and nothing else moves.
    pub fn enter(&mut self, index: usize) -> Vec<Effect> {
        match self.row_for(index) {
            Some(row) if picker::is_directory(row) => {
                let next = picker::row_path(&self.path, &row.n);
                self.open(&next)
            }
            _ => vec![],
        }
    }

    pub fn accept(&mut self, reviewed: bool) -> Vec<Effect> {
        if self.backend_unavailable || self.submitting() || self.mark_request != 0 {
            return vec![];
        }
        if self.req.saving() {
            if self.save_name.is_empty() {
                return self.say("Name the file before saving it", false);
            }
            // The answer names the folder the user was shown, so a separator is refused, not rewritten.
            if !picker::valid_name(&self.save_name) {
                return self.say(picker::NAME_REFUSED, false);
            }
            if !self.save_ready() {
                return self.probe_save();
            }
            if self.save_collision() && !reviewed {
                return vec![Effect::FocusCancel];
            }
            let review = self.review.as_ref().map(|r| r.review).unwrap_or(0);
            let (id, e) = self.check(|id| Request::PickerReview { id, review });
            self.review_request = id;
            return vec![e];
        }
        // A folder request with nothing checked takes the directory the window is standing in.
        if self.marks.is_empty() && self.req.folder_mode() {
            if self.recent() {
                return self.say("Press Space to select a folder first", false);
            }
            self.accept_marks = true;
            let path = self.path.clone();
            let (id, e) = self.check(|id| Request::PickerMark { id, path, directory: true, multiple: false });
            self.mark_request = id;
            return vec![e];
        }
        if self.marks.is_empty() {
            return self.say("Press Space to select a file first", false);
        }
        self.validate_marks(true)
    }

    pub fn cancel(&mut self) -> Vec<Effect> {
        self.finish(picker::RESPONSE_CANCELLED, vec![])
    }

    // Exactly one answer leaves this window, whichever way it is asked for.
    pub(crate) fn finish(&mut self, response: u8, uris: Vec<String>) -> Vec<Effect> {
        if self.answered {
            return vec![];
        }
        let text = picker::reply(response, &uris, self.filter_index);
        self.answered = true;
        let mut out = vec![];
        if !self.backend_unavailable {
            out.push(Effect::Send(Request::PickerClose));
        }
        out.push(Effect::Finish(text));
        out
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;

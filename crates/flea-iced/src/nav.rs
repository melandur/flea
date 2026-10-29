// Moving through the listing and out of it: a step, Enter, Right, the parent, and the hidden-files
// switch, as ui/js/Focus.js step and ui/js/Nav.js openCursor do them.
use crate::pane::{join, parent, Effect, Pane};

impl Pane {
    // Filter.moveCursor: the cursor is a listing row but it moves through what is drawn.
    pub fn step(&mut self, delta: isize) -> Vec<Effect> {
        let count = self.view_total();
        if count == 0 {
            return vec![];
        }
        // ui/js/Focus.js step: off, a step past an end clamps; on, a step taken FROM an end comes round.
        let last = count as isize - 1;
        let here = self.view_of(self.cursor).unwrap_or(0) as isize;
        let next = if self.wrap_at_ends && delta.abs() == 1 && here + delta < 0 && here == 0 {
            last
        } else if self.wrap_at_ends && delta.abs() == 1 && here + delta > last && here == last {
            0
        } else {
            (here + delta).clamp(0, last)
        } as usize;
        match self.at_view(next) {
            Some(row) => self.place_cursor(row),
            None => vec![],
        }
    }

    // Nav.back: to the directory left last, keeping where this one was for forward.
    pub fn back(&mut self) -> Vec<Effect> {
        if self.list_in_flight() {
            return vec![Effect::Message("A directory is already loading.".into())];
        }
        let Some(target) = self.history.pop() else { return vec![] };
        self.forward.push(self.path.clone());
        self.search_leave();
        self.without_history = true;
        self.open(target, None)
    }

    // Nav.mouseBack: the mouse's back button follows history, or climbs when there is none.
    pub fn mouse_back(&mut self) -> Vec<Effect> {
        if self.history.is_empty() { self.up() } else { self.back() }
    }

    // Enter, and a double click: a directory row navigates, every other row goes to `flea --open`.
    pub fn activate(&mut self) -> Vec<Effect> {
        let Some(row) = self.row_at(self.cursor).cloned() else { return vec![] };
        let path = join(&self.path, &row.n);
        if row.d {
            // Nav.open: entering a folder from the results ends the search there.
            self.search_leave();
            self.open(path, None)
        } else {
            vec![Effect::OpenFile(path)]
        }
    }

    // Right: a directory enters, a file opens the preview.
    pub fn forward(&mut self) -> Vec<Effect> {
        match self.row_at(self.cursor) {
            Some(r) if r.d => self.activate(),
            Some(_) => vec![Effect::Preview],
            None => vec![],
        }
    }

    pub fn up(&mut self) -> Vec<Effect> {
        if self.search_listed() {
            return self.search_close();
        }
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

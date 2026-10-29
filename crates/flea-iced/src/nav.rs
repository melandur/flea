// Moving through the listing and out of it: a step, Enter, Right, the parent, and the hidden-files
// switch, as ui/js/Focus.js step and ui/js/Nav.js openCursor do them.
use crate::pane::{join, parent, Effect, Pane};

impl Pane {
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

// The watched re-read: AGENTS.md "The open directory is watched". The backend says the directory
// changed; the pane decides when it may re-read, because a re-read renumbers every row.
use crate::pane::{Effect, Pane};

impl Pane {
    // PaneWire's watchBusy: a re-read renumbers every row, so it waits while anything holds an index.
    pub(crate) fn watch_busy(&self) -> bool {
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

    pub(crate) fn on_changed(&mut self, path: &str) -> Vec<Effect> {
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
}

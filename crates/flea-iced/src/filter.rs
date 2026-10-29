// The filter, as ui/js/Filter.js and ui/js/Match.js hold it: it narrows the rows the pane already
// holds, with no walk and no round trip, and keeps them in the order the backend sent them, so
// directories stay ahead of files. Two index spaces meet here. A listing row is what the backend
// numbers and every write sends; a view position is where a row is drawn. With no filter the two
// are the same number, and view_of() and at_view() are the only places that convert.
use crate::pane::{Effect, Pane};

// Match.run: case-insensitive, the same test the name would paint its match run with.
pub fn matches(name: &str, query: &str) -> bool {
    !query.is_empty() && name.to_lowercase().contains(&query.to_lowercase())
}

// Filter.summary: what the filter kept out of the rows it could test, and the folder those rows
// are a window on, because none of 350 loaded rows is not none of 104,812.
pub fn summary(kept: usize, loaded: usize, total: usize) -> String {
    let head = format!("{kept} of {loaded} shown");
    if total > loaded { format!("{head} · of {} in this folder", crate::format::count(total)) } else { head }
}

pub fn no_match(total: usize) -> String {
    if total == 1 { "1 row here, and not it".into() } else { format!("{} rows here, none of them", crate::format::count(total)) }
}

impl Pane {
    // Recomputed whenever the rows or the query change; None while no query stands.
    pub(crate) fn refilter(&mut self) {
        self.filter_shown = if self.filter_query.is_empty() {
            None
        } else {
            Some(self.rows.iter().enumerate().filter(|(_, r)| matches(&r.n, &self.filter_query)).map(|(i, _)| self.held + i).collect())
        };
    }

    pub fn filtering(&self) -> bool {
        self.filter_shown.is_some()
    }

    pub fn view_total(&self) -> usize {
        self.filter_shown.as_ref().map_or(self.total, Vec::len)
    }

    pub fn view_of(&self, row: usize) -> Option<usize> {
        match &self.filter_shown {
            None => Some(row),
            Some(list) => list.iter().position(|r| *r == row),
        }
    }

    pub fn at_view(&self, view: usize) -> Option<usize> {
        match &self.filter_shown {
            None => (view < self.total).then_some(view),
            Some(list) => list.get(view).copied(),
        }
    }

    // The status bar's line while a filter is up: the query, then what it kept.
    pub fn filter_summary(&self) -> String {
        summary(self.view_total(), self.rows.len(), self.total)
    }

    // Opened from the chrome's filter button, which ChromeBar.qml draws in the grid view alone.
    pub fn filter_start(&mut self) {
        self.filter_typing = true;
    }

    pub fn filter_typed(&mut self, text: &str) -> Vec<Effect> {
        let q = format!("{}{text}", self.filter_query);
        self.filter_apply(q)
    }

    pub fn filter_backspace(&mut self) -> Vec<Effect> {
        let mut q = self.filter_query.clone();
        q.pop();
        self.filter_apply(q)
    }

    // Enter hands the keyboard back with the filter standing; an empty query has nothing to stand.
    pub fn filter_commit(&mut self) {
        if self.filter_query.is_empty() {
            self.filter_close();
        } else {
            self.filter_typing = false;
        }
    }

    // Escape, and every fresh listing. The selection was already cut to what the filter showed.
    pub fn filter_close(&mut self) {
        self.filter_query.clear();
        self.filter_typing = false;
        self.filter_shown = None;
    }

    // Filter.apply: the selection loses the rows that dropped out before anything can act on rows
    // nobody can see, and a cursor the filter hid takes the first row still standing.
    fn filter_apply(&mut self, query: String) -> Vec<Effect> {
        self.filter_query = query;
        self.refilter();
        let Some(list) = self.filter_shown.clone() else { return vec![Effect::ScrollTo(self.cursor.saturating_sub(self.visible_rows / 2))] };
        self.selection.retain(|r| list.contains(r));
        if !list.contains(&self.cursor) {
            if let Some(&first) = list.first() {
                self.cursor = first;
                return vec![Effect::ScrollTo(0)];
            }
        }
        vec![]
    }

    // Filter.between: the rows drawn between two ends, not the index range, which would sweep up
    // every row the filter hid in the gap.
    pub(crate) fn between(&self, a: usize, b: usize) -> std::collections::BTreeSet<usize> {
        let (lo, hi) = (a.min(b), a.max(b));
        match &self.filter_shown {
            None => (lo..=hi).collect(),
            Some(list) => list.iter().copied().filter(|r| (lo..=hi).contains(r)).collect(),
        }
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;

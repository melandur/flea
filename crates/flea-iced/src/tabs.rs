// Directory tabs, ui/js/Tabs.js: the pane owns one live listing and every hidden tab is a snapshot
// of where it was, so a switch is a navigation and never a second listing held in memory. Nine is
// the cap, the digits' own range. A cursor and a selection come back only across a switch that
// re-listed nothing, because an index names a row and a re-read can put another file behind it.
// Toolkit free; tabbar.rs draws the strip and the shell runs what these answer.
use crate::pane::{Anchor, Effect, Pane};
use crate::wire::{Order, Request};

pub const MAX: usize = 9;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub path: String,
    history: Vec<String>,
    forward: Vec<String>,
    cursor: usize,
    // The view the tab was in: list, columns or grid.
    view: String,
    hidden: bool,
    selected: Vec<usize>,
    order: Order,
    // Issue 94: the listing counter, so a selection only returns to its own rows.
    list_requests: u64,
}

#[derive(Debug, Default)]
pub struct Tabs {
    pub items: Vec<Snapshot>,
    pub index: usize,
}

// What a tab move asks of the shell besides the pane's own effects: a sentence, and a view to restore.
#[derive(Debug, Default, PartialEq)]
pub struct TabMove {
    pub effects: Vec<Effect>,
    pub say: Option<String>,
    pub view: Option<String>,
    // The overlay preview closes whenever the tab on screen changes.
    pub moved: bool,
}

fn say(text: &str) -> TabMove {
    TabMove { say: Some(text.to_string()), ..Default::default() }
}

// Tabs.label: Home for home, the leaf otherwise, / for the root.
pub fn label(path: &str, home: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".into();
    }
    if !home.is_empty() && path == home {
        return "Home".into();
    }
    match path.trim_end_matches('/').rsplit('/').next() {
        Some(leaf) if !leaf.is_empty() => leaf.to_string(),
        _ => path.to_string(),
    }
}

impl Pane {
    pub fn tab_count(&self) -> usize {
        self.tabs.items.len().max(1)
    }

    // The path tab i draws: the live pane's for the current tab, the snapshot's for a hidden one.
    pub fn tab_path(&self, i: usize) -> String {
        if i == self.tabs.index {
            return self.resting_path();
        }
        self.tabs.items.get(i).map(|s| s.path.clone()).unwrap_or_default()
    }

    // Where a tab snapshotted now reopens: a search sets the path to the scope it walks.
    fn resting_path(&self) -> String {
        if self.search_listed() && !self.search.from.is_empty() {
            return self.search.from.clone();
        }
        self.path.clone()
    }

    fn snapshot(&self, path: &str, view: &str) -> Snapshot {
        // A cursor and a selection read off a search's own rows name nothing in the tab's folder.
        let elsewhere = path != self.path;
        Snapshot {
            path: path.to_string(),
            history: self.history.clone(),
            forward: self.forward.clone(),
            cursor: if elsewhere { 0 } else { self.cursor },
            view: view.to_string(),
            hidden: self.hidden,
            selected: if elsewhere { vec![] } else { self.selection.iter().copied().collect() },
            order: self.order.clone(),
            list_requests: self.list_requests,
        }
    }

    // Tabs.dropOverlay: the search and the filter go; says whether a walk's results were showing.
    fn drop_overlay(&mut self) -> (Vec<Effect>, bool) {
        let dropped = self.search_listed();
        let mut out = vec![];
        if self.search.running {
            out.push(Effect::Send(Request::SearchCancel));
        }
        self.search_leave();
        self.filter_close();
        (out, dropped)
    }

    fn current_items(&self, here: &str, view: &str) -> Vec<Snapshot> {
        if self.tabs.items.is_empty() { vec![self.snapshot(here, view)] } else { self.tabs.items.clone() }
    }

    // Tabs.openNew: `target` is where it lands, the Places row's own path or Settings > View's choice.
    pub fn tab_open(&mut self, target: &str, view: &str) -> TabMove {
        if self.list_in_flight() {
            return say("A directory is already loading.");
        }
        // Before the preview and the search go: a refused tenth tab must cost the user nothing.
        if self.tab_count() >= MAX {
            return say("Nine tabs is the most.");
        }
        let here = self.resting_path();
        let (mut effects, dropped) = self.drop_overlay();
        let mut items = self.current_items(&here, view);
        items[self.tabs.index] = self.snapshot(&here, view);
        items.push(self.snapshot(target, view));
        self.tabs = Tabs { index: items.len() - 1, items };
        // A target equal to the walked scope still lists again: its rows are the walk's.
        if self.path != target || dropped {
            self.without_history = true;
            effects.extend(self.open(target.to_string(), None));
        }
        TabMove { effects, moved: true, ..Default::default() }
    }

    pub fn tab_select(&mut self, i: usize, view: &str) -> TabMove {
        if self.list_in_flight() {
            return say("A directory is already loading.");
        }
        let here = self.resting_path();
        let mut items = self.current_items(&here, view);
        if i >= items.len() {
            return say(&format!("No tab {}.", i + 1));
        }
        if i == self.tabs.index {
            return TabMove::default();
        }
        let (mut effects, dropped) = self.drop_overlay();
        items[self.tabs.index] = self.snapshot(&here, view);
        let item = items[i].clone();
        self.tabs = Tabs { items, index: i };
        let mut m = self.tab_apply(item, dropped);
        effects.append(&mut m.effects);
        m.effects = effects;
        m
    }

    pub fn tab_close(&mut self, i: usize, view: &str) -> TabMove {
        if self.list_in_flight() {
            return say("A directory is already loading.");
        }
        let mut items = self.current_items(&self.resting_path(), view);
        if items.len() <= 1 {
            return say("Can't close the last tab.");
        }
        if i >= items.len() {
            return say(&format!("No tab {}.", i + 1));
        }
        let index = self.tabs.index;
        items.remove(i);
        let next = if i < index { index - 1 } else if i == index { i.min(items.len() - 1) } else { index };
        if i != index {
            self.tabs = Tabs { items, index: next };
            return TabMove::default();
        }
        let (mut effects, dropped) = self.drop_overlay();
        let item = items[next].clone();
        self.tabs = Tabs { items, index: next };
        let mut m = self.tab_apply(item, dropped);
        effects.append(&mut m.effects);
        m.effects = effects;
        m
    }

    // Tabs.apply: the same directory re-lists nothing and the cursor and selection come straight
    // back; another one lists with the tab's own order and dotfile answer, and lands on its index.
    fn tab_apply(&mut self, item: Snapshot, dropped: bool) -> TabMove {
        let same = self.path == item.path && self.hidden == item.hidden && !dropped;
        self.history = item.history.clone();
        self.forward = item.forward.clone();
        self.hidden = item.hidden;
        let mut effects = vec![];
        if same {
            if self.order != item.order {
                effects.extend(self.resort(&item.order.by, item.order.desc));
                effects.extend(self.place_cursor(item.cursor));
            } else {
                effects.extend(self.place_cursor(item.cursor));
                self.selection.clear();
                if item.list_requests == self.list_requests {
                    self.selection.extend(item.selected.iter().copied().filter(|i| *i < self.total));
                }
            }
        } else {
            self.order = item.order.clone();
            self.without_history = true;
            // An anchor no row is named by lands on the clamped index, which is pendingCursor exactly.
            effects.extend(self.open_anchored(item.path.clone(), Some(Anchor { name: String::new(), fallback: item.cursor, rename: false })));
        }
        TabMove { effects, say: None, view: Some(item.view), moved: true }
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;

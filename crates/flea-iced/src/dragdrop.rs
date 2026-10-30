// Dragging rows onto folders, ui/js/Drag.js, inside the window: what a lift carries, which folder
// may take it, and Finder's rule for move against copy, decided in one place so the words the
// operator reads and the request that is sent cannot drift apart. dragshell.rs tracks the pointer
// and the targets; this is the toolkit-free half.
use crate::opreplies::items;
use crate::pane::{join, parent, Pane};
use std::collections::BTreeSet;

// What a drag carries, fixed at the lift, as the QML marker is.
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    pub side: u8,
    // The directory the rows were lifted from and its device (markerSource, markerDev).
    pub source: String,
    pub src_dev: u64,
    pub rows: Vec<usize>,
    // Every carried row's path, or None when the selection reaches past the rows the pane holds,
    // which only a folder of its own listing can take, by index (Drag.mimeFor).
    pub paths: Option<Vec<String>>,
    // Ctrl at the lift copies, and only at the lift (Drag.copying).
    pub copy: bool,
}

// Where a drag is over: a row of a pane, a pane's floor, or a tab of the focused pane's strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Row(u8, usize),
    Floor(u8),
    Tab(usize),
}

// Drag.carried: the whole selection when the pressed row is in it, that row alone when it is not.
pub fn carried(selection: &BTreeSet<usize>, index: usize) -> Vec<usize> {
    if selection.contains(&index) { selection.iter().copied().collect() } else { vec![index] }
}

// Drag.verbFor for this window's own drag: a drag within one volume moves, across two copies, ctrl
// at the lift copies either way, and an unknown device (0, a failed stat) copies, because moving
// where Finder would copy loses the original.
pub fn copies(ctrl: bool, src_dev: u64, dest_dev: u64) -> bool {
    ctrl || src_dev == 0 || dest_dev == 0 || src_dev != dest_dev
}

// Drag.label: the board's words on the hovered folder.
pub fn label(copy: bool) -> &'static str {
    if copy { "copy here" } else { "move here" }
}

// Drag.line with reachNote: "Move 2 items to omarchy · ctrl at lift copies".
pub fn line(n: usize, name: &str, copy: bool, can_leave: bool) -> String {
    let verb = if copy { "Copy " } else { "Move " };
    let to = if name.is_empty() { " to a folder".to_string() } else { format!(" to {name}") };
    let hint = if copy { "" } else { " · ctrl at lift copies" };
    let reach = if can_leave { "" } else { " · too wide to drag out" };
    format!("{verb}{}{to}{hint}{reach}", items(n))
}

// Drag.canDropInto: nothing into the directory the rows came from, no folder into itself or its own
// subtree, no item into the folder it already lives in, and nothing without paths to send.
pub fn can_drop_into(lift: &Lift, dest: &str) -> bool {
    if lift.source == dest {
        return false;
    }
    let Some(paths) = lift.paths.as_ref().filter(|p| !p.is_empty()) else { return false };
    paths.iter().all(|p| dest != p && !dest.starts_with(&format!("{p}/")) && parent(p).map(|(d, _)| d).as_deref() != Some(dest))
}

// Drag.canDropByIndex: only onto a folder of the listing the rows were lifted from, never one the
// drag carries itself.
pub fn can_drop_by_index(lift: &Lift, path: &str, index: usize) -> bool {
    !lift.rows.is_empty() && lift.source == path && !lift.rows.contains(&index)
}

impl Pane {
    // Pane.dropPath: while a listing is out, the directory it is listing, which a tab's hover switch
    // has already made the pane's, even before its rows arrive.
    pub fn drop_path(&self) -> String {
        self.in_flight.as_ref().map(|f| f.path.clone()).unwrap_or_else(|| self.path.clone())
    }

    // The listing's own device, or 0, unknown, while another listing is out.
    pub fn drop_dev(&self) -> u64 {
        if self.list_in_flight() { 0 } else { self.dir_dev }
    }

    // FileDrag.liftBegan with RowDrag's enabled: never while a listing is out, never off a row the
    // pane does not hold, never off the row being renamed.
    pub fn lift(&self, side: u8, index: usize, copy: bool) -> Option<Lift> {
        if self.list_in_flight() || self.row_at(index).is_none() || self.renaming.as_ref().is_some_and(|r| r.index == index) {
            return None;
        }
        let rows = carried(&self.selection, index);
        let paths = rows.iter().map(|i| self.row_at(*i).map(|r| join(&self.path, &r.n))).collect();
        Some(Lift { side, source: self.path.clone(), src_dev: self.dir_dev, rows, paths, copy })
    }
}

#[cfg(test)]
#[path = "dragdrop_tests.rs"]
mod tests;

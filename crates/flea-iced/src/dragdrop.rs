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
    // Ctrl at the lift copies, and only at the lift (Drag.copying); a drag from anywhere but this
    // window always copies, so no source deletes a file on the strength of a drop it did not deliver.
    pub copy: bool,
    // Whether this window lifted it, and the shelf's token and fixed intent when the shelf did (rule 4).
    pub own: bool,
    pub shelf: Option<String>,
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

// Drag.INSTANCE: unique to this running Flea, because a drag from another Flea carries row indices
// that mean nothing here.
pub fn instance() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)))
}

// Drag.uriFor: each component percent-encoded on its own, encodeURIComponent's unreserved set kept.
pub fn uri_for(path: &str) -> String {
    let mut out = String::from("file://");
    for (i, part) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        for b in part.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(b as char),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
    }
    out
}

// Drag.pathsFromUrls: only a file:/// URI is a path, decoded; anything else is left behind.
pub fn paths_from_uris(uris: &[String]) -> Vec<String> {
    uris.iter().filter_map(|u| u.strip_prefix("file://").filter(|p| p.starts_with('/'))).map(percent_decode).collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).and_then(|c| hex(*c)), b.get(i + 2).and_then(|c| hex(*c))) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// Drag.mimeFor: the marker naming this drag as this Flea's, and for every other application a
// uri-list and plain paths, offered only when every carried row resolved to a path.
pub fn payload(lift: &Lift) -> Vec<(String, Vec<u8>)> {
    let rows: Vec<String> = lift.rows.iter().map(usize::to_string).collect();
    let marker = format!("{}\n{}\n{}\n{}\n{}", instance(), rows.join(","), if lift.copy { "copy" } else { "move" }, lift.source, lift.src_dev);
    let mut out = vec![(crate::dnd::ROWS_MIME.to_string(), marker.into_bytes())];
    if let Some(paths) = lift.paths.as_ref().filter(|p| !p.is_empty()) {
        let uris: Vec<String> = paths.iter().map(|p| uri_for(p)).collect();
        out.push((crate::dnd::URI_MIME.to_string(), format!("{}\r\n", uris.join("\r\n")).into_bytes()));
        // GM's ruling: a terminal or a text field gets the absolute paths, one a line.
        out.push(("text/plain".to_string(), paths.join("\n").into_bytes()));
    }
    out
}

// A drag from another application or another Flea, as feedbackFor reads it: its local paths, and
// the shelf's token and intent when the shelf sent it.
pub fn foreign(uris: &[String], shelf: Option<String>) -> Lift {
    let shelf = shelf.filter(|s| !s.split('\n').next().unwrap_or("").is_empty());
    Lift { side: u8::MAX, source: String::new(), src_dev: 0, rows: vec![], paths: Some(paths_from_uris(uris)), copy: true, own: false, shelf }
}

// Drag.shelfToken and shelfCopying.
pub fn shelf_token(payload: &str) -> &str {
    payload.split('\n').next().unwrap_or("")
}

pub fn shelf_copying(payload: &str) -> bool {
    payload.split('\n').nth(1) == Some("copy")
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
        Some(Lift { side, source: self.path.clone(), src_dev: self.dir_dev, rows, paths, copy, own: true, shelf: None })
    }
}

#[cfg(test)]
#[path = "dragdrop_tests.rs"]
mod tests;

// The write operations, as ui/js/Ops.js, ui/js/Selection.js, ui/js/Tap.js and ui/PaneWire.qml do
// them: the clipboard, the one running transfer and its queue, trash, rename, new folder and undo.
// None of them confirms, because every one of them is reversible with undo.
//
// The hints name the chords that exist. ui/js/Status.js still says "z undoes" and "p pastes", which
// were the bare letters before the key cut removed them; this port does not carry that forward.
use crate::pane::{join, parent, Anchor, Effect, Pane, Rename};
use crate::wire::Request;

pub(crate) const UNDO_HINT: &str = " · Ctrl+Z undoes";
pub(crate) const PASTE_HINT: &str = " · Ctrl+V pastes";

#[derive(Debug, Clone, PartialEq)]
pub struct Clipboard {
    pub paths: Vec<String>,
    pub moving: bool,
    // A cut being pasted is spent by the line that accepts it, and kept if the paste is refused.
    pub spending: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transfer {
    pub id: u64,
    pub moving: bool,
    pub n: usize,
    pub index: usize,
    pub name: String,
    pub failure_reported: bool,
    pub cancelling: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Queued {
    pub id: u64,
    pub n: usize,
    pub moving: bool,
}

#[derive(Debug, Default)]
pub struct Ops {
    pub clipboard: Option<Clipboard>,
    pub(crate) clip_pending: Option<bool>,
    pub transfer: Option<Transfer>,
    pub queued: Vec<Queued>,
    pub(crate) trashed_first: Option<usize>,
    // The archive job this pane started: its status line, and whether it extracts.
    pub archive: Option<(String, bool)>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

impl Pane {
    // The selection when there is one, the cursor row otherwise; sorted, which BTreeSet already is.
    fn targets(&self) -> Vec<usize> {
        if !self.selection.is_empty() {
            return self.selection.iter().copied().collect();
        }
        if self.total > 0 { vec![self.cursor] } else { vec![] }
    }

    fn no_target(&self) -> Vec<Effect> {
        vec![Effect::Message("There is nothing to act on.".into())]
    }

    pub fn toggle_select(&mut self) -> Vec<Effect> {
        if self.total == 0 {
            return vec![];
        }
        if !self.selection.remove(&self.cursor) {
            self.selection.insert(self.cursor);
        }
        self.select_anchor = self.cursor;
        vec![]
    }

    // Marks.selectAll: under a filter, everything it shows and nothing it hides.
    pub fn select_all(&mut self) -> Vec<Effect> {
        self.selection = match &self.filter_shown {
            Some(list) => list.iter().copied().collect(),
            None => (0..self.total).collect(),
        };
        vec![]
    }

    // ui/js/Tap.js for the listing: Ctrl toggles, Shift extends, a plain tap selects only that row
    // and a second tap opens it. A plain tap commits an open rename first, as the QML one does.
    pub fn tap(&mut self, index: usize, taps: u8, ctrl: bool, shift: bool) -> Vec<Effect> {
        if index >= self.total {
            return vec![];
        }
        if ctrl || shift {
            if taps != 1 {
                return vec![];
            }
            let mut out = self.place_cursor(index);
            if ctrl {
                if !self.selection.remove(&index) {
                    self.selection.insert(index);
                }
                self.select_anchor = index;
            } else {
                self.selection = self.between(index, self.select_anchor);
            }
            out.extend(self.pay_debt());
            return out;
        }
        // Search.activateAction: on the results a single tap reveals the file in its own folder,
        // and the second tap of a double is ignored rather than revealing twice.
        if self.search_listed() {
            if taps != 1 {
                return vec![];
            }
            self.selection = [index].into();
            let mut out = self.place_cursor(index);
            out.extend(self.search_reveal());
            return out;
        }
        let mut out = vec![];
        if self.renaming.as_ref().is_some_and(|r| r.index != index) {
            out.extend(self.commit_rename());
        }
        self.selection = [index].into();
        self.select_anchor = index;
        out.extend(self.place_cursor(index));
        if taps == 2 {
            out.extend(self.activate());
        }
        out
    }

    pub fn copy(&mut self, moving: bool) -> Vec<Effect> {
        let rows = self.targets();
        if rows.is_empty() {
            return self.no_target();
        }
        // Absolute paths, resolved by the backend while the listing those indices name still stands.
        self.ops.clip_pending = Some(moving);
        vec![Effect::Send(Request::Paths { rows })]
    }

    pub fn paste(&mut self) -> Vec<Effect> {
        let Some(clip) = self.ops.clipboard.as_mut().filter(|c| !c.paths.is_empty()) else {
            return vec![Effect::Message("There is nothing to paste; Ctrl+C copies and Ctrl+X cuts.".into())];
        };
        if clip.moving && clip.spending {
            return vec![Effect::Message("That cut is already being pasted.".into())];
        }
        let request = Request::Transfer { moving: clip.moving, paths: clip.paths.clone(), dest: self.path.clone() };
        if clip.moving {
            clip.spending = true;
        }
        vec![Effect::Send(request)]
    }

    pub(crate) fn spend_cut(&mut self) {
        if self.ops.clipboard.as_ref().is_some_and(|c| c.spending) {
            self.ops.clipboard = None;
        }
    }

    pub fn trash(&mut self) -> Vec<Effect> {
        let rows = self.targets();
        if rows.is_empty() {
            return self.no_target();
        }
        self.ops.trashed_first = rows.first().copied();
        vec![Effect::Send(Request::Trash { rows })]
    }

    pub fn start_rename(&mut self) -> Vec<Effect> {
        if self.renaming.as_ref().is_some_and(|r| r.pending.is_some()) {
            return vec![];
        }
        self.start_rename_at(self.cursor)
    }

    pub(crate) fn start_rename_at(&mut self, index: usize) -> Vec<Effect> {
        let Some(row) = self.row_at(index) else { return vec![] };
        let source = join(&self.path, &row.n);
        self.renaming = Some(Rename { index, source, draft: row.n.clone(), error: String::new(), pending: None });
        let mut out = self.place_cursor(index);
        out.push(Effect::FocusRename);
        out
    }

    pub fn rename_input(&mut self, draft: String) {
        if let Some(r) = self.renaming.as_mut().filter(|r| r.pending.is_none()) {
            r.draft = draft;
        }
    }

    // Only success or Escape closes the editor; a refusal keeps the draft and says why inside it.
    pub fn commit_rename(&mut self) -> Vec<Effect> {
        let Some(r) = self.renaming.as_mut() else { return vec![] };
        if r.pending.is_some() {
            return vec![];
        }
        if !valid_name(&r.draft) {
            r.error = "A name cannot be empty, . or .., or contain /.".into();
            return vec![];
        }
        r.error.clear();
        let dir = parent(&r.source).map(|(d, _)| d).unwrap_or_default();
        r.pending = Some(join(&dir, &r.draft));
        vec![Effect::Send(Request::Rename { path: r.source.clone(), to: r.draft.clone() })]
    }

    pub fn cancel_rename(&mut self) -> Vec<Effect> {
        self.renaming = None;
        self.pay_debt()
    }

    pub fn new_folder(&mut self) -> Vec<Effect> {
        vec![Effect::Send(Request::Mkdir { path: self.path.clone() })]
    }

    // Ops.duplicate: the cursor row alone, never the selection.
    pub fn duplicate(&mut self) -> Vec<Effect> {
        match self.row_at(self.cursor) {
            Some(row) => vec![Effect::Send(Request::Duplicate { path: join(&self.path, &row.n) })],
            None => vec![],
        }
    }

    pub fn undo(&mut self) -> Vec<Effect> {
        vec![Effect::Send(Request::Undo)]
    }

    // The order AGENTS.md "Write operations" gives Escape, after the shell has dismissed an error:
    // the editor, the selection, then the transfer this pane started, then the newest queued one.
    pub fn escape(&mut self) -> Vec<Effect> {
        if self.renaming.is_some() {
            return self.cancel_rename();
        }
        // Focus.act "escape": a filter, typed or standing, goes before anything else it unwinds.
        if self.filter_typing || !self.filter_query.is_empty() {
            self.filter_close();
            return vec![Effect::ScrollTo(self.cursor.saturating_sub(self.visible_rows / 2))];
        }
        if !self.selection.is_empty() {
            self.selection.clear();
            return self.pay_debt();
        }
        if let Some(t) = self.ops.transfer.as_mut() {
            if !t.cancelling {
                t.cancelling = true;
                return vec![Effect::Send(Request::TransferCancel { id: t.id })];
            }
        }
        match self.ops.queued.last() {
            Some(q) => vec![Effect::Send(Request::TransferCancel { id: q.id })],
            None => vec![],
        }
    }

    // A re-read anchored on a name, which is how every write puts the cursor where it belongs.
    pub(crate) fn refresh_on(&mut self, name: String, fallback: usize, rename: bool) -> Vec<Effect> {
        if self.list_in_flight() {
            return vec![];
        }
        self.open_anchored(self.path.clone(), Some(Anchor { name, fallback, rename, mark: false }))
    }

    // Where an anchor lands: the cursor, then the rename field a made folder opens, or the mark a
    // reveal leaves, which is Nav.applyPendingSelect's selection.only.
    pub(crate) fn land(&mut self, index: usize, anchor: &Anchor) -> Vec<Effect> {
        let mut out = self.place_cursor(index);
        if anchor.mark {
            self.selection = std::iter::once(self.cursor).collect();
            self.select_anchor = self.cursor;
        }
        if anchor.rename {
            out.extend(self.start_rename_at(index));
        }
        out
    }

    pub(crate) fn refresh_here(&mut self) -> Vec<Effect> {
        let name = self.row_at(self.cursor).map(|r| r.n.clone()).unwrap_or_default();
        self.refresh_on(name, self.cursor, false)
    }
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;

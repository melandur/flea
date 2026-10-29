// The write operations, as ui/js/Ops.js, ui/js/Selection.js, ui/js/Tap.js and ui/PaneWire.qml do
// them: the clipboard, the one running transfer and its queue, trash, rename, new folder and undo.
// None of them confirms, because every one of them is reversible with undo.
//
// The hints name the chords that exist. ui/js/Status.js still says "z undoes" and "p pastes", which
// were the bare letters before the key cut removed them; this port does not carry that forward.
use crate::pane::{join, parent, sentence, Anchor, Effect, Pane, Rename};
use crate::wire::{Reply, Request};

const UNDO_HINT: &str = " · Ctrl+Z undoes";
const PASTE_HINT: &str = " · Ctrl+V pastes";

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
    clip_pending: Option<bool>,
    pub transfer: Option<Transfer>,
    pub queued: Vec<Queued>,
    trashed_first: Option<usize>,
}

pub fn items(n: usize) -> String {
    format!("{n} {}", if n == 1 { "item" } else { "items" })
}

fn leaf(path: &str) -> String {
    path.rsplit_once('/').map(|(_, l)| l.to_string()).unwrap_or_else(|| path.to_string())
}

fn progress_line(t: &Transfer) -> String {
    let head = format!("{} {} of {}", if t.moving { "Moving" } else { "Copying" }, t.index + 1, t.n);
    if t.name.is_empty() { head } else { format!("{head} · {}", t.name) }
}

pub fn transfer_done(t: &Transfer, ok: usize, failed: usize, skipped: usize, cancelled: bool) -> String {
    let verb = if t.moving { "Moved " } else { "Copied " };
    let partial = failed > 0 || skipped > 0 || cancelled;
    let mut line = format!("{verb}{}", if partial { format!("{ok} of {}", t.n) } else { items(ok) });
    if failed > 0 {
        line += &format!(" · {failed} failed");
    }
    if skipped > 0 {
        line += &format!(" · {skipped} skipped");
    }
    if cancelled {
        line += " · cancelled";
    }
    if ok > 0 {
        line += UNDO_HINT;
    }
    line
}

pub fn trashed(ok: usize, failed: usize) -> String {
    if ok == 0 {
        return if failed == 1 {
            "That item could not be moved to Trash.".into()
        } else {
            format!("{} could not be moved to Trash.", items(failed))
        };
    }
    let mut line = format!("Moved {} to Trash", items(ok));
    if failed > 0 {
        line += &format!(", {failed} failed");
    }
    line + UNDO_HINT
}

fn undone(op: &str) -> String {
    match op {
        "trash" => "Put it back from Trash.".into(),
        "mkdir" => "Removed the new folder.".into(),
        _ => format!("Undid the {op}."),
    }
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

    pub fn select_all(&mut self) -> Vec<Effect> {
        self.selection = (0..self.total).collect();
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
                let (lo, hi) = (index.min(self.select_anchor), index.max(self.select_anchor));
                self.selection = (lo..=hi).collect();
            }
            out.extend(self.pay_debt());
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

    fn spend_cut(&mut self) {
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

    pub fn undo(&mut self) -> Vec<Effect> {
        vec![Effect::Send(Request::Undo)]
    }

    // The order AGENTS.md "Write operations" gives Escape, after the shell has dismissed an error:
    // the editor, the selection, then the transfer this pane started, then the newest queued one.
    pub fn escape(&mut self) -> Vec<Effect> {
        if self.renaming.is_some() {
            return self.cancel_rename();
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
    fn refresh_on(&mut self, name: String, fallback: usize, rename: bool) -> Vec<Effect> {
        if self.list_in_flight() {
            return vec![];
        }
        self.open_anchored(self.path.clone(), Some(Anchor { name, fallback, rename }))
    }

    fn refresh_here(&mut self) -> Vec<Effect> {
        let name = self.row_at(self.cursor).map(|r| r.n.clone()).unwrap_or_default();
        self.refresh_on(name, self.cursor, false)
    }

    // A write's own error, when it belongs to an editor or a clipboard; None hands it to the bar.
    pub(crate) fn op_error(&mut self, r#where: &str, path: &str, msg: &str) -> Option<Vec<Effect>> {
        if r#where == "transfer" {
            if let Some(c) = self.ops.clipboard.as_mut() {
                c.spending = false;
            }
            return None;
        }
        if r#where == "rename" {
            let r = self.renaming.as_mut()?;
            if r.pending.is_none() || (path != r.source && Some(path) != r.pending.as_deref()) {
                return None;
            }
            r.pending = None;
            r.error = sentence(r#where, msg, path);
            return Some(vec![]);
        }
        if r#where == "rename-kept" {
            // The listing would deny the row the sentence describes, so it is read again.
            self.renaming = None;
            let mut out = vec![Effect::Alert(sentence(r#where, msg, path))];
            out.extend(self.refresh_here());
            return Some(out);
        }
        None
    }

    pub(crate) fn receive_op(&mut self, reply: Reply) -> Vec<Effect> {
        match reply {
            Reply::Paths { paths } => {
                let Some(moving) = self.ops.clip_pending.take() else { return vec![] };
                let line = format!("{} {}{PASTE_HINT}", if moving { "Cut" } else { "Copied" }, items(paths.len()));
                self.ops.clipboard = Some(Clipboard { paths, moving, spending: false });
                vec![Effect::Message(line)]
            }
            Reply::TransferStarted { id, n, moving } => {
                if !self.ops.queued.iter().any(|q| q.id == id) {
                    self.spend_cut();
                }
                self.ops.queued.retain(|q| q.id != id);
                let t = Transfer { id, moving, n, ..Default::default() };
                self.sticky = progress_line(&t);
                self.ops.transfer = Some(t);
                vec![]
            }
            Reply::TransferQueued { id, n, moving, ahead } => {
                self.spend_cut();
                self.ops.queued.push(Queued { id, n, moving });
                let verb = if moving { "Move" } else { "Copy" };
                vec![Effect::Message(format!("{verb} of {} queued · {ahead} ahead", items(n)))]
            }
            Reply::TransferProgress { id, index, name, .. } => {
                if let Some(t) = self.ops.transfer.as_mut().filter(|t| t.id == id) {
                    t.index = index;
                    t.name = name;
                    self.sticky = progress_line(t);
                }
                vec![]
            }
            Reply::TransferItem { id, index, name, ok, err } => {
                let Some(t) = self.ops.transfer.as_mut().filter(|t| t.id == id) else { return vec![] };
                let first_failure = !ok && err != "cancelled" && !t.failure_reported;
                t.failure_reported |= first_failure;
                t.index = index;
                t.name = name.clone();
                self.sticky = progress_line(t);
                if first_failure {
                    let verb = if t.moving { "Move" } else { "Copy" };
                    return vec![Effect::Alert(format!("{verb} failed: {name} · {err}"))];
                }
                vec![]
            }
            Reply::TransferDone { id, ok, failed, skipped, cancelled } => {
                if self.ops.queued.iter().any(|q| q.id == id) {
                    self.ops.queued.retain(|q| q.id != id);
                    return vec![Effect::Message("Cancelled a queued transfer.".into())];
                }
                let Some(t) = self.ops.transfer.take_if(|t| t.id == id) else { return vec![] };
                let line = transfer_done(&t, ok, failed, skipped, cancelled);
                let unreported = failed > 1 || (failed > 0 && !t.failure_reported);
                self.sticky.clear();
                let mut out = vec![if unreported { Effect::Alert(line) } else { Effect::Message(line) }];
                out.extend(self.refresh_here());
                out
            }
            Reply::Trashed { ok, failed } => {
                self.sticky.clear();
                let line = trashed(ok, failed);
                let mut out = vec![if ok == 0 { Effect::Alert(line) } else { Effect::Message(line) }];
                self.selection.clear();
                // A block leaves as a block: the cursor goes where the block started, and the row
                // that took its place is what the re-read lands on.
                let first = self.ops.trashed_first.take();
                if let Some(first) = first.filter(|_| ok > 0) {
                    self.cursor = first.min(self.total.saturating_sub(1));
                }
                out.extend(self.refresh_here());
                out
            }
            Reply::Renamed { path } => {
                if self.renaming.as_ref().and_then(|r| r.pending.as_deref()) != Some(path.as_str()) {
                    return vec![];
                }
                self.renaming = None;
                self.refresh_on(leaf(&path), self.cursor, false)
            }
            Reply::Made { path } => {
                let mut out = vec![Effect::Message(format!("Created {}{UNDO_HINT}", leaf(&path)))];
                out.extend(self.refresh_on(leaf(&path), self.cursor, true));
                out
            }
            Reply::Undone { op } => {
                self.sticky.clear();
                let mut out = vec![Effect::Message(undone(&op))];
                out.extend(self.refresh_here());
                out
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::tests::{listed, row, sends};

    fn pane(total: usize) -> Pane {
        let mut p = Pane::new(false, 10);
        p.open("/d".into(), None);
        listed(&mut p, "/d", total);
        p
    }

    #[test]
    fn copy_resolves_rows_then_paste_sends_a_copy_into_the_current_directory() {
        let mut p = pane(5);
        p.tap(1, 1, false, false);
        p.tap(3, 1, true, false);
        assert_eq!(sends(&p.copy(false)), vec![Request::Paths { rows: vec![1, 3] }]);
        let out = p.receive(Reply::Paths { paths: vec!["/d/a".into(), "/d/b".into()] });
        assert_eq!(out, vec![Effect::Message("Copied 2 items · Ctrl+V pastes".into())]);
        let out = p.paste();
        assert_eq!(sends(&out), vec![Request::Transfer { moving: false, paths: vec!["/d/a".into(), "/d/b".into()], dest: "/d".into() }]);
        // A copy stays on the clipboard so it can be pasted again.
        p.receive(Reply::TransferStarted { id: 1, n: 2, moving: false });
        assert!(p.ops.clipboard.is_some());
    }

    #[test]
    fn a_cut_is_spent_by_the_line_that_accepts_it_and_kept_on_a_refusal() {
        let mut p = pane(5);
        p.copy(true);
        p.receive(Reply::Paths { paths: vec!["/d/a".into()] });
        p.paste();
        assert_eq!(p.paste(), vec![Effect::Message("That cut is already being pasted.".into())]);
        p.receive(Reply::Error { r#where: "transfer".into(), path: "/d".into(), msg: "dest is not a directory".into() });
        assert!(!p.ops.clipboard.as_ref().unwrap().spending);
        p.paste();
        p.receive(Reply::TransferQueued { id: 4, n: 1, moving: true, ahead: 1 });
        assert!(p.ops.clipboard.is_none());
    }

    #[test]
    fn nothing_to_paste_names_the_chords_that_exist() {
        let mut p = pane(1);
        assert_eq!(p.paste(), vec![Effect::Message("There is nothing to paste; Ctrl+C copies and Ctrl+X cuts.".into())]);
    }

    #[test]
    fn a_transfer_reports_progress_one_failure_and_its_done_line() {
        let mut p = pane(3);
        p.receive(Reply::TransferStarted { id: 7, n: 2, moving: true });
        assert_eq!(p.sticky, "Moving 1 of 2");
        p.receive(Reply::TransferProgress { id: 7, index: 0, name: "a.iso".into(), bytes: 1, total: 9 });
        assert_eq!(p.sticky, "Moving 1 of 2 · a.iso");
        let out = p.receive(Reply::TransferItem { id: 7, index: 1, name: "b".into(), ok: false, err: "permission denied".into() });
        assert_eq!(out, vec![Effect::Alert("Move failed: b · permission denied".into())]);
        let out = p.receive(Reply::TransferDone { id: 7, ok: 1, failed: 1, skipped: 0, cancelled: false });
        assert_eq!(out[0], Effect::Message("Moved 1 of 2 · 1 failed · Ctrl+Z undoes".into()));
        assert!(matches!(sends(&out)[0], Request::List { .. }));
        assert!(p.sticky.is_empty() && p.ops.transfer.is_none());
    }

    #[test]
    fn escape_unwinds_the_selection_then_cancels_then_takes_the_newest_queued() {
        let mut p = pane(3);
        p.receive(Reply::TransferStarted { id: 1, n: 1, moving: false });
        p.receive(Reply::TransferQueued { id: 2, n: 1, moving: false, ahead: 1 });
        p.receive(Reply::TransferQueued { id: 3, n: 1, moving: false, ahead: 2 });
        p.toggle_select();
        assert!(p.escape().is_empty());
        assert!(p.selection.is_empty());
        assert_eq!(sends(&p.escape()), vec![Request::TransferCancel { id: 1 }]);
        assert_eq!(sends(&p.escape()), vec![Request::TransferCancel { id: 3 }]);
        let out = p.receive(Reply::TransferDone { id: 3, ok: 0, failed: 0, skipped: 1, cancelled: true });
        assert_eq!(out, vec![Effect::Message("Cancelled a queued transfer.".into())]);
    }

    #[test]
    fn trash_sends_rows_and_lands_on_the_row_that_took_the_blocks_place() {
        let mut p = pane(10);
        p.tap(4, 1, false, false);
        p.tap(6, 1, false, true);
        assert_eq!(sends(&p.trash()), vec![Request::Trash { rows: vec![4, 5, 6] }]);
        let out = p.receive(Reply::Trashed { ok: 3, failed: 0 });
        assert_eq!(out[0], Effect::Message("Moved 3 items to Trash · Ctrl+Z undoes".into()));
        p.receive(Reply::Listed { n: 7 });
        let mut after: Vec<_> = (0..4).map(|i| row(&format!("f{i:06}"), false)).collect();
        after.extend((7..10).map(|i| row(&format!("f{i:06}"), false)));
        let out = p.receive(Reply::Rows { start: 0, rows: after, kinds: vec![] });
        // The anchor named a row that is gone, so the name arena is asked and answers -1.
        assert_eq!(sends(&out), vec![Request::Locate { path: "/d/f000004".into() }]);
        p.receive(Reply::Located { directory: "/d".into(), path: "/d/f000004".into(), index: -1 });
        assert_eq!(p.row_at(p.cursor).unwrap().n, "f000007");
        assert!(p.selection.is_empty());
    }

    #[test]
    fn rename_refuses_a_bad_name_inside_the_editor_and_a_late_reply_finishes_nothing() {
        let mut p = pane(3);
        p.step(1);
        assert!(p.start_rename().contains(&Effect::FocusRename));
        p.rename_input("a/b".into());
        assert!(p.commit_rename().is_empty());
        assert_eq!(p.renaming.as_ref().unwrap().error, "A name cannot be empty, . or .., or contain /.");
        p.rename_input("new.txt".into());
        assert_eq!(sends(&p.commit_rename()), vec![Request::Rename { path: "/d/f000001".into(), to: "new.txt".into() }]);
        // A reply for some other rename is not this one.
        assert!(p.receive(Reply::Renamed { path: "/d/other".into() }).is_empty());
        let out = p.receive(Reply::Error { r#where: "rename".into(), path: "/d/f000001".into(), msg: "File exists (os error 17)".into() });
        assert!(out.is_empty());
        assert_eq!(p.renaming.as_ref().unwrap().error, "A file with that name is already here.");
        p.commit_rename();
        let out = p.receive(Reply::Renamed { path: "/d/new.txt".into() });
        assert!(p.renaming.is_none());
        assert!(matches!(sends(&out)[0], Request::List { .. }));
    }

    #[test]
    fn a_new_folder_lands_the_cursor_on_it_with_its_rename_field_open() {
        let mut p = pane(2);
        assert_eq!(sends(&p.new_folder()), vec![Request::Mkdir { path: "/d".into() }]);
        let out = p.receive(Reply::Made { path: "/d/New Folder".into() });
        assert_eq!(out[0], Effect::Message("Created New Folder · Ctrl+Z undoes".into()));
        p.receive(Reply::Listed { n: 3 });
        let out = p.receive(Reply::Rows { start: 0, rows: vec![row("New Folder", true), row("a", false), row("b", false)], kinds: vec![] });
        assert!(out.contains(&Effect::FocusRename));
        assert_eq!(p.renaming.as_ref().unwrap().source, "/d/New Folder");
    }

    #[test]
    fn a_selection_owes_the_watched_reread_and_clearing_it_pays() {
        let mut p = pane(3);
        p.toggle_select();
        assert!(p.receive(Reply::Changed { path: "/d".into() }).is_empty());
        let out = p.escape();
        assert!(matches!(sends(&out)[0], Request::List { .. }));
    }

    #[test]
    fn a_plain_double_tap_selects_and_opens() {
        let mut p = pane(3);
        let out = p.tap(2, 2, false, false);
        assert_eq!(p.selection, [2].into());
        assert_eq!(out.last(), Some(&Effect::OpenFile("/d/f000002".into())));
    }
}

// The write operations' replies, as ui/PaneWire.qml handles them, and the lines the status bar says.
use crate::ops::{Clipboard, Queued, Transfer, PASTE_HINT, UNDO_HINT};
use crate::pane::{sentence, Effect, Pane};
use crate::wire::Reply;

pub fn items(n: usize) -> String {
    format!("{n} {}", if n == 1 { "item" } else { "items" })
}

pub(crate) fn leaf(path: &str) -> String {
    path.rsplit_once('/').map(|(_, l)| l.to_string()).unwrap_or_else(|| path.to_string())
}

pub(crate) fn progress_line(t: &Transfer) -> String {
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

pub(crate) fn undone(op: &str) -> String {
    match op {
        "trash" => "Put it back from Trash.".into(),
        "mkdir" => "Removed the new folder.".into(),
        _ => format!("Undid the {op}."),
    }
}

impl Pane {
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

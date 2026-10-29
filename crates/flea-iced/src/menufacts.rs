// What the listing menu's rules read, gathered from the window in one place, and the rebuild that
// runs whenever a probe answers under an open menu: ContextMenu's refreshProviderRows, with
// Menu.refreshedCursor keeping the cursor on the action it was on rather than on an index.
use crate::menu::{self, Facts, Item, Kind};
use crate::Flea;

impl Flea {
    pub(crate) fn menu_facts(&self, kind: Kind) -> Facts {
        let cursor_row = self.pane.row_at(self.pane.cursor);
        let file = cursor_row.is_some_and(|r| !r.d);
        let s = &self.sends;
        let p = &s.providers;
        Facts {
            path: self.pane.path.clone(),
            targets: if kind == Kind::File { self.pane.selection.len().max(1) } else { 0 },
            row_mode: cursor_row.map(|r| r.p).unwrap_or(0),
            clipboard: self.pane.ops.clipboard.as_ref().is_some_and(|c| !c.paths.is_empty()),
            show_hidden: self.pane.hidden,
            archive_formats: self.jobs.formats.archive.clone(),
            can_extract: self.jobs.formats.extract,
            can_convert: self.jobs.formats.convert,
            row_is_archive: file && cursor_row.is_some_and(|r| crate::archives::is_archive(&r.n)),
            row_is_image: cursor_row.is_some_and(|r| r.i == "image-x-generic"),
            // Read when a menu opens rather than watched, as ui/Scripts.qml does.
            scripts: if kind == Kind::File { crate::scripts::list() } else { vec![] },
            providers_refreshing: s.refreshing(),
            taildrop_installed: p.taildrop_installed,
            // Taildrop and LocalSend send files, so a folder row is offered no device at all.
            taildrop_peers: if file { s.taildrop_peers.iter().map(|x| (x.id.clone(), x.label.clone())).collect() } else { vec![] },
            localsend_installed: p.localsend_installed,
            localsend_checking: s.localsend_checking,
            localsend_peers: if file { s.localsend_peers.clone() } else { vec![] },
            dropbox_installed: p.dropbox_installed,
            dropbox_ready: s.dropbox_ready(),
            row_in_dropbox: cursor_row.is_some_and(|r| crate::providers::contains(&s.dropbox_path, &crate::pane::join(&self.pane.path, &r.n))),
        }
    }

    pub(crate) fn rebuild_menu(&mut self) {
        let Some(kind) = self.menu.as_ref().map(|m| m.kind) else { return };
        let items = menu::entries(kind, &self.menu_facts(kind), &self.effective_hidden());
        let Some(m) = self.menu.as_mut() else { return };
        let action_of = |items: &[Item], i: usize| match items.get(i) {
            Some(Item::Row(e)) => Some(e.action),
            _ => None,
        };
        let (was, flyout_of) = (action_of(&m.items, m.cursor), m.flyout.then(|| action_of(&m.items, m.cursor)).flatten());
        let kept = was.and_then(|a| items.iter().position(|i| matches!(i, Item::Row(e) if e.action == a)));
        let usable = |i: usize| matches!(items.get(i), Some(Item::Row(e)) if !e.disabled);
        m.cursor = match kept.filter(|i| usable(*i)) {
            Some(i) => i,
            None => {
                let from = kept.unwrap_or(m.cursor).min(items.len().saturating_sub(1));
                (from..items.len()).chain((0..from).rev()).find(|i| usable(*i)).unwrap_or(0)
            }
        };
        // An open flyout survives only on its own row, still offering something, cursor clamped.
        let still = matches!(items.get(m.cursor), Some(Item::Row(e)) if Some(e.action) == flyout_of && !e.sub.is_empty() && !e.disabled);
        m.flyout = still;
        if let Some(Item::Row(e)) = items.get(m.cursor) {
            m.sub_cursor = m.sub_cursor.min(e.sub.len().saturating_sub(1));
        }
        m.items = items;
    }
}

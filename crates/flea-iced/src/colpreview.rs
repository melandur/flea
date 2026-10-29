// What the columns view's preview column has loaded, ui/SelectionPreview.qml: the cursor row once
// the cursor has settled, its meta asked under a token of this column's own, and the selection it
// summarises. A loaded row is kept only while the directory, the cursor, the selection and the row's
// own identity all stand; any of them moving clears it and starts the settle again, because a fact
// stated about a row that changed under it is worse than none. Toolkit free; columnpreview.rs draws it.
use crate::facts::{self, State};
use crate::kinds::is_sheet;
use crate::pane::{join, Pane};
use crate::thumbs::Thumb;
use crate::wire::{Meta, Request, Row};
use std::collections::BTreeSet;

// SelectionPreview's settle is the pane's own thumbnail settle.
pub const SETTLE_MS: u64 = crate::thumbs::SETTLE_MS;

// The row's identity, SelectionPreview.identity: name, size, mtime, mode and icon.
type Identity = (String, u64, i64, u32, String);

fn identity(row: &Row) -> Identity {
    (row.n.clone(), row.s, row.m, row.p, row.i.clone())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub dir: String,
    pub index: usize,
    identity: Identity,
    pub row: Row,
    pub path: String,
    pub kind_name: String,
    pub selection: usize,
    pub selected: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Send(Request),
    Settle(u64),
    // The frame's own reader for a text, code or tabular row: the path, and whether it is a sheet.
    Read(String, bool),
}

// What the last follow saw: the column's input, compared so only a real move restarts the settle.
type Watch = (String, usize, Option<Identity>, BTreeSet<usize>);

#[derive(Debug, Default)]
pub struct ColPreview {
    pub loaded: Option<Loaded>,
    pub meta: Option<Meta>,
    pending: u64,
    token: u64,
    settle_gen: u64,
    watched: Option<Watch>,
}

impl ColPreview {
    // The row as SelectionPreview holds it, with the row state its readers are built from.
    pub fn row_state(&self) -> Option<State> {
        let l = self.loaded.as_ref().filter(|l| !l.row.d)?;
        Some(facts::state(Some(&l.row), 1, false, "", &l.kind_name))
    }

    // The token of the meta still in flight, 0 when none is.
    pub fn pending(&self) -> u64 {
        self.pending
    }

    // Loading is the meta still in flight for a file row; a multi-selection asks for none.
    pub fn busy(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| !l.row.d) && self.meta.is_none()
    }

    pub fn clear(&mut self) {
        self.loaded = None;
        self.meta = None;
        self.pending = 0;
        // A settle still running belongs to a load that is no longer wanted.
        self.settle_gen += 1;
    }

    // canRead, onCursorIndexChanged, onRowsChanged, onPathChanged and onSelectionVersionChanged in
    // one: run after every input. `readable` is the column on screen with no listing in flight.
    pub fn follow(&mut self, pane: &Pane, readable: bool, automatic: bool) -> Vec<Out> {
        if !readable {
            if self.watched.take().is_some() || self.loaded.is_some() {
                self.clear();
            }
            return vec![];
        }
        let candidate = pane.row_at(pane.cursor).map(identity);
        let watch = (pane.path.clone(), pane.cursor, candidate.clone(), pane.selection.clone());
        if self.watched.as_ref() == Some(&watch) {
            return vec![];
        }
        // A new directory or a new selection clears whatever was loaded, as SelectionPreview does.
        let moved_away = self.watched.as_ref().is_some_and(|w| w.0 != watch.0 || w.3 != watch.3);
        self.watched = Some(watch);
        if moved_away {
            self.clear();
        }
        let same = self.loaded.as_ref().is_some_and(|l| l.dir == pane.path && l.index == pane.cursor && Some(&l.identity) == candidate.as_ref());
        if same {
            return vec![];
        }
        self.clear();
        if automatic {
            vec![Out::Settle(self.settle_gen)]
        } else {
            vec![]
        }
    }

    pub fn settled(&mut self, generation: u64, pane: &mut Pane, automatic: bool) -> Vec<Out> {
        if generation != self.settle_gen || !automatic || pane.list_in_flight() {
            return vec![];
        }
        self.load(pane)
    }

    // loadSelection: the cursor row, the selection's rows the held window carries, the meta with the
    // hints its classification names, and the row's thumbnail when the pane has not asked for it.
    pub fn load(&mut self, pane: &mut Pane) -> Vec<Out> {
        self.clear();
        let Some(row) = pane.row_at(pane.cursor).cloned() else { return vec![] };
        let index = pane.cursor;
        let kind_name = pane.kinds.get(row.k).cloned().unwrap_or_default();
        let selected: Vec<Row> = pane.selection.iter().filter_map(|i| pane.row_at(*i).cloned()).collect();
        let loaded = Loaded {
            dir: pane.path.clone(),
            index,
            identity: identity(&row),
            path: join(&pane.path, &row.n),
            kind_name: kind_name.clone(),
            selection: pane.selection.len(),
            selected,
            row: row.clone(),
        };
        let mut out = vec![];
        let multi = loaded.selection > 1;
        let path = loaded.path.clone();
        self.loaded = Some(loaded);
        if row.d || multi {
            return out;
        }
        let kind = facts::state(Some(&row), 1, false, "", &kind_name);
        let lined = matches!(kind, State::Text | State::Code);
        self.token += 1;
        self.pending = self.token;
        // A sheet is a zip, so its newlines are not lines, and it is asked for no count.
        out.push(Out::Send(Request::Meta {
            row: index,
            text: lined && !is_sheet(&row.n),
            media: matches!(kind, State::Video | State::Audio),
            archive: kind == State::Archive,
            token: self.token,
        }));
        if lined {
            out.push(Out::Read(path, is_sheet(&row.n)));
        }
        if row.t && !pane.thumbs.contains_key(&index) {
            pane.thumbs.insert(index, Thumb::Asked);
            pane.thumb_order.push_back(index);
            out.push(Out::Send(Request::Thumb { rows: vec![index] }));
        }
        out
    }

    // onMetaResult: only this column's newest token, and only while its row still stands.
    pub fn meta_answered(&mut self, meta: Meta, pane: &Pane) -> bool {
        let stands = self.loaded.as_ref().is_some_and(|l| l.dir == pane.path && pane.row_at(l.index).map(identity).as_ref() == Some(&l.identity));
        if self.pending == 0 || meta.token != self.pending || !stands {
            return false;
        }
        self.pending = 0;
        self.meta = Some(meta);
        true
    }

    // Whether the pane's thumbnail for the loaded row is refused, or none was ever coming.
    pub fn no_thumb_coming(&self, pane: &Pane) -> bool {
        self.loaded.as_ref().is_some_and(|l| !l.row.t || pane.thumbs.get(&l.index) == Some(&Thumb::None))
    }
}

#[cfg(test)]
#[path = "colpreview_tests.rs"]
mod tests;

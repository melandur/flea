// The drag, ui/FileDrag.qml, ui/RowDrag.qml and ui/DropInto.qml: band.rs lifts it off a row and
// hands it to the platform (dnd.rs), whose drop lands wherever its coordinates place it (dndshell.rs);
// with no platform drag to be had, it stays in the window, the folder rows, each pane's floor and the
// tabs reporting the pointer entering and leaving them, and band.rs drops it on the release. Either
// way this decides what the drop would do, says it in the status bar, frames the folder that would
// take it, and sends the one transfer. dragdrop.rs holds the rules.
use crate::dragdrop::{self, Lift, Target};
use crate::pane::{join, Effect, Pane};
use crate::tabbar::Msg as TabMsg;
use crate::wire::Request;
use crate::{sleep, Flea, Message};
use iced::widget::{container, MouseArea};
use iced::{Background, Border, Color, Task};
use std::time::Duration;

// TabBar.hoverSwitchMs: a drag resting on a tab this long selects it, so the drop can land inside.
const HOVER_SWITCH_MS: u64 = 400;

#[derive(Debug)]
pub(crate) struct Drag {
    pub lift: Lift,
    pub(crate) row: Option<(u8, usize)>,
    pub(crate) floor: Option<u8>,
    pub(crate) tab: Option<usize>,
    tab_gen: u64,
    // The platform carries it, so its targets are found by position and its drop is the platform's.
    pub(crate) platform: bool,
}

impl Drag {
    pub(crate) fn new(lift: Lift, platform: bool) -> Drag {
        Drag { lift, row: None, floor: None, tab: None, tab_gen: 0, platform }
    }
}

// What a drop would do here: where the transfer goes, what the bar names, and move or copy.
pub(crate) struct Landing {
    pub side: u8,
    pub request: Request,
    pub row: Option<(u8, usize)>,
    pub name: String,
    pub copy: bool,
}

impl Flea {
    pub(crate) fn pane_on(&self, side: u8) -> Option<&Pane> {
        if side == self.side {
            return Some(&self.pane);
        }
        self.other.as_ref().filter(|o| o.side == side).map(|o| &o.pane)
    }

    pub(crate) fn drag_lift(&mut self, side: u8, view: usize) -> Task<Message> {
        self.pending_collapse = None;
        let copy = self.modifiers.control();
        let Some(pane) = self.pane_on(side) else { return Task::none() };
        let Some(index) = pane.at_view(view) else { return Task::none() };
        let Some(lift) = pane.lift(side, index, copy) else { return Task::none() };
        // Drag.active: the platform takes the drag from here when it can, every payload already fixed.
        let platform = self.dnd.as_ref().is_some_and(|d| d.drag(dragdrop::payload(&lift)));
        // FLEA_ICED_TRACE: which of the two drags this lift became.
        if std::env::var_os("FLEA_ICED_TRACE").is_some_and(|v| !v.is_empty()) {
            eprintln!("flea-iced: drag of {} rows, platform={platform}", lift.rows.len());
        }
        // The pointer starts over the row it lifted and over that pane's floor; leaving either says so.
        self.drag = Some(Drag { row: Some((side, index)), floor: Some(side), ..Drag::new(lift, platform) });
        Task::none()
    }

    // A drag resting on a tab arms the switch, as DropInto's accepted enter does: when the drag carries
    // paths, or when the tab's folder could take it as it stands.
    pub(crate) fn drag_hover(&mut self, target: Target, inside: bool) -> Task<Message> {
        let Some(drag) = self.drag.as_mut() else { return Task::none() };
        match (target, inside) {
            (Target::Row(s, i), true) => drag.row = Some((s, i)),
            (Target::Row(s, i), false) if drag.row == Some((s, i)) => drag.row = None,
            (Target::Floor(s), true) => drag.floor = Some(s),
            (Target::Floor(s), false) if drag.floor == Some(s) => drag.floor = None,
            (Target::Tab(i), true) => {
                drag.tab = Some(i);
                drag.tab_gen += 1;
                let stamp = drag.tab_gen;
                let arms = drag.lift.paths.is_some() || dragdrop::can_drop_into(&drag.lift, &self.pane.tab_path(i));
                if arms {
                    return Task::perform(sleep(Duration::from_millis(HOVER_SWITCH_MS)), move |_| Message::DragTab(stamp));
                }
            }
            (Target::Tab(i), false) if drag.tab == Some(i) => {
                drag.tab = None;
                drag.tab_gen += 1;
            }
            _ => {}
        }
        Task::none()
    }

    pub(crate) fn drag_tab_switch(&mut self, stamp: u64) -> Task<Message> {
        match self.drag.as_ref().and_then(|d| d.tab.filter(|_| d.tab_gen == stamp)) {
            Some(i) if i != self.pane.tabs.index => self.tab_message(TabMsg::Select(i)),
            _ => Task::none(),
        }
    }

    // RowDrag over DropInto: a tab answers for itself; a folder row takes the drop when it can, and
    // what a row refuses falls through to the floor under it.
    pub(crate) fn landing(&self) -> Option<Landing> {
        let drag = self.drag.as_ref()?;
        let lift = &drag.lift;
        let by_path = |side: u8, row: Option<(u8, usize)>, dest: String, dev: u64, name: String| {
            dragdrop::can_drop_into(lift, &dest).then(|| match lift.shelf.as_deref() {
                // Rule 4: the shelf fixed its verb at the lift, and its token is the authority.
                Some(shelf) => {
                    let request = Request::TransferShelf { token: dragdrop::shelf_token(shelf).to_string(), dest };
                    Landing { side, request, row, name, copy: dragdrop::shelf_copying(shelf) }
                }
                None => {
                    let copy = dragdrop::copies(lift.copy, lift.src_dev, dev);
                    let request = Request::Transfer { moving: !copy, paths: lift.paths.clone().unwrap_or_default(), dest };
                    Landing { side, request, row, name, copy }
                }
            })
        };
        if let Some(i) = drag.tab {
            let dest = if i == self.pane.tabs.index { self.pane.drop_path() } else { self.pane.tab_path(i) };
            return by_path(self.side, None, dest.clone(), self.pane.tab_dev(i), dest);
        }
        if let Some((side, i)) = drag.row {
            let pane = self.pane_on(side)?;
            if let Some(row) = pane.row_at(i).filter(|r| r.d) {
                let dest = join(&pane.path, &row.n);
                if lift.paths.is_some() {
                    if let Some(l) = by_path(side, Some((side, i)), dest, row.v, row.n.clone()) {
                        return Some(l);
                    }
                } else if side == lift.side && dragdrop::can_drop_by_index(lift, &pane.path, i) {
                    let copy = dragdrop::copies(lift.copy, lift.src_dev, row.v);
                    let request = Request::TransferRows { moving: !copy, rows: lift.rows.clone(), dest };
                    return Some(Landing { side, request, row: Some((side, i)), name: row.n.clone(), copy });
                }
            }
        }
        // PaneWire's floor: the list, the grid and the columns' own listing, never a search or the Trash.
        let side = drag.floor?;
        let pane = self.pane_on(side)?;
        let trash = side == self.side && self.trash.opened;
        if trash || pane.search_listed() {
            return None;
        }
        by_path(side, None, pane.drop_path(), pane.drop_dev(), pane.drop_path())
    }

    // FileDrag.say: the drag's line, unless a transfer is running and owns the bar.
    pub(crate) fn drag_line(&self) -> Option<String> {
        let drag = self.drag.as_ref()?;
        if self.pane.ops.transfer.is_some() {
            return None;
        }
        let lift = &drag.lift;
        let (name, copy) = match self.landing() {
            Some(l) => (l.name, l.copy),
            None => (String::new(), match lift.shelf.as_deref() {
                Some(shelf) => dragdrop::shelf_copying(shelf),
                None => dragdrop::copies(lift.copy, lift.src_dev, lift.src_dev),
            }),
        };
        // feedbackFor's count: this window's rows, or the paths a foreign drag carried.
        let count = if lift.own { lift.rows.len() } else { lift.paths.as_ref().map_or(0, Vec::len) };
        Some(dragdrop::line(count, &name, copy, lift.paths.as_ref().is_some_and(|p| !p.is_empty())))
    }

    // The row a drop would land on, and its word, for the frame the view draws over it.
    pub(crate) fn drop_row(&self) -> Option<(u8, usize, bool)> {
        self.landing().and_then(|l| l.row.map(|(s, i)| (s, i, l.copy)))
    }

    pub(crate) fn drag_drop(&mut self) -> Task<Message> {
        let landing = self.landing();
        self.drag = None;
        let Some(Landing { side, request, .. }) = landing else { return Task::none() };
        self.with_side(side, |f| f.run(vec![Effect::Send(request)]))
    }

    // A plain press on a row inside a selection waits for its release before it narrows the
    // selection to that row, because the press may yet lift the whole selection (Tap.tapped fires on
    // the release in QML); a release on the same row with no drag in between is the tap.
    pub(crate) fn row_released(&mut self, side: u8, index: usize) -> Task<Message> {
        if self.drag.is_some() || self.pending_collapse != Some((side, index)) {
            return Task::none();
        }
        self.pending_collapse = None;
        self.with_side(side, |f| {
            let effects = f.pane.tap(index, 1, false, false);
            f.run(effects)
        })
    }
}

// Row.qml's drop frame: a hairline of accent inset in the row over Style's hover wash.
pub(crate) fn drop_frame(accent: Color) -> container::Style {
    container::Style::default()
        .background(Background::Color(Color { a: 0.08, ..accent }))
        .border(Border { color: accent, width: 1.0, radius: 0.0.into() })
}

impl Flea {
    // Only while a drag is up does a target report the pointer entering and leaving it, so plain
    // pointer travel costs no message at all.
    pub(crate) fn drag_target<'a>(&self, area: MouseArea<'a, Message>, target: Target) -> MouseArea<'a, Message> {
        if self.drag.is_none() {
            return area;
        }
        area.on_enter(Message::DragHover(target, true)).on_exit(Message::DragHover(target, false))
    }
}

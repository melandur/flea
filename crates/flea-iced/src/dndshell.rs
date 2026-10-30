// The window's half of the platform drag: dnd.rs's events placed against the window by position,
// since the compositor owns the pointer for the length of a drag. A drag of this window's own rows
// keeps the lift it began with; anything else that enters, from another application or another Flea,
// becomes a drag that only ever copies (Drag.verbFor) or, from the shelf, one its token redeems. The
// drop then lands exactly where an in-window drag would: dragshell.rs's landing.
use crate::band;
use crate::dnd::{self, Dnd};
use crate::dragdrop::{self, Target};
use crate::dragshell::Drag;
use crate::pane::Effect;
use crate::{listcols, Flea, Message, PAD_X, ROW_H};
use iced::window::raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use iced::{Point, Task};

// The window's wl_display and wl_surface, which exist only on Wayland; X11 has no data device here.
pub(crate) fn handles() -> Task<Message> {
    iced::window::latest()
        .and_then(|id| {
            iced::window::run(id, |w| {
                let display = match w.display_handle().ok()?.as_raw() {
                    RawDisplayHandle::Wayland(d) => d.display.as_ptr() as usize,
                    _ => return None,
                };
                match w.window_handle().ok()?.as_raw() {
                    RawWindowHandle::Wayland(s) => Some((display, s.surface.as_ptr() as usize)),
                    _ => None,
                }
            })
        })
        .map(Message::DndHandles)
}

impl Flea {
    pub(crate) fn dnd_start(&mut self, handles: Option<(usize, usize)>) -> Task<Message> {
        self.dnd = handles.and_then(|(display, surface)| Dnd::start(display, surface));
        Task::none()
    }

    pub(crate) fn dnd_event(&mut self, event: dnd::Event) -> Task<Message> {
        match event {
            dnd::Event::Enter { x, y, own, uris, shelf } => {
                // Our own drag keeps its lift; a drag that is not ours copies, so no source deletes a
                // file on the strength of a drop it did not deliver.
                if !own || self.drag.is_none() {
                    self.drag = Some(Drag::new(dragdrop::foreign(&uris, shelf), true));
                }
                self.aim(x, y)
            }
            dnd::Event::Motion { x, y } => self.aim(x, y),
            dnd::Event::Leave => {
                match self.drag.as_mut() {
                    Some(d) if d.lift.own => (d.row, d.floor, d.tab) = (None, None, None),
                    _ => self.drag = None,
                }
                Task::none()
            }
            dnd::Event::Drop { x, y } => {
                let armed = self.aim(x, y);
                let landing = self.landing();
                if let Some(d) = self.dnd.as_ref() {
                    d.finish();
                }
                // Our own drag is over once it lands; the platform's Ended follows and finds nothing.
                self.drag = None;
                let sent = match landing {
                    Some(l) => {
                        let request = l.request;
                        self.with_side(l.side, |f| f.run(vec![Effect::Send(request)]))
                    }
                    None => Task::none(),
                };
                Task::batch([armed, sent])
            }
            dnd::Event::Ended => {
                if self.drag.as_ref().is_some_and(|d| d.lift.own && d.platform) {
                    self.drag = None;
                }
                Task::none()
            }
        }
    }

    // Where a platform drag is, in the window's own coordinates: a tab of the strip, or a pane's row
    // and floor from where band.rs last drew them; then the offer is taken where a drop would land
    // and refused elsewhere, the DropArea's accepted.
    fn aim(&mut self, x: f64, y: f64) -> Task<Message> {
        let scale = listcols::ui_scale(self.prefs.text_size, self.omarchy_base);
        let at = Point::new(x as f32 / scale, y as f32 / scale);
        let tab = self.tab_at(at);
        let (row, floor) = match band::hit(at).filter(|_| tab.is_none()) {
            Some((side, view)) => (view.and_then(|v| self.pane_on(side).and_then(|p| p.at_view(v))).map(|i| (side, i)), Some(side)),
            None => (None, None),
        };
        let was = self.drag.as_ref().and_then(|d| d.tab);
        let Some(drag) = self.drag.as_mut() else { return Task::none() };
        (drag.row, drag.floor) = (row, floor);
        // The hover switch is armed once per tab entered, as DropInto's entered is.
        let armed = match (was, tab) {
            (a, Some(i)) if a != Some(i) => self.drag_hover(Target::Tab(i), true),
            (Some(a), b) if b != Some(a) => self.drag_hover(Target::Tab(a), false),
            _ => Task::none(),
        };
        let mime = self.landing().map(|_| {
            let lift = &self.drag.as_ref().expect("aimed").lift;
            match (&lift.shelf, &lift.paths) {
                (Some(_), _) => dnd::SHELF_MIME,
                (None, Some(_)) => dnd::URI_MIME,
                (None, None) => dnd::ROWS_MIME,
            }
        });
        if let Some(d) = self.dnd.as_ref() {
            d.accept(mime);
        }
        armed
    }

    // TabBar's tabs: the strip under the chrome while there are two, each tab_width wide from its padding.
    fn tab_at(&self, at: Point) -> Option<usize> {
        if !self.tabs_open() || at.y < ROW_H || at.y >= 2.0 * ROW_H || at.x < PAD_X {
            return None;
        }
        let i = ((at.x - PAD_X) / self.tab_width()).floor() as usize;
        (i < self.pane.tab_count()).then_some(i)
    }
}

// The Convert and Open with cards' keyboards and pointer messages, as ConvertDialog.qml's and
// OpenWithDialog.qml's own Keys.onPressed hold them. Neither card holds a real text field: Open with's
// search is rule 8's "typing anywhere goes to the search", read straight off the key's text, which
// is what keeps Space and Enter meaning the control that has focus rather than a field's.
use crate::convertcard::FORMATS;
use crate::{Flea, Message};
use iced::keyboard::{key::Named, Key, Modifiers};
use iced::Task;

// Rule 5's viewport: seven applications before the list scrolls, and what PageDown moves.
pub const VIEWPORT_ROWS: isize = 7;
pub const PICK_H: f32 = crate::ROW_H - 6.0;
pub const EYEBROW_H: f32 = 26.0;
pub const OPEN_WITH_LIST: &str = "open-with-list";

// OpenWithDialog.moveCursor's positionViewAtIndex(Contain): the offset that keeps the cursor's row
// on the list's last visible line or above, taken from the drawn rows' own heights.
fn keep_in_view(c: &crate::openwith::OpenWith) -> Task<Message> {
    let mut top = 0.0;
    for r in c.rows() {
        match r {
            crate::openwith::Row::Eyebrow(_) => top += EYEBROW_H,
            crate::openwith::Row::App(_, seat) if seat == c.cursor => break,
            crate::openwith::Row::App(..) => top += PICK_H,
        }
    }
    let y = (top + PICK_H - VIEWPORT_ROWS as f32 * PICK_H).max(0.0);
    iced::widget::operation::scroll_to(OPEN_WITH_LIST, iced::widget::scrollable::AbsoluteOffset { x: 0.0, y })
}

#[derive(Debug, Clone)]
pub enum Msg {
    ConvertChoose(usize),
    ConvertStrip,
    ConvertCommit,
    ConvertClose,
    OpenWithPick(usize),
    OpenWithAlways,
    OpenWithCommit,
    OpenWithClose,
    // A script finished: None when it exited 0, which says nothing, else its own report.
    Script(Option<String>),
}

impl Flea {
    pub(crate) fn job_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::ConvertChoose(i) => self.convert_do(|c| c.choose(i)),
            Msg::ConvertStrip => self.convert_do(|c| {
                c.toggle_strip();
                vec![]
            }),
            Msg::ConvertCommit => self.convert_commit(),
            Msg::ConvertClose => {
                // A conversion under way is not something a dismissal can take back.
                if self.jobs.convert.as_ref().is_some_and(|c| !c.busy) {
                    self.jobs.convert = None;
                }
                Task::none()
            }
            Msg::OpenWithPick(seat) => {
                if let Some(c) = self.jobs.open_with.as_mut().filter(|c| !c.busy) {
                    c.cursor = seat;
                    c.focus = 1;
                }
                Task::none()
            }
            Msg::OpenWithAlways => {
                if let Some(c) = self.jobs.open_with.as_mut().filter(|c| !c.busy) {
                    c.always = !c.always;
                }
                Task::none()
            }
            Msg::OpenWithCommit => self.open_with_do(|c| c.commit()),
            Msg::OpenWithClose => self.open_with_do(|c| c.close()),
            Msg::Script(None) => Task::none(),
            Msg::Script(Some(line)) => self.show(line, true),
        }
    }

    fn convert_do(&mut self, f: impl FnOnce(&mut crate::convertcard::Convert) -> Vec<serde_json::Value>) -> Task<Message> {
        let out = self.jobs.convert.as_mut().map(f).unwrap_or_default();
        Task::batch(out.into_iter().map(|v| self.run(vec![crate::pane::Effect::Send(crate::wire::Request::Json(v))])).collect::<Vec<_>>())
    }

    // Ops.convert: the status line says what runs from the moment it is asked for.
    fn convert_commit(&mut self) -> Task<Message> {
        let Some(c) = self.jobs.convert.as_ref().filter(|c| c.can_convert()) else { return Task::none() };
        self.pane.sticky = format!("Converting {} to .{}", c.name, c.format());
        self.convert_do(|c| c.commit())
    }

    fn open_with_do(&mut self, f: impl FnOnce(&mut crate::openwith::OpenWith) -> Vec<crate::openwith::Out>) -> Task<Message> {
        let out = self.jobs.open_with.as_mut().map(f).unwrap_or_default();
        self.carry_open_with(out)
    }

    pub(crate) fn job_key(&mut self, key: &Key, modifiers: Modifiers, text: Option<&str>) -> Option<Task<Message>> {
        if let Some(c) = self.jobs.convert.as_mut() {
            let task = match key.as_ref() {
                Key::Named(Named::Escape) => return Some(self.job_message(Msg::ConvertClose)),
                Key::Named(Named::Tab) => {
                    c.step_focus(modifiers.shift());
                    Task::none()
                }
                _ if c.busy => Task::none(),
                Key::Named(Named::ArrowDown) => {
                    let next = (c.cursor + 1).min(FORMATS.len() - 1);
                    return Some(self.job_message(Msg::ConvertChoose(next)));
                }
                Key::Named(Named::ArrowUp) => {
                    let next = c.cursor.saturating_sub(1);
                    return Some(self.job_message(Msg::ConvertChoose(next)));
                }
                // Space toggles the one check box from the formats and the box itself.
                Key::Named(Named::Space) if c.focus < 2 => {
                    c.toggle_strip();
                    Task::none()
                }
                Key::Named(Named::Enter | Named::Space) => match c.focus {
                    2 => return Some(self.job_message(Msg::ConvertClose)),
                    1 => {
                        c.toggle_strip();
                        Task::none()
                    }
                    _ => return Some(self.convert_commit()),
                },
                _ => Task::none(),
            };
            return Some(task);
        }
        let c = self.jobs.open_with.as_mut()?;
        let task = match key.as_ref() {
            Key::Named(Named::Escape) => return Some(self.job_message(Msg::OpenWithClose)),
            Key::Named(Named::Tab) => {
                c.step_focus(modifiers.shift());
                Task::none()
            }
            _ if c.busy => Task::none(),
            Key::Named(Named::ArrowDown) => {
                c.move_cursor(1);
                keep_in_view(c)
            }
            Key::Named(Named::ArrowUp) => {
                c.move_cursor(-1);
                keep_in_view(c)
            }
            Key::Named(Named::PageDown) => {
                c.move_cursor(VIEWPORT_ROWS);
                keep_in_view(c)
            }
            Key::Named(Named::PageUp) => {
                c.move_cursor(-VIEWPORT_ROWS);
                keep_in_view(c)
            }
            // Backspace edits the search from anywhere, the mirror of typing reaching it.
            Key::Named(Named::Backspace) => {
                let mut q = c.query.clone();
                q.pop();
                c.search(q);
                c.focus = 0;
                Task::none()
            }
            // Space answers the control with focus; only from the search with text in it is it a character.
            Key::Named(Named::Space) if c.focus == 3 => return Some(self.job_message(Msg::OpenWithClose)),
            Key::Named(Named::Space) if c.focus == 4 => return Some(self.job_message(Msg::OpenWithCommit)),
            Key::Named(Named::Space) if c.focus != 0 || c.query.is_empty() => {
                c.always = !c.always;
                Task::none()
            }
            Key::Named(Named::Enter) => match c.focus {
                3 => return Some(self.job_message(Msg::OpenWithClose)),
                2 => {
                    c.always = !c.always;
                    Task::none()
                }
                _ => return Some(self.job_message(Msg::OpenWithCommit)),
            },
            _ => match text.filter(|t| !modifiers.control() && t.chars().next().is_some_and(|ch| !ch.is_control())) {
                Some(t) => {
                    let q = format!("{}{t}", c.query);
                    c.search(q);
                    c.focus = 0;
                    keep_in_view(c)
                }
                None => Task::none(),
            },
        };
        Some(task)
    }
}

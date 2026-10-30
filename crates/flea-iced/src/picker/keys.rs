// The chooser's keyboard: Escape refuses, Tab walks the ring ui/picker.qml stepFocus builds, and each
// stop answers its own keys, the list taking what keys.toml binds for the listing.
use super::app::{Focus, PMsg, Picking, SAVE_ID};
use iced::keyboard::{self, key::Named, Key};
use iced::widget::operation;
use iced::Task;

impl Picking {
    pub(super) fn key(&mut self, event: keyboard::Event) -> Task<PMsg> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else { return Task::none() };
        if self.c.answered {
            return Task::none();
        }
        let named = |n: Named| key == Key::Named(n);
        if named(Named::Escape) {
            return self.update(PMsg::Cancel);
        }
        if named(Named::Tab) {
            return self.step_focus(modifiers.shift());
        }
        let effects = match self.focus {
            Focus::Rail => {
                let last = self.places.len().saturating_sub(1);
                if named(Named::ArrowDown) {
                    self.rail_cursor = (self.rail_cursor + 1).min(last);
                } else if named(Named::ArrowUp) {
                    self.rail_cursor = self.rail_cursor.saturating_sub(1);
                } else if named(Named::Enter) || named(Named::ArrowRight) || named(Named::Space) {
                    return self.choose_place(self.rail_cursor);
                }
                vec![]
            }
            Focus::KeepFile if named(Named::Enter) || named(Named::Space) => self.c.cancel(),
            Focus::UseLocation if named(Named::Enter) || named(Named::Space) => self.c.accept(true),
            Focus::List if modifiers.control() && key == Key::Character("h".into()) => self.c.toggle_hidden(),
            Focus::List if modifiers.is_empty() || modifiers == keyboard::Modifiers::SHIFT => {
                let cursor = self.c.cursor;
                if named(Named::ArrowDown) {
                    self.c.move_cursor(1);
                    vec![]
                } else if named(Named::ArrowUp) {
                    self.c.move_cursor(-1);
                    vec![]
                } else if named(Named::Space) && modifiers.is_empty() {
                    self.c.toggle_mark(cursor)
                } else if named(Named::Enter) {
                    self.c.activate(cursor)
                } else if named(Named::ArrowRight) {
                    self.c.enter(cursor)
                } else if named(Named::ArrowLeft) {
                    self.c.go_up()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };
        self.run(effects)
    }

    fn step_focus(&mut self, back: bool) -> Task<PMsg> {
        let mut ring = vec![Focus::Rail, Focus::List];
        if self.c.req.saving() {
            ring.push(Focus::Save);
            if self.c.save_collision() {
                ring.extend([Focus::KeepFile, Focus::UseLocation]);
            }
        }
        let at = ring.iter().position(|f| *f == self.focus).unwrap_or(1);
        let n = ring.len();
        self.focus = ring[if back { (at + n - 1) % n } else { (at + 1) % n }];
        // Focusing an id nothing carries is how every field gives the keyboard back.
        operation::focus(iced::widget::Id::new(if self.focus == Focus::Save { SAVE_ID } else { "none" }))
    }
}

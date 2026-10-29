// The keyboard and the pointer's tap count: what keys.toml binds for the listing, routed to the pane.
use crate::{Flea, Message, DOUBLE_TAP};
use iced::keyboard::{self, key::Named, Key};
use iced::Task;
use std::time::Instant;

impl Flea {
    pub(crate) fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        if let keyboard::Event::ModifiersChanged(m) = event {
            self.modifiers = m;
            return Task::none();
        }
        let keyboard::Event::KeyPressed { key, modifiers, repeat, .. } = event else { return Task::none() };
        if key == Key::Named(Named::Escape) && self.message.as_ref().is_some_and(|m| m.2) {
            // An error is dismissed before anything else Escape unwinds.
            self.message = None;
            return Task::none();
        }
        // Tab is the only thing that moves the keyboard between the views, and a hidden rail is not
        // a place it can go (ui/js/Focus.js next).
        if key == Key::Named(Named::Tab) && !modifiers.control() && self.pane.renaming.is_none() {
            self.rail.menu = None;
            self.rail.focused = !self.rail.focused && self.rail.shown;
            return Task::none();
        }
        if key == Key::Character("g".into()) && modifiers.control() {
            return self.toggle_rail();
        }
        if let Some(task) = self.rail_key(&key) {
            return task;
        }
        if key == Key::Named(Named::Escape) {
            let effects = self.pane.escape();
            return self.run(effects);
        }
        // The editor owns every other key while it is open; Enter reaches it through on_submit.
        if self.pane.renaming.is_some() {
            return Task::none();
        }
        let effects = match (key.as_ref(), modifiers.control()) {
            (Key::Named(Named::ArrowDown), false) => self.pane.step(1),
            (Key::Named(Named::ArrowUp), false) => self.pane.step(-1),
            (Key::Named(Named::ArrowLeft), false) => self.pane.up(),
            (Key::Named(Named::ArrowRight), false) => self.pane.forward(),
            (Key::Named(Named::Enter), false) => self.pane.activate(),
            (Key::Named(Named::F5), false) => self.pane.reread(),
            (Key::Character("h"), true) => self.pane.toggle_hidden(),
            (Key::Character("c"), true) => self.pane.copy(false),
            (Key::Character("x"), true) => self.pane.copy(true),
            (Key::Character("v"), true) => self.pane.paste(),
            (Key::Character("z"), true) => self.pane.undo(),
            (Key::Character("a"), true) => self.pane.select_all(),
            (Key::Character("n"), true) => self.pane.new_folder(),
            (Key::Named(Named::Delete), false) => self.pane.trash(),
            (Key::Named(Named::F2), false) => self.pane.start_rename(),
            // A held Space toggles once; the paint gesture of ui/js/Marks.js is not ported yet.
            (Key::Named(Named::Space), false) if !repeat => self.pane.toggle_select(),
            (Key::Character("m"), false) => return self.say("The listing menu is not in this build yet.".into()),
            (Key::Character(c), true) if "fbtmjkl".contains(c) => {
                return self.say(format!("Ctrl+{} is not in this build yet.", c.to_uppercase()));
            }
            _ => return Task::none(),
        };
        self.run(effects)
    }

    // ui/js/Tap.js reads a tap count, and iced's press arrives on both clicks of a double, so the
    // count is taken here: a second press on the same row inside the interval is the second tap.
    pub(crate) fn tap(&mut self, index: usize) -> Task<Message> {
        let now = Instant::now();
        let taps = match self.last_tap {
            Some((i, at)) if i == index && now.duration_since(at) < DOUBLE_TAP => 2,
            _ => 1,
        };
        self.last_tap = if taps == 1 { Some((index, now)) } else { None };
        // A click in the listing is the listing taking the keyboard back from the rail.
        self.rail.focused = false;
        self.rail.menu = None;
        let effects = self.pane.tap(index, taps, self.modifiers.control(), self.modifiers.shift());
        self.run(effects)
    }
}

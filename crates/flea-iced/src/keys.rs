// The keyboard and the pointer's tap count: what keys.toml binds for the listing, routed to the pane.
use crate::{Flea, Message, DOUBLE_TAP};
use iced::keyboard::{self, key::Named, Key};
use iced::Task;
use std::time::Instant;

// A focused text field captures Escape to unfocus itself, so keyboard::listen never sees the one
// press every editor here closes on. This hands exactly those captured presses to the same router;
// an uncaptured Escape still arrives through keyboard::listen alone, so none is handled twice.
pub(crate) fn captured_escape(event: iced::Event, status: iced::event::Status, _: iced::window::Id) -> Option<keyboard::Event> {
    match (event, status) {
        (iced::Event::Keyboard(e @ keyboard::Event::KeyPressed { key: Key::Named(Named::Escape), .. }), iced::event::Status::Captured) => Some(e),
        _ => None,
    }
}

impl Flea {
    pub(crate) fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        if let keyboard::Event::ModifiersChanged(m) = event {
            self.modifiers = m;
            return Task::none();
        }
        let keyboard::Event::KeyPressed { key, modifiers, repeat, text, .. } = event else { return Task::none() };
        // FLEA_ICED_TRACE: every key the window was handed, beside the requests it caused.
        if std::env::var_os("FLEA_ICED_TRACE").is_some_and(|v| !v.is_empty()) {
            eprintln!("flea-iced: key {key:?} {modifiers:?} repeat={repeat} text={text:?}");
        }
        // An open settings panel takes every key, so none reaches the listing behind it.
        if self.panel.open && self.dialog.is_none() {
            return self.panel_key(&key);
        }
        // "," opens the panel from anywhere in the window that is not a field being typed in.
        if text.as_deref() == Some(",") && !modifiers.control() && self.dialog.is_none() && !self.chrome.editing
            && self.pane.renaming.is_none() && self.rail.renaming.is_none() && self.pane.search.mode != crate::search::Mode::Typing
        {
            return self.open_panel();
        }
        // The dialog owns the keyboard while it is up: Escape closes it, Tab walks its fields, and
        // its fields take everything else, Enter included through their on_submit.
        if self.dialog.is_some() {
            return match key.as_ref() {
                Key::Named(Named::Escape) => self.dialog_close(),
                Key::Named(Named::Tab) if modifiers.shift() => iced::widget::operation::focus_previous(),
                Key::Named(Named::Tab) => iced::widget::operation::focus_next(),
                _ => Task::none(),
            };
        }
        // The path bar owns the keys while it is open: Escape closes it, Tab completes the leaf.
        if self.chrome.editing {
            return match key.as_ref() {
                Key::Named(Named::Escape) => {
                    self.close_path_edit();
                    Task::none()
                }
                Key::Named(Named::Tab) => self.complete_path(),
                _ => Task::none(),
            };
        }
        if let Some(task) = self.preview_key(&key) {
            return task;
        }
        // The rail's rename field owns the keys while it is open; Escape is the one it hands back.
        if self.rail.renaming.is_some() {
            if key == Key::Named(Named::Escape) {
                self.rail.renaming = None;
            }
            return Task::none();
        }
        if key == Key::Named(Named::Escape) && self.message.as_ref().is_some_and(|m| m.2) {
            // An error is dismissed before anything else Escape unwinds.
            self.message = None;
            return Task::none();
        }
        if let Some(task) = self.search_key(&key, modifiers, text.as_deref()) {
            return task;
        }
        // Tab is the only thing that moves the keyboard between the views, and a hidden rail is not
        // a place it can go (ui/js/Focus.js next).
        if key == Key::Named(Named::Tab) && !modifiers.control() && self.pane.renaming.is_none() {
            self.rail.menu = None;
            // ui/js/Focus.js: in the split, Tab from the listing goes to the other pane.
            if self.dual() && !self.rail.focused {
                return self.switch_pane();
            }
            self.rail.focused = !self.rail.focused && self.rail.shown;
            return Task::none();
        }
        if key == Key::Character("g".into()) && modifiers.control() {
            return self.toggle_rail();
        }
        if let Some(task) = self.rail_key(&key) {
            return task;
        }
        if let Some(task) = self.share_key(&key) {
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
            // Ctrl+F searches this folder, Ctrl+Shift+F it and every folder below (2026-09-23).
            (Key::Character("f" | "F"), true) => self.pane.search_start(modifiers.shift()),
            (Key::Character("j"), true) => self.pane.sort_chord("name"),
            (Key::Character("k"), true) => self.pane.sort_chord("size"),
            (Key::Character("l"), true) => self.pane.sort_chord("mtime"),
            (Key::Character("t"), true) => return self.toggle_split(),
            (Key::Character(c), true) if "bm".contains(c) => {
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

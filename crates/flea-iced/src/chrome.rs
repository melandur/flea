// The chrome above the listing, ui/ChromeBar.qml: the path drawn as crumbs, each one its own click
// target, and the same strip typed into as the path bar after a double click on it. A single click
// waits out the double-click interval before it navigates, because one target cannot host both
// gestures and answer the first one early (AGENTS.md "The key table is generated", the crumb note).
use crate::pathbar::{self, Crumb};
use crate::wire::{Reply, Request};
use crate::{sleep, Flea, Message, DOUBLE_TAP, PAD_X, ROW_H, TEXT_PX};
use iced::widget::{container, mouse_area, row, text, text_input, Id, Row};
use iced::{Element, Length, Task};
use std::time::Instant;

pub const PATH_ID: &str = "path-bar";

#[derive(Debug, Default)]
pub struct Chrome {
    pub editing: bool,
    pub field: String,
    tap: Option<(usize, Instant)>,
    tap_gen: u64,
    // The peek a Tab is waiting on, and the last folder's names, keyed by folder and hidden flag.
    pending: Option<(String, bool)>,
    cached: Option<((String, bool), Vec<String>)>,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

impl Flea {
    pub(crate) fn crumbs(&self) -> Vec<Crumb> {
        pathbar::crumbs(&self.pane.path, &home())
    }

    pub(crate) fn crumb_pressed(&mut self, index: usize) -> Task<Message> {
        let now = Instant::now();
        if let Some((i, at)) = self.chrome.tap.take() {
            if i == index && now.duration_since(at) < DOUBLE_TAP {
                self.chrome.tap_gen += 1;
                return self.start_path_edit();
            }
        }
        self.chrome.tap = Some((index, now));
        self.chrome.tap_gen += 1;
        let generation = self.chrome.tap_gen;
        Task::perform(sleep(DOUBLE_TAP), move |_| Message::CrumbSettle(generation))
    }

    // The interval passed with no second tap: a crumb that is not the folder already listed opens.
    pub(crate) fn crumb_settled(&mut self, generation: u64) -> Task<Message> {
        if generation != self.chrome.tap_gen {
            return Task::none();
        }
        let Some((index, _)) = self.chrome.tap.take() else { return Task::none() };
        let Some(c) = self.crumbs().into_iter().nth(index).filter(|c| !c.last) else { return Task::none() };
        let effects = self.pane.open(c.path, None);
        self.run(effects)
    }

    // ChromeBar.startEdit: the line opens on the path with its trailing slash, so typing a child is
    // the natural next keystroke.
    pub(crate) fn start_path_edit(&mut self) -> Task<Message> {
        self.chrome.editing = true;
        self.chrome.field = if self.pane.path == "/" { "/".into() } else { format!("{}/", self.pane.path) };
        Task::batch([iced::widget::operation::focus(PATH_ID), iced::widget::operation::move_cursor_to_end(PATH_ID)])
    }

    pub(crate) fn close_path_edit(&mut self) {
        self.chrome.editing = false;
        self.chrome.field.clear();
    }

    // ChromeBar.commitEdit: an empty line and the path already shown close the bar and do nothing
    // else; a line that named a file on another host says why it went nowhere.
    pub(crate) fn commit_path_edit(&mut self) -> Task<Message> {
        let typed = std::mem::take(&mut self.chrome.field);
        let target = pathbar::resolve(&typed, &self.pane.path, &home());
        self.close_path_edit();
        if !target.is_empty() && target != self.pane.path {
            let effects = self.pane.open(target, None);
            return self.run(effects);
        }
        if pathbar::refused(&typed) {
            return self.say("That URI names a file on another host, not a path on this machine.".into());
        }
        Task::none()
    }

    // Tab: the folder the typed head names is peeked once, and its folder names complete the leaf.
    pub(crate) fn complete_path(&mut self) -> Task<Message> {
        let dir = pathbar::completion_dir(&self.chrome.field, &self.pane.path, &home());
        let hidden = pathbar::wants_hidden(&self.chrome.field, self.pane.hidden);
        if let Some((key, names)) = self.chrome.cached.clone() {
            if key == (dir.clone(), hidden) {
                return self.apply_completion(&dir, &names);
            }
        }
        self.chrome.pending = Some((dir.clone(), hidden));
        let first = self.pane.window_size();
        match self.backend.as_ref().map(|b| b.send(&Request::Peek { path: dir, first, hidden })) {
            Some(Err(e)) => self.say(e),
            _ => Task::none(),
        }
    }

    fn apply_completion(&mut self, dir: &str, names: &[String]) -> Task<Message> {
        let before = self.chrome.field.clone();
        let after = pathbar::complete(&before, names);
        self.chrome.field = after.0.clone();
        let moved = iced::widget::operation::move_cursor_to_end(PATH_ID);
        match pathbar::completion_message(&before, &after, dir) {
            Some(m) => Task::batch([moved, self.say(m)]),
            None => moved,
        }
    }

    // A peek is matched on its folder and its hidden flag, because two clients read this wire.
    pub(crate) fn peeked(&mut self, reply: Reply) -> Task<Message> {
        let Reply::Peeked { path, hidden, rows } = reply else { return Task::none() };
        if self.chrome.pending.as_ref() != Some(&(path.clone(), hidden)) {
            return Task::none();
        }
        self.chrome.pending = None;
        let names: Vec<String> = rows.into_iter().filter(|r| r.d).map(|r| r.n).collect();
        self.chrome.cached = Some(((path.clone(), hidden), names.clone()));
        let still = pathbar::completion_dir(&self.chrome.field, &self.pane.path, &home()) == path;
        if self.chrome.editing && still { self.apply_completion(&path, &names) } else { Task::none() }
    }

    pub(crate) fn chrome_view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let body: Element<'_, Message> = if self.chrome.editing {
            text_input("", &self.chrome.field)
                .id(Id::new(PATH_ID))
                .on_input(Message::PathInput)
                .on_submit(Message::PathSubmit)
                .size(TEXT_PX)
                .into()
        } else {
            let mut strip = Row::new();
            for (i, c) in self.crumbs().into_iter().enumerate() {
                let ink = if c.last { p.foreground } else { p.muted };
                strip = strip.push(mouse_area(text(c.text).color(ink)).on_press(Message::CrumbPressed(i)));
            }
            strip.into()
        };
        container(row![body].align_y(iced::Center))
            .padding([0.0, PAD_X])
            .height(ROW_H)
            .width(Length::Fill)
            .align_y(iced::Center)
            .style(move |_| iced::widget::container::Style::default().background(p.surface))
            .into()
    }
}

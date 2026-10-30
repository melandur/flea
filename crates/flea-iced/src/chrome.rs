// The chrome above the listing, ui/ChromeBar.qml: the path drawn as crumbs, each one its own click
// target, and the same strip typed into as the path bar after a double click on it. A single click
// waits out the double-click interval before it navigates, because one target cannot host both
// gestures and answer the first one early (AGENTS.md "The key table is generated", the crumb note).
use crate::pathbar::{self, Crumb};
use crate::wire::{Reply, Request};
use crate::{marks, sleep, Flea, Message, DOUBLE_TAP, MARK, PAD_X, ROW_H, TEXT_PX};
use iced::widget::{container, mouse_area, row, space, svg, text, text_input, Id, Row};
use iced::{Element, Length, Task};
use std::time::Instant;

pub const PATH_ID: &str = "path-bar";

// The chrome's buttons and the two other surfaces it opens or answers for: ChromeBar's back, up,
// search, view modes and settings, the key sheet's close, and the mouse's back button.
#[derive(Debug, Clone)]
pub enum Act {
    Back,
    Up,
    Search,
    Filter,
    Sort,
    Mode(&'static str),
    Settings,
    SheetClose,
    MouseBack,
    // WindowDrag: a press on the bar's own ground, the pointer travelling over it, and the release.
    Grab,
    Travel(iced::Point),
    Let,
}

// WindowDrag's dragThreshold: well under Qt's own 10, because a title bar reads a few pixels of
// travel as intent.
const WINDOW_DRAG_PX: f32 = 4.0;

// The mouse's back button, which belongs to no row: keys.toml's [[pointer]] where = "window".
pub(crate) fn mouse_back(event: iced::Event, _: iced::event::Status, _: iced::window::Id) -> Option<Act> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back)) => Some(Act::MouseBack),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub struct Chrome {
    pub editing: bool,
    pub field: String,
    tap: Option<(usize, Instant)>,
    tap_gen: u64,
    // The peek a Tab is waiting on, and the last folder's names, keyed by folder and hidden flag.
    pending: Option<(String, bool)>,
    cached: Option<((String, bool), Vec<String>)>,
    // A press on the bar's ground, and where the pointer first moved after it.
    grab: Option<Option<iced::Point>>,
}

// Format.tilde: home itself or home and a separator, never a sibling wearing home's name.
fn tilde(path: &str, home: &str) -> String {
    if !home.is_empty() && (path == home || path.starts_with(&format!("{home}/"))) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_string()
}

// Text.ElideLeft by character count: the head goes, an ellipsis in its place.
fn elide_head(s: &str, budget: usize) -> String {
    let n = s.chars().count();
    if n <= budget {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - budget.saturating_sub(1)).collect();
    format!("…{tail}")
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

impl Flea {
    pub(crate) fn chrome_act(&mut self, act: Act) -> Task<Message> {
        // ui/WindowDrag.qml: the press is a click on whatever is under it until it travels, and then
        // the move is the compositor's (startSystemMove).
        match act {
            Act::Grab => {
                self.chrome.grab = Some(None);
                return Task::none();
            }
            Act::Let => {
                self.chrome.grab = None;
                return Task::none();
            }
            Act::Travel(at) => {
                let Some(origin) = self.chrome.grab else { return Task::none() };
                let Some(origin) = origin else {
                    self.chrome.grab = Some(Some(at));
                    return Task::none();
                };
                if origin.distance(at) < WINDOW_DRAG_PX {
                    return Task::none();
                }
                self.chrome.grab = None;
                return iced::window::latest().and_then(iced::window::drag);
            }
            _ => {}
        }
        // Nav.mouseBack: a press behind an open menu, card or panel goes nowhere.
        let covered = self.menu.is_some() || self.card.is_some() || self.panel.open || self.dialog.is_some() || self.sheet_open;
        let effects = match act {
            Act::Grab | Act::Travel(_) | Act::Let => return Task::none(),
            Act::SheetClose => {
                self.sheet_open = false;
                return Task::none();
            }
            _ if covered => return Task::none(),
            Act::Back => self.pane.back(),
            Act::MouseBack => self.pane.mouse_back(),
            Act::Up => self.pane.up(),
            Act::Search => self.pane.search_start(false),
            // The grid's two: the filter over the held rows, and the next order the backend makes.
            Act::Filter => {
                self.pane.filter_start();
                vec![]
            }
            Act::Sort => self.pane.sort_next(),
            Act::Settings => return self.open_panel(),
            Act::Mode(mode) => return self.choose_view(mode),
        };
        self.run(effects)
    }

    // shell.qml onViewChosen: "dual" toggles the split in, any other mode is the view key, and a mode
    // chosen while split leaves it, because the split's panes are list-only (Pane.qml listOnly).
    fn choose_view(&mut self, mode: &'static str) -> Task<Message> {
        if mode == "dual" {
            return if self.other.is_none() { self.toggle_split() } else { Task::none() };
        }
        let was = (self.grid_mode(), self.columns_mode());
        self.dual_from = mode.to_string();
        let left = if self.other.is_some() { self.toggle_split() } else { crate::split::save_view_mode(mode) };
        Task::batch([left, self.after_view_change(was)])
    }

    // What a view change costs once the mode is set, whether chosen or restored by a tab.
    pub(crate) fn after_view_change(&mut self, was: (bool, bool)) -> Task<Message> {
        if was == (self.grid_mode(), self.columns_mode()) {
            return Task::none();
        }
        // Pane.qml onViewModeChanged: only the views that draw a filter keep one, and the held
        // thumbnails are decoded again at the new view's own size.
        // Focus.lookup: the columns view offers no filter, so leaving the grid takes it away.
        if !self.grid_mode() {
            self.pane.filter_close();
        }
        let files: Vec<String> = self.pane.thumbs.keys().filter_map(|i| self.pane.thumb_file(*i).map(String::from)).collect();
        let decodes: Vec<Task<Message>> = files.into_iter().map(|f| self.decode_task(f)).collect();
        self.scroll_y = 0.0;
        let scrolled = self.run(vec![crate::pane::Effect::ScrollTo(0)]);
        let reported = self.report_viewport();
        Task::batch([scrolled, reported, Task::batch(decodes)])
    }

    fn chrome_button(&self, glyph: &'static str, enabled: bool, ink: iced::Color, act: Act) -> Element<'_, Message> {
        let p = self.palette;
        let mark = svg(marks::handle(glyph, if enabled { ink } else { iced::Color { a: 0.35, ..p.muted } })).width(MARK).height(MARK);
        let area = mouse_area(container(mark).padding(4));
        if enabled { area.on_press(Message::Chrome(act)).into() } else { area.into() }
    }

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
        let Reply::Peeked { path, hidden, rows, failed } = reply else { return Task::none() };
        if self.columns.answered(&path, hidden, rows.clone(), failed) {
            return Task::none();
        }
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
        } else if self.prefs.address_bar == "path" {
            // ChromeBar's Path: the folder as one line, home spelled ~, its head cut when it is too
            // long because the tail is what names the folder; a double press opens the field, which
            // is the last crumb's own rule, so it goes through the same press.
            let last = self.crumbs().len().saturating_sub(1);
            let budget = ((self.window_w - 330.0) / (TEXT_PX * 0.55)).max(8.0) as usize;
            mouse_area(text(elide_head(&tilde(&self.pane.path, &home()), budget)).color(p.foreground).wrapping(text::Wrapping::None))
                .on_press(Message::CrumbPressed(last))
                .into()
        } else {
            let mut strip = Row::new();
            for (i, c) in self.crumbs().into_iter().enumerate() {
                let ink = if c.last { p.foreground } else { p.muted };
                strip = strip.push(mouse_area(text(c.text).color(ink)).on_press(Message::CrumbPressed(i)));
            }
            strip.into()
        };
        let p2 = p;
        let mode = if self.other.is_some() { "dual" } else { self.dual_from.as_str() };
        // ChromeBar: the grid trades Search for the filter and the sort; both stand down while a
        // search is up, because the search owns the header then.
        let free = self.pane.search.mode == crate::search::Mode::Off;
        let mut right = if self.grid_mode() {
            row![self.chrome_button("filter", free, p.foreground, Act::Filter), self.chrome_button("sort", free, p.foreground, Act::Sort)]
        } else {
            row![self.chrome_button("search", true, p.foreground, Act::Search)]
        }
        .spacing(4)
        .align_y(iced::Center);
        for m in ["list", "columns", "grid", "dual"] {
            right = right.push(self.chrome_button(m, true, if m == mode { p.foreground } else { p.muted }, Act::Mode(m)));
        }
        let rule = container(space().width(1).height(MARK - 1.0)).style(move |_| iced::widget::container::Style::default().background(iced::Color { a: 0.12, ..p2.foreground }));
        right = right.push(rule).push(self.chrome_button("sliders", true, p.foreground, Act::Settings));
        let left = row![
            self.chrome_button("arrow-left", !self.pane.history.is_empty(), p.foreground, Act::Back),
            self.chrome_button("arrow-up", crate::pane::parent(&self.pane.path).is_some(), p.foreground, Act::Up),
        ]
        .spacing(4)
        .align_y(iced::Center);
        let bar = container(row![left, container(body).width(Length::Fill), right].spacing(12).align_y(iced::Center))
            .padding([0.0, PAD_X])
            .height(ROW_H)
            .width(Length::Fill)
            .align_y(iced::Center)
            .style(move |_| iced::widget::container::Style::default().background(p.surface));
        // The buttons and the crumbs take their own presses; only the bar's ground reaches this, and
        // it hears the pointer travel only between that press and its release.
        // The path being typed in selects text under a travelling press; the window stays put.
        if self.chrome.editing {
            return bar.into();
        }
        let grab = mouse_area(bar).on_press(Message::Chrome(Act::Grab)).on_release(Message::Chrome(Act::Let));
        if self.chrome.grab.is_some() {
            return grab.on_move(|at| Message::Chrome(Act::Travel(at))).on_exit(Message::Chrome(Act::Let)).into();
        }
        grab.into()
    }
}

#[cfg(test)]
mod tests {
    use super::{elide_head, tilde};

    #[test]
    fn the_path_bar_spells_home_and_loses_its_head_first() {
        assert_eq!(tilde("/home/u/docs", "/home/u"), "~/docs");
        assert_eq!(tilde("/home/u", "/home/u"), "~");
        assert_eq!(tilde("/home/ux", "/home/u"), "/home/ux", "issue 95: a sibling is not home");
        assert_eq!(elide_head("~/a/b/c", 20), "~/a/b/c");
        assert_eq!(elide_head("/very/long/path/name", 8), "…th/name");
    }
}

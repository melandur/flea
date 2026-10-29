// The listing's context menu in the window, as ui/ContextMenu.qml and ui/PaneMenuActions.qml hold
// it: opened by a right click, m, the Menu key or Shift+F10, stepped by the arrows, and every chosen
// row dispatched to the pane operation the same key already reaches. menu.rs decides what it holds.
use crate::menu::{self, Item, Kind};
use crate::pane::join;
use crate::{blocking, Flea, Message, ROW_H};
use iced::keyboard::{key::Named, Key};
use iced::Task;

// Theme.menuWidth, Style.space(220) at the default size ratio.
pub const MENU_W: f32 = 220.0;
// One menu row and one separator, MenuRow's own heights at this box's spacing.
pub const ITEM_H: f32 = ROW_H - 6.0;
pub const SEPARATOR_H: f32 = 9.0;
pub const FRAME_PAD: f32 = 4.0;

#[derive(Debug, Clone)]
pub enum Msg {
    // A right click on a row of the pane on that side, or on the empty space below its rows.
    Row(u8, usize),
    Background(u8),
    Chose(usize),
    SubChose(usize),
    Close,
    Favourite(Result<Vec<serde_json::Value>, String>),
    Terminal(Option<i32>),
    Copied(bool),
}

#[derive(Debug, Clone)]
pub struct ListMenu {
    pub kind: Kind,
    pub items: Vec<Item>,
    pub cursor: usize,
    // The flyout of the row under the cursor, and its own cursor.
    pub flyout: bool,
    pub sub_cursor: usize,
    // Where the frame sits, in window coordinates, already clamped to the window.
    pub x: f32,
    pub y: f32,
    // The directory and the target names the rows were built from; a choice made after either
    // changed is refused, ui/PaneMenuActions.qml's "Selected items changed; reopen the menu."
    path: String,
    names: Vec<String>,
}

pub fn frame_height(items: &[Item]) -> f32 {
    2.0 * FRAME_PAD + items.iter().map(|i| if *i == Item::Separator { SEPARATOR_H } else { ITEM_H }).sum::<f32>()
}

impl Flea {
    pub(crate) fn menu_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Row(side, index) => {
                // Closed before the focus moves, so the pane it was open over stops being busy.
                self.close_menu();
                let focused = self.focus_side(side);
                Task::batch([focused, self.open_row_menu(index)])
            }
            Msg::Background(side) => {
                self.close_menu();
                let focused = self.focus_side(side);
                Task::batch([focused, self.open_menu(Kind::Background, None)])
            }
            Msg::Chose(i) => self.menu_choose(i),
            Msg::SubChose(i) => self.menu_choose_sub(i),
            Msg::Close => {
                self.close_menu();
                Task::none()
            }
            Msg::Favourite(Ok(records)) => {
                self.rail.favourites = records;
                self.rebuild_rail();
                Task::none()
            }
            Msg::Favourite(Err(line)) => self.show(line, true),
            Msg::Terminal(code) => {
                self.opening_terminal = false;
                if code == Some(0) { Task::none() } else { self.show("No terminal on this system opened that directory.".into(), true) }
            }
            Msg::Copied(ok) => if ok { Task::none() } else { self.show("The path could not be copied to the clipboard.".into(), true) },
        }
    }

    // Tap.tappedMenu: pressed inside the selection the menu addresses all of it, pressed outside it
    // that row replaces the selection, and the row takes the cursor either way.
    fn open_row_menu(&mut self, index: usize) -> Task<Message> {
        if self.pane.row_at(index).is_none() {
            return Task::none();
        }
        if !self.pane.selection.is_empty() && !self.pane.selection.contains(&index) {
            self.pane.selection.clear();
        }
        let effects = self.pane.place_cursor(index);
        let moved = self.run(effects);
        Task::batch([moved, self.open_menu(Kind::File, Some(index))])
    }

    // m, the Menu key and Shift+F10: the cursor row's menu, under that row; Menu.openAtCursor.
    pub(crate) fn open_cursor_menu(&mut self) -> Task<Message> {
        if self.pane.row_at(self.pane.cursor).is_none() {
            return self.say("No row under the cursor to open a menu on.".into());
        }
        let effects = self.pane.place_cursor(self.pane.cursor);
        let moved = self.run(effects);
        Task::batch([moved, self.open_menu(Kind::File, Some(self.pane.cursor))])
    }

    fn targets(&self) -> Vec<String> {
        let rows: Vec<usize> = if self.pane.selection.is_empty() { vec![self.pane.cursor] } else { self.pane.selection.iter().copied().collect() };
        rows.into_iter().filter_map(|i| self.pane.row_at(i).map(|r| r.n.clone())).collect()
    }

    fn open_menu(&mut self, kind: Kind, row: Option<usize>) -> Task<Message> {
        self.close_menu();
        // A file menu asks every send row's service again; the rows grey until they answer.
        let asked = if kind == Kind::File { self.refresh_sends() } else { self.ask_formats() };
        let items = menu::entries(kind, &self.menu_facts(kind), &self.effective_hidden());
        let Some(cursor) = menu::first(&items) else { return Task::none() };
        // Under the row's bottom-left like the rail's, or under the last row over empty space. A
        // right click opens there too: iced reports no pointer position with the press itself.
        // Measured in view positions, so a filtered row's menu opens under the row as it is drawn; the
        // grid's under the tile, at the tile's own column, and with no header band above its tiles.
        let (x0, width) = self.pane_box();
        let view = row.and_then(|i| self.pane.view_of(i));
        let (below, left, listing_top) = if self.grid_mode() {
            let g = self.grid_geometry();
            let below = match view {
                Some(v) => (v / g.columns + 1) as f32 * g.cell_h,
                None => self.pane.view_total().div_ceil(g.columns) as f32 * g.cell_h,
            };
            (below, view.map_or(0.0, |v| (v % g.columns) as f32 * g.cell_w), ROW_H)
        } else {
            let below = view.map_or(self.pane.view_total(), |v| v + 1) as f32 * ROW_H;
            (below, 0.0, 2.0 * ROW_H)
        };
        let y_in = (below - self.scroll_y).clamp(0.0, self.viewport_h);
        let window_h = self.viewport_h + 3.0 * ROW_H;
        let x = x0 + menu::clamp(left + crate::PAD_X, MENU_W, width);
        let y = menu::clamp(listing_top + y_in, frame_height(&items), window_h);
        self.menu = Some(ListMenu { kind, items, cursor, flyout: false, sub_cursor: 0, x, y, path: self.pane.path.clone(), names: self.targets() });
        self.pane.menu_open = true;
        // Asked again on every menu, so a tool installed while the window was open counts next time.
        asked
    }

    pub(crate) fn close_menu(&mut self) {
        self.menu = None;
        self.pane.menu_open = false;
    }

    // Where the focused pane sits across the window: after the rail, and in the split the primary
    // is always the left half, whichever side holds the keyboard.
    pub(crate) fn pane_box(&self) -> (f32, f32) {
        let rail = if self.rail.shown { self.rail.width } else { 0.0 };
        let rest = (self.window_w - rail).max(0.0);
        match &self.other {
            None => (rail, rest),
            Some(_) => {
                let half = ((rest - 1.0) / 2.0).max(0.0);
                (if self.side == 0 { rail } else { rail + half + 1.0 }, half)
            }
        }
    }

    // ContextMenu's keyCatcher: every key lands here while the menu is up, so none reaches the listing.
    pub(crate) fn menu_key(&mut self, key: &Key) -> Task<Message> {
        let Some(m) = self.menu.as_mut() else { return Task::none() };
        let sub_len = match m.items.get(m.cursor) {
            Some(Item::Row(e)) => e.sub.len(),
            _ => 0,
        };
        match key.as_ref() {
            Key::Named(Named::Escape) if m.flyout => m.flyout = false,
            Key::Named(Named::Escape) => self.close_menu(),
            Key::Named(Named::ArrowDown) if m.flyout => m.sub_cursor = (m.sub_cursor + 1).min(sub_len.saturating_sub(1)),
            Key::Named(Named::ArrowUp) if m.flyout => m.sub_cursor = m.sub_cursor.saturating_sub(1),
            Key::Named(Named::ArrowDown) => m.cursor = menu::step(&m.items, m.cursor, 1),
            Key::Named(Named::ArrowUp) => m.cursor = menu::step(&m.items, m.cursor, -1),
            Key::Named(Named::ArrowLeft) => m.flyout = false,
            Key::Named(Named::ArrowRight) if sub_len > 0 => {
                m.flyout = true;
                m.sub_cursor = 0;
            }
            Key::Named(Named::Enter | Named::Space) if m.flyout => {
                let at = m.sub_cursor;
                return self.menu_choose_sub(at);
            }
            Key::Named(Named::Enter | Named::Space) => {
                let at = m.cursor;
                return self.menu_choose(at);
            }
            _ => {}
        }
        Task::none()
    }

    fn menu_choose(&mut self, index: usize) -> Task<Message> {
        let Some(m) = self.menu.as_mut() else { return Task::none() };
        let Some(Item::Row(e)) = m.items.get(index).cloned() else { return Task::none() };
        if e.disabled {
            return Task::none();
        }
        // A row with a flyout opens it rather than acting; the flyout's own rows are the choice.
        if !e.sub.is_empty() {
            m.cursor = index;
            m.flyout = true;
            m.sub_cursor = 0;
            return Task::none();
        }
        self.menu_act(e.action, e.label)
    }

    fn menu_choose_sub(&mut self, index: usize) -> Task<Message> {
        let Some(m) = self.menu.as_ref() else { return Task::none() };
        let Some(Item::Row(e)) = m.items.get(m.cursor) else { return Task::none() };
        let Some((id, _)) = e.sub.get(index).cloned() else { return Task::none() };
        let action = e.action;
        if m.kind == Kind::File && (m.path != self.pane.path || m.names != self.targets()) {
            self.close_menu();
            return self.show("Selected items changed; reopen the menu.".into(), true);
        }
        self.close_menu();
        match action {
            // Focus.act routes "sort:<key>" to the header click's own function, Sort.column.
            "sort" => {
                let effects = self.pane.sort_column(&id);
                self.run(effects)
            }
            "compress" | "runScript" | "taildrop" | "localsend" => self.activate_row(format!("{action}:{id}")),
            _ => Task::none(),
        }
    }

    fn menu_act(&mut self, action: &'static str, label: &'static str) -> Task<Message> {
        let Some(m) = self.menu.take() else { return Task::none() };
        self.pane.menu_open = false;
        if m.kind == Kind::File && (m.path != self.pane.path || m.names != self.targets()) {
            return self.show("Selected items changed; reopen the menu.".into(), true);
        }
        if !menu::PORTED.contains(&action) {
            return self.say(format!("{label} is not in this build yet."));
        }
        let effects = match action {
            "open" => self.pane.activate(),
            "newFolder" => self.pane.new_folder(),
            "cut" => self.pane.copy(true),
            "copy" => self.pane.copy(false),
            "paste" => self.pane.paste(),
            "duplicate" => self.pane.duplicate(),
            "rename" => self.pane.start_rename(),
            "selectAll" => self.pane.select_all(),
            "trash" => self.pane.trash(),
            "toggleHidden" => self.pane.toggle_hidden(),
            "openTerminal" => return self.open_terminal(),
            "copypath" => return self.copy_path(),
            "addFavourite" => return self.menu_favourite(m.kind),
            "settings" => return self.open_panel(),
            "properties" => return self.open_action(crate::actioncard::Kind::Properties),
            "newFile" => return self.open_action(crate::actioncard::Kind::NewFile),
            "deletePermanently" => return self.open_action(crate::actioncard::Kind::Delete),
            "moveTo" => return self.open_action(crate::actioncard::Kind::MoveTo),
            "copyTo" => return self.open_action(crate::actioncard::Kind::CopyTo),
            "permissions" => return self.open_action(crate::actioncard::Kind::Permissions),
            "openWith" => return self.open_with_row(),
            "extract" | "convert" | "addToShelf" => return self.activate_row(action.to_string()),
            "dropbox" | "sharelink" => return self.activate_to(action.to_string(), Some(self.sends.dropbox_path.clone())),
            _ => vec![],
        };
        self.run(effects)
    }

    // ui/Opener.qml openTerminal: `flea --terminal` in the directory on screen, one at a time.
    pub(crate) fn open_terminal(&mut self) -> Task<Message> {
        if self.opening_terminal {
            return self.say("Still opening the last terminal; try again in a moment.".into());
        }
        self.opening_terminal = true;
        let (bin, dir) = (crate::backend::binary(), self.pane.path.clone());
        Task::perform(blocking(move || std::process::Command::new(&bin).arg("--terminal").arg(&dir).status().ok().and_then(|s| s.code())), |code| {
            Message::Menu(Msg::Terminal(code))
        })
    }

    // Pane.copyPath, the row and Ctrl+B alike: the cursor row's full path, unquoted, through wl-copy,
    // which outlives the window; an empty listing has no row to name and copies nothing.
    pub(crate) fn copy_path(&mut self) -> Task<Message> {
        let Some(row) = self.pane.row_at(self.pane.cursor) else { return Task::none() };
        let path = join(&self.pane.path, &row.n);
        Task::perform(
            blocking(move || std::process::Command::new("wl-copy").arg("--").arg(&path).status().is_ok_and(|s| s.success())),
            |ok| Message::Menu(Msg::Copied(ok)),
        )
    }

    // The file menu's row adds the folder under the cursor, the background's the folder on screen.
    fn menu_favourite(&mut self, kind: Kind) -> Task<Message> {
        let path = match (kind, self.pane.row_at(self.pane.cursor)) {
            (Kind::File, Some(r)) => join(&self.pane.path, &r.n),
            (Kind::File, None) => return Task::none(),
            (Kind::Background, _) => self.pane.path.clone(),
        };
        let label = path.trim_end_matches('/').rsplit('/').next().filter(|l| !l.is_empty()).unwrap_or("/").to_string();
        let op = serde_json::json!({ "op": "add", "record": { "label": label, "path": path } }).to_string();
        let bin = crate::backend::binary();
        Task::perform(blocking(move || crate::dialog::add_favourite(&bin, &op)), |r| Message::Menu(Msg::Favourite(r)))
    }
}

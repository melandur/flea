// The Trash view's half of the shell, ui/TrashHost.qml: opening it over the focused pane, running
// what trash.rs decides, the keys it owns while it is up (every one of them, so nothing reaches the
// listing it covers), its menus, and the Trash count `gio info` reads for the rail's menu.
use crate::menu::Kind;
use crate::trash::{Extend, Out};
use crate::trashview::{Msg, LIST_ID};
use crate::wire::Request;
use crate::{blocking, Flea, Message, ROW_H};
use iced::keyboard::{key::Named, Key, Modifiers};
use iced::widget::{operation, scrollable};
use iced::Task;
use crate::trashsweep;
use crate::trashwatch::Watch;
use crate::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl Flea {
    pub(crate) fn run_trash(&mut self, outs: Vec<Out>) -> Task<Message> {
        let mut tasks = vec![];
        for out in outs {
            tasks.push(match out {
                Out::Send(v) => self.run(vec![crate::pane::Effect::Send(Request::Json(v))]),
                Out::Status(text, alert) => self.show(text, alert),
                // The status bar here holds one line, so a failure's detail rides after its headline.
                Out::Result(headline, detail, error) => {
                    let line = if detail.is_empty() { headline } else { format!("{headline} · {}", detail.replace('\n', " · ")) };
                    Task::batch([self.show(line, error), self.refresh_trash_count(false)])
                }
                Out::Closed => {
                    self.rail.focused = false;
                    Task::none()
                }
                Out::Reveal(row) => self.reveal_trash_row(row),
            });
        }
        Task::batch(tasks)
    }

    // TrashHost.open: the overlay preview, the share browser and the selection all give way to it.
    pub(crate) fn open_trash(&mut self, action: &str) -> Task<Message> {
        if self.trash.confirming() {
            return Task::none();
        }
        let stopped = self.trash.sweep.stop();
        let stopped = self.run_sweep(stopped);
        if self.trash.opened {
            return Task::batch([stopped, self.trash_action(if action.is_empty() { "openTrash" } else { action })]);
        }
        let closed = if self.preview.active {
            let effects = self.preview.close();
            self.run_preview(effects)
        } else {
            Task::none()
        };
        self.share_browser = None;
        self.pane.selection.clear();
        self.close_menu();
        self.rail.focused = false;
        let rows = (self.viewport_h / self.file_h()).ceil() as usize;
        self.trash.window_rows = (rows + 2).clamp(1, crate::trash::MAX_WINDOW);
        self.trash.visible = rows.max(1);
        let outs = self.trash.open(action);
        Task::batch([stopped, closed, self.run_trash(outs)])
    }

    // TrashHost.action: a row from either menu, or the view's own Empty Trash.
    pub(crate) fn trash_action(&mut self, name: &str) -> Task<Message> {
        if self.trash.confirming() {
            return Task::none();
        }
        if !self.trash.opened {
            return self.open_trash(name);
        }
        let outs = match name {
            "restoreTrashSelection" => self.trash.restore(false),
            "deletePermanently" => self.trash.prepare(false),
            "restoreAll" => self.trash.restore(true),
            "emptyTrash" => self.trash.prepare(true),
            _ => vec![],
        };
        self.run_trash(outs)
    }

    pub(crate) fn trash_message(&mut self, msg: Msg) -> Task<Message> {
        let outs = match msg {
            Msg::Row(i) => return self.trash_row_pressed(i),
            // A right click on a row outside the selection selects it first, then raises the menu.
            Msg::RowMenu(i) => {
                let marked = self.trash.row_at(i).is_some_and(|r| self.trash.is_selected(&r.uri));
                let chose = if marked { Task::none() } else { self.trash_row_pressed(i) };
                let y = ROW_H * 3.0 + self.tab_strip_h() + (i.saturating_sub(self.trash.top) + 1) as f32 * self.file_h();
                let total = self.trash.total;
                return Task::batch([chose, self.open_trash_menu(Kind::TrashSelection, None, y, total)]);
            }
            Msg::Back => self.trash.close(),
            Msg::Empty => self.trash.prepare(true),
            Msg::Scrolled(vp) => {
                self.trash.top = (vp.absolute_offset().y / self.file_h()).floor().max(0.0) as usize;
                self.trash.visible = (vp.bounds().height / self.file_h()).ceil().max(1.0) as usize;
                self.trash.ensure_window()
            }
            Msg::Cancel => self.trash.confirm_cancel(),
            Msg::Delete => {
                if let Some(c) = self.trash.confirmation.as_mut() {
                    c.destructive = true;
                }
                self.trash.confirm_activate()
            }
            Msg::Tick => self.trash.check_tick(),
            Msg::Count(n, menu) => {
                // TrashMonitor's failure is said only where it shows: the view, or a badge drawn.
                let said = if n.is_none() && (self.trash.opened || self.prefs.trash_count_shown) {
                    self.show("Could not read the Trash count.".into(), true)
                } else {
                    Task::none()
                };
                self.trash.count = n.or(self.trash.count);
                return Task::batch([said, if menu { self.open_rail_trash_menu() } else { Task::none() }]);
            }
            Msg::Watched(Watch::Changed) => {
                self.trash.settle_gen += 1;
                let gen = self.trash.settle_gen;
                return Task::perform(sleep(Duration::from_millis(120)), move |_| Message::Trash(Msg::Settled(gen)));
            }
            Msg::Watched(Watch::Failed) => {
                return if self.trash.opened || self.prefs.trash_count_shown { self.show("Trash monitoring is unavailable.".into(), true) } else { Task::none() };
            }
            Msg::Settled(gen) if gen == self.trash.settle_gen => {
                let count = self.refresh_trash_count(false);
                let outs = self.trash.source_changed();
                return Task::batch([count, self.run_trash(outs)]);
            }
            Msg::Settled(_) => vec![],
            Msg::SweepDue => {
                let outs = self.trash.sweep.start(self.prefs.trash_auto_empty, self.prefs.trash_swept_on, self.trash.used(), now_ms());
                return self.run_sweep(outs);
            }
        };
        self.run_trash(outs)
    }

    pub(crate) fn trash_reply(&mut self, v: &serde_json::Value) -> Task<Message> {
        // Every reply while a sweep is in flight is the sweep's: the view cannot be open then.
        if self.trash.sweep.sweeping() {
            let outs = self.trash.sweep.receive(v, now_ms());
            return self.run_sweep(outs);
        }
        let outs = self.trash.receive(v);
        self.run_trash(outs)
    }

    fn run_sweep(&mut self, outs: Vec<trashsweep::Out>) -> Task<Message> {
        let mut tasks = vec![];
        for out in outs {
            tasks.push(match out {
                trashsweep::Out::Send(v) => self.run(vec![crate::pane::Effect::Send(Request::Json(v))]),
                trashsweep::Out::Finished(day, changed) => {
                    self.prefs.trash_swept_on = day;
                    let bin = crate::backend::binary();
                    let saved = Task::perform(blocking(move || crate::prefs::save_swept(&bin, day)), Message::Saved);
                    Task::batch([saved, if changed { self.refresh_trash_count(false) } else { Task::none() }])
                }
            });
        }
        Task::batch(tasks)
    }

    // The count is read again before the rail's Trash menu opens, so its rows grey on what is there now.
    pub(crate) fn refresh_trash_count(&mut self, then_menu: bool) -> Task<Message> {
        Task::perform(blocking(crate::trashwatch::count), move |n| Message::Trash(Msg::Count(n, then_menu)))
    }

    fn open_rail_trash_menu(&mut self) -> Task<Message> {
        let Some(at) = self.rail.entries.iter().position(|e| e.kind == crate::rail::Kind::Trash) else { return Task::none() };
        let y = self.rail_row_bottom(at);
        let total = self.trash.count.unwrap_or(0);
        self.open_trash_menu(Kind::Trash, Some(crate::PAD_X), y, total)
    }

    fn reveal_trash_row(&mut self, row: usize) -> Task<Message> {
        let t = &self.trash;
        let visible = t.visible.max(1);
        let top = if row < t.top { row } else if row >= t.top + visible { row + 1 - visible } else { return Task::none() };
        self.trash.top = top;
        operation::scroll_to(LIST_ID, scrollable::AbsoluteOffset { x: 0.0, y: top as f32 * self.file_h() })
    }

    // Every key while the view is up: the rail's own when it holds the keyboard, the confirmation's
    // while it is open, and the view's otherwise; None only when the view is shut.
    pub(crate) fn trash_key(&mut self, key: &Key, modifiers: Modifiers) -> Option<Task<Message>> {
        if !self.trash.opened {
            return None;
        }
        if self.rail.focused {
            if *key == Key::Named(Named::Tab) {
                self.rail.focused = false;
                return Some(Task::none());
            }
            return Some(self.rail_key(key).unwrap_or(Task::none()));
        }
        if let Some(c) = self.trash.confirmation.as_mut() {
            let outs = match key.as_ref() {
                Key::Named(Named::Escape) => self.trash.confirm_cancel(),
                _ if modifiers.control() || modifiers.alt() || modifiers.logo() => vec![],
                Key::Named(Named::Tab) => {
                    c.destructive = !c.destructive;
                    vec![]
                }
                Key::Named(Named::ArrowRight) => {
                    c.destructive = true;
                    vec![]
                }
                Key::Named(Named::ArrowLeft) => {
                    c.destructive = false;
                    vec![]
                }
                Key::Named(Named::Enter | Named::Space) => self.trash.confirm_activate(),
                _ => vec![],
            };
            return Some(self.run_trash(outs));
        }
        let plain = !(modifiers.control() || modifiers.shift() || modifiers.alt() || modifiers.logo());
        let cursor = self.trash.cursor as isize;
        let outs = match key.as_ref() {
            Key::Named(Named::Escape) => self.trash.close(),
            Key::Named(Named::Backspace) if plain => self.trash.close(),
            Key::Named(Named::ArrowDown) => self.trash.choose(cursor + 1, Extend::No),
            Key::Named(Named::ArrowUp) => self.trash.choose(cursor - 1, Extend::No),
            Key::Named(Named::Space) => self.trash.choose(cursor, Extend::Toggle),
            Key::Character("a") if modifiers.control() => self.trash.select_all(),
            Key::Named(Named::Delete) => self.trash.prepare(false),
            Key::Named(Named::Tab) => {
                self.rail.focused = self.rail.shown;
                vec![]
            }
            Key::Named(Named::F5) if plain => self.trash.refresh(),
            Key::Character("m") | Key::Named(Named::ContextMenu) => {
                let kind = if self.trash.selected_count() > 0 { Kind::TrashSelection } else { Kind::Trash };
                let total = self.trash.total;
                return Some(self.open_trash_menu(kind, None, 2.0 * ROW_H + self.tab_strip_h(), total));
            }
            _ => vec![],
        };
        Some(self.run_trash(outs))
    }
}

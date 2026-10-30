// What the pane asks for, carried out: requests to the backend, scrolling, the opener, timers, the
// status line, saving a setting and decoding a thumbnail.
use crate::pane::Effect;
use crate::{backend, blocking, decode, list_id, pane, prefs, sleep, Flea, Message, RENAME_ID};
use iced::widget::{operation, scrollable};
use iced::Task;
use std::time::Duration;

impl Flea {
    pub(crate) fn run(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = vec![];
        for effect in effects {
            match effect {
                Effect::Send(request) => {
                    let sent = self.backend.as_ref().map(|b| b.send(&request));
                    if let Some(Err(e)) = sent {
                        tasks.push(self.say(pane::sentence("backend", &e, "")));
                    }
                }
                Effect::ScrollTo(index) => {
                    let y = if self.grid_mode() { self.grid_geometry().scroll_y(index) } else { index as f32 * self.file_h() };
                    tasks.push(operation::scroll_to(list_id(self.side), scrollable::AbsoluteOffset { x: 0.0, y }));
                }
                // Nav.openCursor: an archive nobody claimed opens Flea's own view and launches nothing.
                Effect::OpenFile(path) if self.archive_routes_here(&path) => tasks.push(self.open_preview()),
                Effect::OpenFile(path) => tasks.push(self.open_file(path)),
                Effect::Preview => tasks.push(self.open_preview()),
                Effect::OpenDir(path) => {
                    let effects = self.pane.open(path, None);
                    tasks.push(self.run(effects));
                }
                Effect::SearchTimer(generation) => tasks.push(Task::perform(
                    sleep(Duration::from_millis(crate::search::LIVE_MS)),
                    {
                        let side = self.side;
                        move |_| Message::SearchLive(side, generation)
                    },
                )),
                Effect::WatchTimer => tasks.push(Task::perform(
                    sleep(Duration::from_millis(pane::WATCH_MS)),
                    {
                        let side = self.side;
                        move |_| Message::WatchFired(side)
                    },
                )),
                Effect::Message(text) => tasks.push(self.show(text, false)),
                Effect::Alert(text) => tasks.push(self.show(text, true)),
                Effect::FocusRename => tasks.push(operation::focus(RENAME_ID)),
                Effect::Settle(generation, ms) => {
                    let side = self.side;
                    tasks.push(Task::perform(sleep(Duration::from_millis(ms)), move |_| Message::Settled(side, generation)));
                }
                Effect::DecodeThumb(path) => tasks.push(self.decode_task(path)),
                Effect::SaveHidden(hidden) => {
                    let bin = backend::binary();
                    tasks.push(Task::perform(blocking(move || prefs::save_hidden(&bin, hidden)), Message::Saved));
                }
            }
        }
        Task::batch(tasks)
    }

    // The scrollable's rows on screen, reported to the pane, out of main.rs whole.
    pub(crate) fn decode_task(&self, path: String) -> Task<Message> {
        let px = if self.grid_mode() { self.grid_geometry().thumb as u32 } else { crate::THUMB_PX };
        let key = path.clone();
        Task::perform(blocking(move || decode(&path, px)), move |h| Message::Decoded(key.clone(), h))
    }

    // ViewState's view: the grid in a window that is not split, as Pane.qml's listOnly makes it.
    pub(crate) fn grid_mode(&self) -> bool {
        self.other.is_none() && self.dual_from == "grid"
    }

    pub(crate) fn report_viewport(&mut self) -> Task<Message> {
        if self.grid_mode() {
            let (first, visible, last) = self.grid_geometry().viewport(self.scroll_y, self.viewport_h);
            let effects = self.pane.viewport(first, visible, last);
            return self.run(effects);
        }
        let first = (self.scroll_y / self.file_h()).floor().max(0.0) as usize;
        // Fully visible rows, which is what the cursor clamp wants; the view draws one more.
        let visible = (self.viewport_h / self.file_h()).floor().max(1.0) as usize;
        // Thumbs.viewport: measured in pixels, so a row straddling the bottom edge is asked for.
        let last = ((self.scroll_y + self.viewport_h) / self.file_h()).ceil().max(1.0) as usize - 1;
        let effects = self.pane.viewport(first, visible, last);
        self.run(effects)
    }

    // List.qml's gate on the walk: the list view only, and only while folder sizes are on or the
    // Items column is drawn; the split's panes are lists that never draw Items.
    pub(crate) fn dirs_follow(&mut self) {
        let wanted = !self.grid_mode() && !self.columns_mode() && (self.prefs.folder_sizes || self.list_cols(false).items);
        self.pane.dirs.wanted = wanted;
        let folder_sizes = self.prefs.folder_sizes;
        if let Some(o) = self.other.as_mut() {
            o.pane.dirs.wanted = folder_sizes;
        }
    }

    pub(crate) fn say(&mut self, text: String) -> Task<Message> {
        self.show(text, false)
    }

    // The status bar's one transient slot, cleared after MESSAGE_MS unless a newer line took it.
    pub(crate) fn show(&mut self, text: String, alert: bool) -> Task<Message> {
        self.message_gen += 1;
        let gen = self.message_gen;
        self.message = Some((text, gen, alert));
        Task::perform(sleep(Duration::from_millis(pane::MESSAGE_MS)), move |_| Message::Expired(gen))
    }

    // ui/Opener.qml: one `flea --open` at a time, and a second press while it runs says so.
    fn open_file(&mut self, path: String) -> Task<Message> {
        if self.opening {
            return self.say("Still opening the last file; try again in a moment.".into());
        }
        self.opening = true;
        let bin = backend::binary();
        Task::perform(
            blocking(move || {
                let code = std::process::Command::new(&bin).arg("--open").arg(&path).status().ok().and_then(|s| s.code());
                (code, path)
            }),
            {
                let side = self.side;
                move |(code, path)| Message::Opened(side, code, path)
            },
        )
    }
}

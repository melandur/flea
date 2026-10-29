// What the pane asks for, carried out: requests to the backend, scrolling, the opener, timers, the
// status line, saving a setting and decoding a thumbnail.
use crate::pane::Effect;
use crate::{backend, blocking, decode, list_id, pane, prefs, sleep, Flea, Message, RENAME_ID, ROW_H};
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
                    let y = index as f32 * ROW_H;
                    tasks.push(operation::scroll_to(list_id(self.side), scrollable::AbsoluteOffset { x: 0.0, y }));
                }
                // Nav.openCursor: an archive nobody claimed opens Flea's own view and launches nothing.
                Effect::OpenFile(path) if self.archive_routes_here(&path) => tasks.push(self.open_preview()),
                Effect::OpenFile(path) => tasks.push(self.open_file(path)),
                Effect::Preview => tasks.push(self.open_preview()),
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
                Effect::DecodeThumb(path) => {
                    let key = path.clone();
                    tasks.push(Task::perform(blocking(move || decode(&path)), move |h| Message::Decoded(key.clone(), h)));
                }
                Effect::SaveHidden(hidden) => {
                    let bin = backend::binary();
                    tasks.push(Task::perform(blocking(move || prefs::save_hidden(&bin, hidden)), Message::Saved));
                }
            }
        }
        Task::batch(tasks)
    }

    // The scrollable's rows on screen, reported to the pane, out of main.rs whole.
    pub(crate) fn report_viewport(&mut self) -> Task<Message> {
        let first = (self.scroll_y / ROW_H).floor().max(0.0) as usize;
        // Fully visible rows, which is what the cursor clamp wants; the view draws one more.
        let visible = (self.viewport_h / ROW_H).floor().max(1.0) as usize;
        // Thumbs.viewport: measured in pixels, so a row straddling the bottom edge is asked for.
        let last = ((self.scroll_y + self.viewport_h) / ROW_H).ceil().max(1.0) as usize - 1;
        let effects = self.pane.viewport(first, visible, last);
        self.run(effects)
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

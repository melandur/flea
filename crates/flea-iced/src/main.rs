// Flea's window in iced: the listing pane and the write operations over the unchanged `flea --backend`.
// Everything the QML tree does that is not here yet says so rather than doing nothing.
mod backend;
mod boot;
mod contrast;
mod devices;
mod delimited;
mod dialog;
mod dialogview;
mod effects;
mod form;
mod format;
#[cfg(test)]
mod live_tests;
mod marks;
mod media;
mod mediashell;
mod mounts;
mod netopen;
mod netshell;
mod network;
mod opreplies;
mod ops;
mod pane;
mod pdf;
mod prefs;
mod preview;
mod previewshell;
mod previewview;
mod process;
mod protocols;
mod rail;
mod railshell;
mod railview;
mod sentence;
#[cfg(test)]
mod testexec;
mod theme;
mod thumbs;
mod watch;
mod keys;
mod kinds;
mod view;
mod wire;

use backend::Backend;
use iced::keyboard;
use iced::widget::{image, scrollable};
use iced::{window, Font, Subscription, Task};
use pane::Pane;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::Palette;
use wire::Request;

// ui/Theme.qml's measured box metrics at Omarchy's default text size: a 37 px row, read off the IPC
// seam as Theme.rowHeight, around a bodySmall of 12 px drawn at a 1.8 line box.
const TEXT_PX: f32 = 13.0;
const ROW_H: f32 = 37.0;
const PAD_X: f32 = 14.0;
// Theme.iconSize, the row's own text line box: rowHeight less twice rowPaddingY, 23 px here.
const ICON_SLOT: f32 = 23.0;
// Theme.markSize, bodySmall * 1.45; Glyph.qml draws the mark at the smaller of this and the slot.
const MARK: f32 = 12.0 * 1.45;
// Decoded at twice the slot so a 1.25 or 2x output scale is not drawn soft; still about 9 kB a row
// where the 256 px cache PNG would be 256 kB, the saving Image.sourceSize bought in QML.
const THUMB_PX: u32 = 46;
// Decoded thumbnails kept beyond the pane's own 240-row map, oldest out first.
const DECODED_CAP: usize = 480;
const SIZE_W: f32 = 90.0;
const DATE_W: f32 = 140.0;
const LIST_ID: &str = "listing";
const RENAME_ID: &str = "rename";
// The second press of a double click lands inside this, iced's own mouse::click interval.
const DOUBLE_TAP: Duration = Duration::from_millis(300);
// Longer than the backend's own 25 s drain limit; see ui/Backend.qml quitDeadline.
const QUIT_DEADLINE: Duration = Duration::from_secs(30);

struct Flea {
    pane: Pane,
    backend: Option<Arc<Backend>>,
    palette: Palette,
    // The status bar's one transient slot: text, generation, and whether it is an error.
    message: Option<(String, u64, bool)>,
    modifiers: keyboard::Modifiers,
    last_tap: Option<(usize, Instant)>,
    message_gen: u64,
    opening: bool,
    quitting: bool,
    viewport_h: f32,
    scroll_y: f32,
    // Keyed by the cache path alone: every thumbed answer decodes again and replaces the entry, so a
    // regenerated thumbnail at the same path cannot show its old frame (AGENTS.md "Icons in the row").
    decoded: HashMap<String, image::Handle>,
    decoded_order: VecDeque<String>,
    rail: railshell::Rail,
    devices: devices::Devices,
    network: network::Network,
    share_browser: Option<netshell::ShareBrowser>,
    dialog: Option<dialog::Dialog>,
    // NetworkDialog's unacknowledged failure: the place and the reason, kept across a close.
    dialog_memory: Option<(String, String)>,
    preview: preview::Preview,
    surfaces: previewshell::Surfaces,
    table_scroll: f32,
    window_w: f32,
    prefs: prefs::Prefs,
    pdf: Option<mediashell::PdfView>,
    playback: mediashell::Playback,
}

#[derive(Debug, Clone)]
enum Message {
    Backend(backend::Event),
    Key(keyboard::Event),
    Scrolled(scrollable::Viewport),
    RowPressed(usize),
    RenameInput(String),
    RenameSubmit,
    Opened(Option<i32>, String),
    WatchFired,
    Settled(u64),
    // The scrollable reports only on a scroll, so the window's own size is what tells the pane how
    // many rows it shows before the first one.
    Resized(iced::Size),
    DeviceListed(Option<String>),
    DeviceRan(devices::Job, Option<i32>),
    DeviceTimer(devices::Timer, u64),
    RailPressed(usize),
    RailMenu(usize),
    RailChose(rail::Action),
    RailMenuClosed,
    NetTick,
    NetListed(process::Ran),
    NetLeg(u64, network::Leg, process::Ran),
    NetAuth(u64, process::Ran),
    FavouriteSaved(Result<Vec<serde_json::Value>, String>),
    Dialog(dialog::Msg),
    PreviewLoaded(String, Result<Result<previewshell::Table, String>, ()>),
    PreviewImage(String, Option<image::Handle>),
    PreviewSettled(u64),
    PreviewClose,
    TableScrolled(f32),
    PdfRendered(Result<pdf::Page, ()>),
    PdfTurn(i32),
    PdfZoom(i32),
    MediaTick,
    MediaToggle,
    MediaMute,
    NetUnmounted(process::Ran),
    RailRenameInput(String),
    RailRenameSubmit,
    ShareChosen(usize),
    Decoded(String, Option<image::Handle>),
    Expired(u64),
    Saved(bool),
    CloseRequested,
    QuitDeadline,
}

fn sleep(d: Duration) -> impl std::future::Future<Output = ()> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    async move {
        let _ = rx.await;
    }
}

// A blocking child off the UI thread, answered once.
fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = T> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("the worker thread answers before it ends") }
}


impl Flea {
    fn say(&mut self, text: String) -> Task<Message> {
        self.show(text, false)
    }

    fn show(&mut self, text: String, alert: bool) -> Task<Message> {
        self.message_gen += 1;
        let gen = self.message_gen;
        self.message = Some((text, gen, alert));
        Task::perform(sleep(Duration::from_millis(pane::MESSAGE_MS)), move |_| Message::Expired(gen))
    }



    fn quit(&mut self) -> Task<Message> {
        if self.quitting {
            return Task::none();
        }
        self.quitting = true;
        // The backend drains as it quits, so a copy in flight removes its own partial destination.
        match self.backend.as_ref().map(|b| b.send(&Request::Quit)) {
            Some(Ok(())) => Task::perform(sleep(QUIT_DEADLINE), |_| Message::QuitDeadline),
            _ => iced::exit(),
        }
    }


    // Every input ends by asking whether the watched re-read it was holding can now be paid.
    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let owed = self.pane.pay_debt();
        Task::batch([task, self.run(owed)])
    }

    fn report_viewport(&mut self) -> Task<Message> {
        let first = (self.scroll_y / ROW_H).floor().max(0.0) as usize;
        // Fully visible rows, which is what the cursor clamp wants; the view draws one more.
        let visible = (self.viewport_h / ROW_H).floor().max(1.0) as usize;
        // Thumbs.viewport: measured in pixels, so a row straddling the bottom edge is asked for.
        let last = ((self.scroll_y + self.viewport_h) / ROW_H).ceil().max(1.0) as usize - 1;
        let effects = self.pane.viewport(first, visible, last);
        self.run(effects)
    }


    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Backend(backend::Event::Reply(reply @ wire::Reply::Meta { .. })) => {
                self.meta_answered(reply);
                Task::none()
            }
            Message::Backend(backend::Event::Reply(reply)) => {
                let effects = self.pane.receive(reply);
                self.run(effects)
            }
            Message::Backend(backend::Event::Unreadable) => {
                self.say("The backend sent a line this build cannot read.".into())
            }
            Message::Backend(backend::Event::Exited(code)) => {
                self.backend = None;
                if self.quitting {
                    return iced::exit();
                }
                let code = code.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into());
                self.say(format!("The backend exited with code {code}."))
            }
            Message::Key(event) => self.key(event),
            Message::Resized(size) => {
                // The header and the status bar are a row each.
                self.viewport_h = (size.height - 2.0 * ROW_H).max(ROW_H);
                self.window_w = size.width;
                self.report_viewport()
            }
            Message::Scrolled(viewport) => {
                self.scroll_y = viewport.absolute_offset().y;
                self.viewport_h = viewport.bounds().height;
                self.report_viewport()
            }
            Message::RowPressed(i) => self.tap(i),
            Message::RenameInput(draft) => {
                self.pane.rename_input(draft);
                Task::none()
            }
            Message::RenameSubmit => {
                let effects = self.pane.commit_rename();
                self.run(effects)
            }
            Message::Opened(code, path) => {
                self.opening = false;
                match code {
                    Some(0) => Task::none(),
                    // 3 is a directory the opener refuses to hand on: navigate instead.
                    Some(3) => {
                        let effects = self.pane.open(path, None);
                        self.run(effects)
                    }
                    _ => self.say("That file could not be opened.".into()),
                }
            }
            Message::FavouriteSaved(result) => self.favourite_saved(result),
            Message::Dialog(msg) => self.dialog_message(msg),
            m @ (Message::DeviceListed(..)
            | Message::DeviceRan(..)
            | Message::DeviceTimer(..)
            | Message::NetAuth(..)
            | Message::NetLeg(..)
            | Message::NetListed(..)
            | Message::NetTick
            | Message::RailChose(..)
            | Message::RailMenu(..)
            | Message::RailPressed(..)
            | Message::NetUnmounted(..)
            | Message::RailMenuClosed
            | Message::RailRenameInput(..)
            | Message::RailRenameSubmit
            | Message::ShareChosen(..)) => self.rail_message(m),
            m @ (Message::PreviewLoaded(..)
            | Message::PreviewImage(..)
            | Message::PreviewSettled(_)
            | Message::PreviewClose
            | Message::PdfRendered(_)
            | Message::PdfTurn(_)
            | Message::PdfZoom(_)
            | Message::MediaTick
            | Message::MediaToggle
            | Message::MediaMute
            | Message::TableScrolled(_)) => self.preview_message(m),
            Message::Settled(generation) => {
                let effects = self.pane.settled(generation);
                self.run(effects)
            }
            Message::Decoded(path, handle) => {
                match handle {
                    Some(h) => {
                        if self.decoded.insert(path.clone(), h).is_none() {
                            self.decoded_order.push_back(path);
                        }
                        while self.decoded_order.len() > DECODED_CAP {
                            if let Some(old) = self.decoded_order.pop_front() {
                                self.decoded.remove(&old);
                            }
                        }
                    }
                    // An evicted or unreadable cache file: the row keeps its mark, as Row.qml's
                    // thumbDrawn falls back on Image.Error.
                    None => {
                        self.decoded.remove(&path);
                    }
                }
                Task::none()
            }
            Message::WatchFired => {
                let effects = self.pane.watch_fired();
                self.run(effects)
            }
            Message::Expired(gen) => {
                if self.message.as_ref().is_some_and(|m| m.1 == gen) {
                    self.message = None;
                }
                Task::none()
            }
            Message::Saved(ok) => {
                if ok {
                    Task::none()
                } else {
                    self.say("That setting could not be saved.".into())
                }
            }
            Message::CloseRequested => self.quit(),
            Message::QuitDeadline => iced::exit(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run(backend::events).map(Message::Backend),
            keyboard::listen().map(Message::Key),
            window::close_requests().map(|_| Message::CloseRequested),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            // A playing file is drawn at its own pace, about 30 frames a second; a paused one is
            // only polled for its bus, and nothing ticks at all without media open.
            match &self.playback.media {
                Some(m) if m.playing => iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::MediaTick),
                Some(_) => iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::MediaTick),
                None => Subscription::none(),
            },
        ])
    }
}

// One cache PNG to an icon-slot image. The path is always the backend's own thumbed answer, never a
// filename, and the decode is a PNG decoder alone; AGENTS.md "Deliberate corners" names the residual.
fn decode(path: &str) -> Option<image::Handle> {
    let img = ::image::open(path).ok()?.thumbnail(THUMB_PX, THUMB_PX).to_rgba8();
    let (w, h) = img.dimensions();
    Some(image::Handle::from_rgba(w, h, img.into_raw()))
}

fn main() -> iced::Result {
    iced::application(boot::boot, Flea::update, Flea::view)
        .title("Flea")
        .subscription(Flea::subscription)
        .exit_on_close_request(false)
        .default_font(Font::DEFAULT)
        .settings(iced::Settings { default_text_size: TEXT_PX.into(), ..Default::default() })
        // Its own app id while both front ends exist, so a compositor rule or a test can tell them apart.
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "com.thisisgm.flea.iced".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

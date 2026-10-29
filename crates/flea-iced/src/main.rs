// Flea's window in iced: the listing pane and the write operations over the unchanged `flea --backend`.
// Everything the QML tree does that is not here yet says so rather than doing nothing.
mod backend;
mod boot;
mod chrome;
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
mod nav;
mod netopen;
mod netshell;
mod network;
mod openrulecard;
mod openrules;
mod opreplies;
mod ops;
mod pane;
mod pathbar;
mod places_rows;
mod pdf;
mod picker;
mod prefs;
mod preview;
mod previewshell;
mod previewview;
mod process;
mod protocols;
mod rail;
mod railshell;
mod railview;
mod search;
mod searchkeys;
mod sentence;
mod settings;
mod settingsapply;
mod settingspanel;
mod settingsview;
mod split;
#[cfg(test)]
mod testexec;
// Every table build.rs generates: the mark paths and families, the filetype colours, the themes.
mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}
mod tasks;
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
use tasks::{blocking, decode, sleep};
use theme::Palette;

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
// A pane's listing scrollable, one id per side, so a scroll lands on the pane it was meant for.
fn list_id(side: u8) -> iced::widget::Id {
    iced::widget::Id::from(format!("listing-{side}"))
}
const RENAME_ID: &str = "rename";
// The second press of a double click lands inside this, iced's own mouse::click interval.
const DOUBLE_TAP: Duration = Duration::from_millis(300);
// Longer than the backend's own 25 s drain limit; see ui/Backend.qml quitDeadline.
const QUIT_DEADLINE: Duration = Duration::from_secs(30);

struct Flea {
    // The focused pane and everything it owns; split.rs keeps the other one in `other`.
    pane: Pane,
    backend: Option<Arc<Backend>>,
    backend_key: u64,
    side: u8,
    other: Option<split::Slot>,
    spawns: u64,
    // The view to go back to on leaving the split, ui.json's dual.from.
    dual_from: String,
    // The children asked to quit and not yet gone: only their exits may end the window, so a dropped
    // second pane's child that exits late is not counted for the primary's drain.
    quit_keys: Vec<u64>,
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
    chrome: chrome::Chrome,
    panel: settingspanel::Panel,
}

#[derive(Debug, Clone)]
enum Message {
    // A backend's line, tagged with the spawn it came from, which names the pane.
    Backend(u64, backend::Event),
    Key(keyboard::Event),
    Scrolled(u8, scrollable::Viewport),
    RowPressed(u8, usize),
    RenameInput(String),
    RenameSubmit,
    Opened(u8, Option<i32>, String),
    WatchFired(u8),
    Settled(u8, u64),
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
    SearchLive(u8, u64),
    SearchPaste(Option<String>),
    SortColumn(u8, &'static str),
    PanelLoaded(Option<serde_json::Value>, Box<settings::Facts>),
    PanelSaved(Option<serde_json::Value>),
    PanelClose,
    PanelSection(&'static str),
    PanelActivate(usize),
    PanelPick(usize, usize),
    PanelRemoveRule(usize),
    Card(openrulecard::CardMsg),
    CrumbPressed(usize),
    CrumbSettle(u64),
    PathInput(String),
    PathSubmit,
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
            Message::Backend(key, event) => self.backend_event(key, event),
            Message::Key(event) => self.key(event),
            Message::Resized(size) => {
                // The chrome, the header and the status bar are a row each.
                self.viewport_h = (size.height - 3.0 * ROW_H).max(ROW_H);
                self.window_w = size.width;
                let other = self.other.as_ref().map(|o| o.side);
                let mine = self.report_viewport();
                let theirs = other.map(|s| self.with_side(s, Flea::report_viewport)).unwrap_or_else(Task::none);
                Task::batch([mine, theirs])
            }
            Message::Scrolled(side, viewport) => self.with_side(side, |f| {
                f.scroll_y = viewport.absolute_offset().y;
                f.viewport_h = viewport.bounds().height;
                f.report_viewport()
            }),
            Message::RowPressed(side, i) => {
                let focused = self.focus_side(side);
                Task::batch([focused, self.tap(i)])
            }
            Message::RenameInput(draft) => {
                self.pane.rename_input(draft);
                Task::none()
            }
            Message::RenameSubmit => {
                let effects = self.pane.commit_rename();
                self.run(effects)
            }
            Message::Opened(side, code, path) => {
                self.opening = false;
                match code {
                    Some(0) => Task::none(),
                    // 3 is a directory the opener refuses to hand on: the pane that asked navigates.
                    Some(3) => self.with_side(side, |f| {
                        let effects = f.pane.open(path, None);
                        f.run(effects)
                    }),
                    _ => self.say("That file could not be opened.".into()),
                }
            }
            Message::FavouriteSaved(result) => self.favourite_saved(result),
            Message::Dialog(msg) => self.dialog_message(msg),
            m @ (Message::PanelLoaded(..)
            | Message::PanelSaved(_)
            | Message::PanelClose
            | Message::PanelSection(_)
            | Message::PanelActivate(_)
            | Message::PanelPick(..)
            | Message::PanelRemoveRule(_)
            | Message::Card(_)) => self.panel_message(m),
            m @ (Message::SearchLive(..)
            | Message::SearchPaste(_)
            | Message::SortColumn(..)
            | Message::CrumbPressed(_)
            | Message::CrumbSettle(_)
            | Message::PathInput(_)
            | Message::PathSubmit) => self.search_message(m),
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
            Message::Settled(side, generation) => self.with_side(side, |f| {
                let effects = f.pane.settled(generation);
                f.run(effects)
            }),
            Message::Decoded(path, handle) => {
                self.decoded_thumb(path, handle);
                Task::none()
            }
            Message::WatchFired(side) => self.with_side(side, |f| {
                let effects = f.pane.watch_fired();
                f.run(effects)
            }),
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
            // One stream per live child, each tagged with the spawn key that names its pane.
            Subscription::batch(
                std::iter::once(self.backend_key)
                    .chain(self.other.as_ref().map(|o| o.key))
                    .map(|key| Subscription::run_with(key, backend::events).with(key).map(|(key, event)| Message::Backend(key, event))),
            ),
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

fn main() -> iced::Result {
    if std::env::args().nth(1).as_deref() == Some("--pick") {
        return picker::launch();
    }
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


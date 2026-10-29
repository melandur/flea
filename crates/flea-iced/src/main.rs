// Flea's window in iced: the listing pane and the write operations over the unchanged `flea --backend`.
// Everything the QML tree does that is not here yet says so rather than doing nothing.
mod actioncard;
mod activation;
mod archives;
mod backend;
mod boot;
mod brands;
mod cardshell;
mod cardview;
mod convertcard;
mod jobkeys;
mod jobshell;
mod jobview;
mod chrome;
mod colpreview;
mod columnmedia;
mod columnpreview;
mod columns;
mod columnsview;
mod contrast;
mod devices;
mod delimited;
mod dialog;
mod dialogview;
mod facts;
mod effects;
mod form;
mod filter;
mod format;
mod grid;
mod gridview;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod card_live_tests;
mod marks;
mod media;
mod menu;
mod message;
mod menufacts;
mod menushell;
mod menuview;
mod metrics;
mod mediashell;
mod mounts;
mod nav;
mod netopen;
mod netshell;
mod network;
mod openwith;
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
mod providers;
mod preview;
mod previewshell;
mod previewview;
mod process;
mod protocols;
mod rail;
mod railshell;
mod railview;
mod scripts;
mod search;
mod sendshell;
mod searchkeys;
mod sentence;
mod settings;
mod settingsapply;
mod settingspanel;
mod settingsview;
mod split;
mod syntax;
mod syntaxlang;
mod syntaxmd;
mod syntaxview;
mod subs;
#[cfg(test)]
mod testexec;
// Every table build.rs generates: the mark paths and families, the filetype colours, the themes.
mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}
mod tasks;
mod theme;
mod trash;
mod trashshell;
mod trashsweep;
mod trashwatch;
mod trashview;
mod thumbs;
mod watch;
mod keys;
mod keysheet;
mod kinds;
mod view;
mod wire;

use backend::Backend;
pub(crate) use message::Message;
use iced::keyboard;
use iced::widget::image;
use iced::{window, Font, Task};
use pane::Pane;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tasks::{blocking, decode, sleep};
use theme::Palette;
pub(crate) use metrics::*;

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
    menu: Option<menushell::ListMenu>,
    opening_terminal: bool,
    // The menu's dialog row on screen, or the deletion it started while that runs; one at a time.
    card: Option<actioncard::Card>,
    card_ids: u64,
    jobs: jobshell::Jobs,
    sends: sendshell::Sends,
    sheet_open: bool,
    columns: columns::Columns,
    // menuOnNeighbour: the folder and row a neighbour's right click is waiting to raise the menu on.
    pending_menu: Option<(String, String)>,
    colprev: columnpreview::ColumnShell,
    trash: trash::Trash,
}


impl Flea {
    // Every input ends by asking whether the watched re-read it was holding can now be paid, and,
    // in the columns view, whether a neighbour column has yet to be asked for.
    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let owed = self.pane.pay_debt();
        let paid = self.run(owed);
        Task::batch([task, paid, self.columns_follow()])
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
            Message::Menu(msg) => self.menu_message(msg),
            Message::Action(msg) => self.action_message(msg),
            Message::Job(msg) => self.job_message(msg),
            Message::Send(msg) => self.send_message(msg),
            Message::Chrome(act) => self.chrome_act(act),
            Message::Column(msg) => self.column_message(msg),
            Message::Trash(msg) => self.trash_message(msg),
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
}

fn main() -> iced::Result {
    if std::env::args().nth(1).as_deref() == Some("--pick") {
        return picker::launch();
    }
    iced::application(boot::boot, Flea::update, Flea::view)
        .title("Flea")
        .subscription(Flea::subscription)
        .default_font(Font::DEFAULT)
        .settings(iced::Settings { default_text_size: TEXT_PX.into(), ..Default::default() })
        // Its own app id while both front ends exist, so a compositor rule or a test can tell them apart.
        .window(window::Settings {
            // Not on the builder: .window() replaces the settings whole, and a close then skips the drain.
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: "com.thisisgm.flea.iced".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}


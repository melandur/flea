// The file chooser window, ui/picker.qml in iced: one portal request, one window, answered through
// the reply file tools/flea-portal reads. The same backend, rows and places the browser window draws
// with, and none of its operations: a chooser that can rename or delete is a file manager wearing a
// dialog's clothes.
use crate::backend::{self, Backend};
use crate::devices::{self, Device};
use crate::picker::{self, recent, RECENT};
use crate::picker::state::{Chooser, Effect};
use crate::process::{self, Ran};
use crate::rail::{self, Activation, Entry, Group};
use crate::tasks::{blocking, sleep};
use crate::theme::Palette;
use crate::wire::{Reply, Request};
use crate::{prefs, DOUBLE_TAP, QUIT_DEADLINE, ROW_H};
use iced::keyboard;
use iced::widget::{operation, scrollable};
use iced::{window, Subscription, Task};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const LIST_ID: &str = "picker-list";
pub const SAVE_ID: &str = "picker-save";
const MESSAGE_LIFE: Duration = Duration::from_millis(4000);
const LSBLK_DEADLINE: Duration = Duration::from_secs(10);

// Where the keyboard is: Tab walks these in this order, the ring ui/picker.qml stepFocus builds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Rail,
    List,
    Save,
    KeepFile,
    UseLocation,
}

// A rail row: Recent, or one of the window's own places.
pub enum Place {
    Recent,
    Entry(Entry),
}

pub struct Picking {
    pub c: Chooser,
    backend: Option<Arc<Backend>>,
    pub palette: Palette,
    reply_path: String,
    home: String,
    user_dirs: String,
    favourites: Vec<serde_json::Value>,
    pub devices: Vec<Device>,
    pub places: Vec<Place>,
    pub rail_cursor: usize,
    pub focus: Focus,
    pub scroll_y: f32,
    pub viewport_h: f32,
    last_tap: Option<(usize, Instant)>,
    quitting: bool,
}

#[derive(Debug, Clone)]
pub enum PMsg {
    Backend(backend::Event),
    Key(keyboard::Event),
    Scrolled(scrollable::Viewport),
    Row(usize),
    Check(usize),
    Place(usize),
    Chip(i64),
    Back,
    Up,
    Accept,
    UseLocation,
    Cancel,
    SaveName(String),
    RecentRead(Vec<String>),
    Devices(Vec<Device>),
    Expire(u64),
    Written(bool),
    CloseRequested,
    QuitDeadline,
}

pub fn boot() -> (Picking, Task<PMsg>) {
    let bin = backend::binary();
    let prefs = prefs::Prefs::load(&bin);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let config = std::env::var("XDG_CONFIG_HOME").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
    let req = picker::request(&std::env::var("FLEA_PICKER").unwrap_or_default());
    let mut p = Picking {
        // 10 rows is the 440 px card's own list height; the first viewport report corrects it.
        c: Chooser::new(req, 10),
        backend: None,
        palette: Palette::load(&prefs.theme_id),
        reply_path: std::env::var("FLEA_PICKER_REPLY").unwrap_or_default(),
        user_dirs: std::fs::read_to_string(format!("{config}/user-dirs.dirs")).unwrap_or_default(),
        home: home.clone(),
        favourites: prefs.favourites.clone(),
        devices: vec![],
        places: vec![],
        rail_cursor: 0,
        focus: Focus::List,
        scroll_y: 0.0,
        viewport_h: 10.0 * ROW_H,
        last_tap: None,
        quitting: false,
    };
    p.rebuild_places();
    let mut tasks = vec![];
    match Backend::spawn(0) {
        Ok(b) => p.backend = Some(b),
        Err(e) => {
            let effects = p.c.failed("backend", &e);
            tasks.push(p.run(effects));
        }
    }
    let effects = p.c.start(&home);
    tasks.push(p.run(effects));
    // The DEVICES group from one bounded lsblk read: rows only, nothing here mounts or ejects.
    let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
    tasks.push(Task::perform(
        blocking(move || match process::run_within(&devices::LSBLK.map(String::from), &[], LSBLK_DEADLINE) {
            Ran::Exited(Some(0), body) => devices::parse(&body, false, &host),
            _ => vec![],
        }),
        PMsg::Devices,
    ));
    (p, Task::batch(tasks))
}

impl Picking {
    pub fn title(&self) -> String {
        picker::title(&self.c.req)
    }

    // ui/PickerPlaces.qml: Recent unless saving, the stored favourites, home and its folders, then
    // the mounted volumes. Network places are the browser window's in this build.
    fn rebuild_places(&mut self) {
        let entries = rail::entries(&rail::Inputs {
            home: &self.home,
            user_dirs: &self.user_dirs,
            favourites: &self.favourites,
            devices: &self.devices,
            shares: &[],
            show_network: false,
            show_trash: false,
            show_devices: true,
        });
        let mut places: Vec<Place> = if self.c.req.saving() { vec![] } else { vec![Place::Recent] };
        for group in [Group::Favourites, Group::Places, Group::Devices] {
            places.extend(entries.iter().filter(|e| e.group() == group).cloned().map(Place::Entry));
        }
        self.places = places;
        self.rail_cursor = self.rail_cursor.min(self.places.len().saturating_sub(1));
    }

    pub(super) fn run(&mut self, effects: Vec<Effect>) -> Task<PMsg> {
        let mut tasks = vec![];
        for e in effects {
            match e {
                Effect::Send(r) => {
                    if let Some(b) = &self.backend {
                        if b.send(&r).is_err() {
                            let more = self.c.failed("backend", "The backend stopped");
                            tasks.push(self.run(more));
                        }
                    }
                }
                Effect::ReadRecent => {
                    let data = std::env::var("XDG_DATA_HOME").unwrap_or_default();
                    let file = recent::history_path(&data, &self.home);
                    tasks.push(Task::perform(blocking(move || recent::read(std::path::Path::new(&file))), PMsg::RecentRead));
                }
                Effect::Finish(text) => tasks.push(self.write_reply(text)),
                Effect::FocusCancel => {
                    self.focus = Focus::KeepFile;
                    tasks.push(operation::focus(iced::widget::Id::new("none")));
                }
                Effect::Expire(g) => tasks.push(Task::perform(sleep(MESSAGE_LIFE), move |_| PMsg::Expire(g))),
            }
        }
        tasks.push(self.reveal());
        Task::batch(tasks)
    }

    // The one write out of this process, a temp beside the reply and a rename, so tools/flea-portal
    // only ever reads a whole answer. The backend is told to quit once it has landed.
    fn write_reply(&self, text: String) -> Task<PMsg> {
        let dest = self.reply_path.clone();
        Task::perform(
            blocking(move || {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                if dest.is_empty() {
                    return false;
                }
                let temp = format!("{dest}.{}.tmp", std::process::id());
                let _ = std::fs::remove_file(&temp);
                let wrote = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)
                    .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|_| f.sync_all()))
                    .and_then(|_| std::fs::rename(&temp, &dest));
                if wrote.is_err() {
                    let _ = std::fs::remove_file(&temp);
                }
                wrote.is_ok()
            }),
            PMsg::Written,
        )
    }

    // Keep the cursor row on screen after a key moved it.
    fn reveal(&self) -> Task<PMsg> {
        let top = self.c.cursor as f32 * ROW_H;
        let y = if top < self.scroll_y {
            top
        } else if top + ROW_H > self.scroll_y + self.viewport_h {
            top + ROW_H - self.viewport_h
        } else {
            return Task::none();
        };
        operation::scroll_to(LIST_ID, scrollable::AbsoluteOffset { x: 0.0, y: y.max(0.0) })
    }

    pub(super) fn choose_place(&mut self, index: usize) -> Task<PMsg> {
        if self.c.submitting() || self.c.backend_unavailable {
            return Task::none();
        }
        let effects = match self.places.get(index) {
            Some(Place::Recent) => self.c.open(RECENT),
            Some(Place::Entry(e)) => match rail::activate(e) {
                Activation::Open(path) => self.c.open(&path),
                Activation::Device(i) => match self.devices.get(i) {
                    Some(d) if d.mounted && !d.path.is_empty() => {
                        let path = d.path.clone();
                        self.c.open(&path)
                    }
                    _ => self.c.say("This volume is not mounted; mount it from the Flea window first.", false),
                },
                Activation::Remote(..) | Activation::Share(_) => {
                    self.c.say("Network places open from the Flea window in this build; pick from the mounted folder.", false)
                }
                Activation::Say(text, error) => self.c.say(&text, error),
                // The chooser's rail lists no Trash, which is not a place a file is picked from.
                Activation::Trash => self.c.say("The Trash opens from the Flea window.", false),
            },
            None => vec![],
        };
        self.focus = Focus::List;
        self.run(effects)
    }

    fn quit(&mut self) -> Task<PMsg> {
        self.quitting = true;
        match &self.backend {
            Some(b) if b.send(&Request::Quit).is_ok() => Task::perform(sleep(QUIT_DEADLINE), |_| PMsg::QuitDeadline),
            _ => iced::exit(),
        }
    }

    pub fn update(&mut self, m: PMsg) -> Task<PMsg> {
        match m {
            PMsg::Backend(event) => self.backend_event(event),
            PMsg::Key(event) => self.key(event),
            PMsg::Scrolled(vp) => {
                self.scroll_y = vp.absolute_offset().y;
                self.viewport_h = vp.bounds().height;
                self.c.visible_rows = (self.viewport_h / ROW_H).ceil().max(1.0) as usize;
                let first = (self.scroll_y / ROW_H).floor().max(0.0) as usize;
                let drift = self.c.drifted(first).into_iter().collect();
                self.run(drift)
            }
            PMsg::Row(i) => {
                // Tap.js's rule with the chooser's one difference: the second tap on a file marks it
                // and never sends, because a double click that hands a file over is a send nobody asked for.
                let again = self.last_tap.is_some_and(|(at, when)| at == i && when.elapsed() < DOUBLE_TAP);
                self.last_tap = if again { None } else { Some((i, Instant::now())) };
                self.c.cursor = i;
                self.focus = Focus::List;
                let row = self.c.row_for(i).cloned();
                let effects = match row {
                    Some(r) if again && picker::is_directory(&r) => self.c.open(&picker::row_path(&self.c.path, &r.n)),
                    Some(r) if again && self.c.markable(&r) => self.c.toggle_mark(i),
                    _ => vec![],
                };
                self.run(effects)
            }
            PMsg::Check(i) => {
                self.c.cursor = i;
                let effects = self.c.toggle_mark(i);
                self.run(effects)
            }
            PMsg::Place(i) => {
                self.rail_cursor = i;
                self.choose_place(i)
            }
            PMsg::Chip(i) => {
                let effects = self.c.set_filter(i);
                self.run(effects)
            }
            PMsg::Back => {
                let effects = self.c.go_back();
                self.run(effects)
            }
            PMsg::Up => {
                let effects = self.c.go_up();
                self.run(effects)
            }
            PMsg::Accept => {
                let effects = self.c.accept(false);
                self.run(effects)
            }
            PMsg::UseLocation => {
                let effects = self.c.accept(true);
                self.run(effects)
            }
            PMsg::Cancel => {
                let effects = self.c.cancel();
                self.run(effects)
            }
            PMsg::SaveName(text) => {
                self.focus = Focus::Save;
                let effects = self.c.set_save_name(text);
                self.run(effects)
            }
            PMsg::RecentRead(paths) => {
                let effects = self.c.recent_read(paths);
                self.run(effects)
            }
            PMsg::Devices(list) => {
                self.devices = list;
                self.rebuild_places();
                Task::none()
            }
            PMsg::Expire(g) => {
                self.c.expire(g);
                Task::none()
            }
            PMsg::Written(ok) => {
                if !ok {
                    eprintln!("flea-iced: the portal reply could not be written, so the request fails rather than reporting a refusal");
                }
                self.quit()
            }
            // Closing the window is a refusal, the board's own rule; after an answer it is teardown.
            PMsg::CloseRequested if self.c.answered => Task::none(),
            PMsg::CloseRequested => self.update(PMsg::Cancel),
            PMsg::QuitDeadline => iced::exit(),
        }
    }

    fn backend_event(&mut self, event: backend::Event) -> Task<PMsg> {
        let effects = match event {
            backend::Event::Exited(_) if self.quitting => return iced::exit(),
            backend::Event::Exited(_) => self.c.failed("backend", "The backend stopped"),
            backend::Event::Unreadable => self.c.say("The backend sent a line this build cannot read.", true),
            backend::Event::Reply(r) => match r {
                Reply::Listed { n } => {
                    self.c.listed(n);
                    vec![]
                }
                Reply::Rows { start, rows, .. } => {
                    self.c.rows_arrived(start, rows);
                    vec![]
                }
                Reply::Changed { path } => self.c.changed(&path),
                Reply::Error { r#where, msg, .. } => self.c.failed(&r#where, &msg),
                Reply::Picker(answer) => self.c.checked(answer),
                _ => vec![],
            },
        };
        self.run(effects)
    }

    pub fn subscription(&self) -> Subscription<PMsg> {
        Subscription::batch([
            Subscription::run_with(0u64, backend::events).map(PMsg::Backend),
            keyboard::listen().map(PMsg::Key),
            iced::event::listen_with(crate::keys::captured_escape).map(PMsg::Key),
            window::close_requests().map(|_| PMsg::CloseRequested),
        ])
    }
}


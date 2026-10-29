// The rail's half of the shell: its state, the device service's processes and timers, and what
// its keys do. ui/js/Focus.js decides between the rail and the listing; RailKeys.js is the rest.
use crate::devices::{self, Job, Timer};
use crate::rail::{self, Action, Activation, Entry};
use crate::{blocking, sleep, Flea, Message};
use iced::keyboard::{key::Named, Key};
use iced::Task;
use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub entry: usize,
    pub actions: Vec<Action>,
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct Rail {
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub focused: bool,
    pub shown: bool,
    pub width: f32,
    pub menu: Option<Menu>,
    // The inline editor on a share's row: which entry, and the draft.
    pub renaming: Option<(usize, String)>,
    pub show_network: bool,
    pub home: String,
    pub user_dirs: String,
    pub favourites: Vec<Value>,
    pub show_trash: bool,
    pub show_devices: bool,
}

// One argv under a deadline: Ok(stdout) on exit 0, Err otherwise, and a child still running at the
// deadline is killed. lsblk is the only caller; a wedged one froze the rail before this existed.
fn output_within(argv: &[&str], deadline: Duration) -> Option<String> {
    let mut child = Command::new(argv[0]).args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut s).ok().map(|_| s)
    });
    let end = Instant::now() + deadline;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return reader.join().ok().flatten().filter(|_| status.success()),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn status(argv: Vec<String>) -> Option<i32> {
    Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().ok()?.code()
}

impl Flea {
    pub(crate) fn rebuild_rail(&mut self) {
        let r = &self.rail;
        self.rail.entries = rail::entries(&rail::Inputs {
            home: &r.home,
            user_dirs: &r.user_dirs,
            favourites: &r.favourites,
            devices: &self.devices.entries,
            shares: &self.network.entries,
            show_network: r.show_network,
            show_trash: r.show_trash,
            show_devices: r.show_devices,
        });
        self.rail.cursor = self.rail.cursor.min(self.rail.entries.len().saturating_sub(1));
        // The poll can renumber the rows under an open menu, so a menu whose row moved closes.
        if self.rail.menu.as_ref().is_some_and(|m| m.entry >= self.rail.entries.len()) {
            self.rail.menu = None;
        }
        // A poll that rebinds the rows would leave a rename standing on a different share.
        if self.rail.renaming.as_ref().is_some_and(|(i, _)| *i >= self.rail.entries.len()) {
            self.rail.renaming = None;
        }
    }

    pub(crate) fn run_devices(&mut self, effects: Vec<devices::Effect>) -> Task<Message> {
        let mut tasks = vec![];
        for e in effects {
            tasks.push(match e {
                devices::Effect::List => Task::perform(
                    blocking(|| output_within(&devices::LSBLK, Duration::from_millis(devices::LIST_TIMEOUT_MS))),
                    Message::DeviceListed,
                ),
                devices::Effect::Run(job, argv) => Task::perform(blocking(move || status(argv)), move |code| Message::DeviceRan(job, code)),
                devices::Effect::Timer(which, generation, ms) => {
                    Task::perform(sleep(Duration::from_millis(ms)), move |_| Message::DeviceTimer(which, generation))
                }
                devices::Effect::Message(text, alert) => self.show(text, alert),
                devices::Effect::Opened(path) => {
                    let effects = self.pane.open(path, None);
                    self.run(effects)
                }
            });
        }
        Task::batch(tasks)
    }

    pub(crate) fn device_listed(&mut self, stdout: Option<String>) -> Task<Message> {
        let effects = self.devices.listed(stdout);
        self.rebuild_rail();
        self.run_devices(effects)
    }

    pub(crate) fn device_ran(&mut self, job: Job, code: Option<i32>) -> Task<Message> {
        let effects = match job {
            Job::Mount => self.devices.mounted(code),
            Job::Unmount => self.devices.unmounted(code),
            Job::Eject => self.devices.ejected(code),
        };
        self.run_devices(effects)
    }

    pub(crate) fn device_timer(&mut self, which: Timer, generation: u64) -> Task<Message> {
        let effects = self.devices.timer_fired(which, generation);
        self.run_devices(effects)
    }

    // A tap on a rail row activates it, as the rail's one-tap rule says; it does not take the keyboard.
    pub(crate) fn rail_activate(&mut self, index: usize, into: bool) -> Task<Message> {
        self.rail.menu = None;
        self.rail.cursor = index;
        let Some(entry) = self.rail.entries.get(index) else { return Task::none() };
        let task = match rail::activate(entry) {
            Activation::Open(path) => {
                let effects = self.pane.open(path, None);
                self.run(effects)
            }
            Activation::Device(i) => {
                let effects = self.devices.activate(i);
                self.run_devices(effects)
            }
            Activation::Share(i) => self.open_share(i),
            Activation::Remote(uri, label) => self.open_remote(&uri, &label),
            Activation::Say(text, alert) => {
                // Nothing opened, so nothing is handed the keyboard.
                return self.show(text, alert);
            }
        };
        // Right in the rail opens and takes the keyboard into what it opened; Enter stays.
        if into {
            self.rail.focused = false;
        }
        task
    }

    // Mounts.raiseMenu: a row with nothing to release says so rather than opening an empty menu.
    pub(crate) fn rail_open_menu(&mut self, index: usize) -> Task<Message> {
        self.rail.cursor = index;
        let Some(entry) = self.rail.entries.get(index) else { return Task::none() };
        let actions = match entry.kind {
            rail::Kind::Device(i) => self.devices.entries.get(i).map(rail::menu).unwrap_or_default(),
            rail::Kind::Share(i) => self.network.entries.get(i).map(rail::share_menu).unwrap_or_default(),
            _ => vec![],
        };
        if actions.is_empty() {
            return self.say(format!("{} has nothing to eject or unmount.", entry.label));
        }
        self.rail.menu = Some(Menu { entry: index, actions, selected: 0 });
        Task::none()
    }

    pub(crate) fn rail_choose(&mut self, action: Action) -> Task<Message> {
        let Some(menu) = self.rail.menu.take() else { return Task::none() };
        let kind = self.rail.entries.get(menu.entry).map(|e| e.kind.clone());
        if let Some(rail::Kind::Share(i)) = kind {
            return match action {
                Action::Unmount => {
                    let effects = self.network.unmount(i);
                    self.run_network(effects)
                }
                Action::Edit => self.edit_share(i),
                Action::Rename => self.start_rail_rename(menu.entry),
                Action::Remove => self.forget_share(i),
                _ => Task::none(),
            };
        }
        let Some(rail::Kind::Device(i)) = kind else { return Task::none() };
        let effects = match action {
            Action::Eject => self.devices.eject(i),
            Action::UnmountVolume => self.devices.unmount(i),
            Action::MountVolume | Action::OpenVolume => self.devices.activate(i),
            Action::Unmount | Action::Edit | Action::Rename | Action::Remove => vec![],
        };
        self.run_devices(effects)
    }

    // Ctrl+G: the rail shows or hides, and the choice is saved; a hidden rail cannot hold the keyboard.
    pub(crate) fn toggle_rail(&mut self) -> Task<Message> {
        self.rail.shown = !self.rail.shown;
        if !self.rail.shown {
            self.rail.focused = false;
            self.rail.menu = None;
        }
        let (bin, shown) = (crate::backend::binary(), self.rail.shown);
        Task::perform(blocking(move || crate::prefs::save_rail(&bin, shown)), Message::Saved)
    }

    // The rail's keys while it holds the keyboard; None hands the key back to the shell.
    pub(crate) fn rail_key(&mut self, key: &Key) -> Option<Task<Message>> {
        if let Some(menu) = self.rail.menu.as_mut() {
            let last = menu.actions.len() - 1;
            match key.as_ref() {
                Key::Named(Named::ArrowDown) | Key::Character("j") => menu.selected = (menu.selected + 1).min(last),
                Key::Named(Named::ArrowUp) | Key::Character("k") => menu.selected = menu.selected.saturating_sub(1),
                Key::Named(Named::Enter | Named::Space) => {
                    let action = menu.actions[menu.selected];
                    return Some(self.rail_choose(action));
                }
                Key::Named(Named::Escape) => self.rail.menu = None,
                _ => {}
            }
            return Some(Task::none());
        }
        if !self.rail.focused {
            return None;
        }
        let last = self.rail.entries.len().saturating_sub(1);
        Some(match key.as_ref() {
            Key::Named(Named::ArrowDown) => {
                self.rail.cursor = (self.rail.cursor + 1).min(last);
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                self.rail.cursor = self.rail.cursor.saturating_sub(1);
                Task::none()
            }
            Key::Named(Named::Enter) => self.rail_activate(self.rail.cursor, false),
            // An empty rail keeps the keyboard: there was nothing to open.
            Key::Named(Named::ArrowRight) if !self.rail.entries.is_empty() => self.rail_activate(self.rail.cursor, true),
            Key::Named(Named::Escape) => {
                self.rail.focused = false;
                Task::none()
            }
            Key::Character("m") | Key::Named(Named::ContextMenu) => self.rail_open_menu(self.rail.cursor),
            Key::Named(Named::F10) if self.modifiers.shift() => self.rail_open_menu(self.rail.cursor),
            // addNetwork: the dialog is reached from the rail, by its "+" or by this key.
            Key::Character("a") if self.rail.show_network => self.dialog_open(),
            _ => Task::none(),
        })
    }
}

// The NETWORK group's half of the shell: running the service's gio legs under their deadlines,
// reading and rewriting the bookmarks file, the rail's inline rename and the share browser.
//
// Every bookmark edit reads the file itself just before it writes, and refuses when that read fails
// for any reason but absence: a failed read used to empty the text and the write that followed
// replaced the operator's saved places with one line (AGENTS.md "A failed FileView read").
use crate::mounts;
use crate::network::{self, Leg};
use crate::process::{run_within, GIO_ENV};
use crate::{blocking, sleep, Flea, Message};
use iced::Task;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct ShareBrowser {
    pub base: String,
    pub label: String,
    pub names: Vec<String>,
    pub cursor: usize,
}

fn bookmarks_path() -> PathBuf {
    // ui/NetworkPlaces.qml names $HOME/.config/gtk-3.0/bookmarks, as GTK itself does.
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/gtk-3.0/bookmarks")
}

// Ok("") for a file that does not exist yet, which is the one read that is legitimately empty.
pub fn read_bookmarks() -> Result<String, ()> {
    match std::fs::read_to_string(bookmarks_path()) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(_) => Err(()),
    }
}

// A temp beside the file, created exclusively, then renamed over it: never a truncation.
fn write_bookmarks(body: &str) -> bool {
    let path = bookmarks_path();
    let Some(dir) = path.parent() else { return false };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let temp = dir.join(format!(".bookmarks.flea-{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .and_then(|mut f| f.write_all(body.as_bytes()).and_then(|_| f.sync_all()));
    if written.is_err() || std::fs::rename(&temp, &path).is_err() {
        let _ = std::fs::remove_file(&temp);
        return false;
    }
    true
}

impl Flea {
    pub(crate) fn run_network(&mut self, effects: Vec<network::Effect>) -> Task<Message> {
        let mut tasks = vec![];
        for e in effects {
            tasks.push(match e {
                network::Effect::List => Task::perform(
                    blocking(|| run_within(&["gio".into(), "mount".into(), "-li".into()], &GIO_ENV, Duration::from_millis(network::LIST_TIMEOUT_MS))),
                    Message::NetListed,
                ),
                network::Effect::Leg(leg, argv) => Task::perform(
                    blocking(move || run_within(&argv, &GIO_ENV, Duration::from_millis(network::LEG_TIMEOUT_MS))),
                    move |ran| Message::NetLeg(leg, ran),
                ),
                // An unmount is bounded too, which QML's was not; a hang reads as the failure it is.
                network::Effect::Unmount(argv) => Task::perform(
                    blocking(move || run_within(&argv, &GIO_ENV, Duration::from_millis(network::LEG_TIMEOUT_MS))),
                    Message::NetUnmounted,
                ),
                network::Effect::Message(text, alert) => self.show(text, alert),
                network::Effect::Opened(path) => {
                    // The browser stays up while a slow share mounts and goes once there is a folder.
                    self.share_browser = None;
                    let effects = self.pane.open(path, None);
                    self.run(effects)
                }
                network::Effect::SharesListed { base, label, names } => {
                    self.share_browser = Some(ShareBrowser { base, label, names, cursor: 0 });
                    Task::none()
                }
            });
        }
        Task::batch(tasks)
    }

    // The network poll's own tick, which also re-reads the bookmarks a file manager elsewhere wrote.
    pub(crate) fn net_tick(&mut self) -> Task<Message> {
        if let Ok(text) = read_bookmarks() {
            self.network.set_bookmarks(text);
            self.rebuild_rail();
        }
        let effects = self.network.poll();
        let polled = self.run_network(effects);
        let next = Task::perform(sleep(Duration::from_millis(network::POLL_MS)), |_| Message::NetTick);
        Task::batch([polled, next])
    }

    pub(crate) fn net_listed(&mut self, ran: crate::process::Ran) -> Task<Message> {
        let effects = self.network.listed(ran);
        self.rebuild_rail();
        self.run_network(effects)
    }

    pub(crate) fn net_leg(&mut self, leg: Leg, ran: crate::process::Ran) -> Task<Message> {
        let effects = self.network.leg_done(leg, ran);
        self.run_network(effects)
    }

    pub(crate) fn net_unmounted(&mut self, ran: crate::process::Ran) -> Task<Message> {
        let effects = self.network.unmounted(ran);
        self.run_network(effects)
    }

    pub(crate) fn open_share(&mut self, index: usize) -> Task<Message> {
        let Some(s) = self.network.entries.get(index).cloned() else { return Task::none() };
        let effects = self.network.open(&s.uri, s.mounted, &s.label);
        self.run_network(effects)
    }

    // NetworkPlaces.forget, with its three answers: not saved, forgotten, forgotten but still mounted.
    pub(crate) fn forget_share(&mut self, index: usize) -> Task<Message> {
        let Some(s) = self.network.entries.get(index).cloned() else { return Task::none() };
        let Ok(current) = read_bookmarks() else {
            return self.show("Saved places could not be read, so nothing was forgotten.".into(), true);
        };
        let next = mounts::remove_bookmark(&current, &s.uri);
        if next == current {
            return self.say(format!("{} is not a saved place, and stays on the rail until it is unmounted.", s.label));
        }
        if !write_bookmarks(&next) {
            return self.show("Saved places could not be written.".into(), true);
        }
        self.network.set_bookmarks(next);
        self.rebuild_rail();
        if s.mounted {
            self.say(format!("{} is forgotten, and stays on the rail until it is unmounted.", s.label))
        } else {
            self.say(format!("{} is forgotten.", s.label))
        }
    }

    pub(crate) fn start_rail_rename(&mut self, entry: usize) -> Task<Message> {
        let Some(e) = self.rail.entries.get(entry) else { return Task::none() };
        self.rail.renaming = Some((entry, e.label.clone()));
        iced::widget::operation::focus(crate::railview::RAIL_RENAME_ID)
    }

    pub(crate) fn commit_rail_rename(&mut self) -> Task<Message> {
        let Some((entry, name)) = self.rail.renaming.take() else { return Task::none() };
        let Some(crate::rail::Kind::Share(i)) = self.rail.entries.get(entry).map(|e| e.kind.clone()) else { return Task::none() };
        let Some(s) = self.network.entries.get(i).cloned() else { return Task::none() };
        if name.trim().is_empty() {
            return Task::none();
        }
        let Ok(current) = read_bookmarks() else {
            return self.show("Saved places could not be read, so the new name was not saved.".into(), true);
        };
        let next = mounts::relabel(&current, &s.uri, &name);
        if !write_bookmarks(&next) {
            return self.show("Saved places could not be written.".into(), true);
        }
        self.network.set_bookmarks(next);
        self.rebuild_rail();
        Task::none()
    }

    // ui/js/Focus.js shareBrowserAct: the browser takes the listing's keys while it is up.
    pub(crate) fn share_key(&mut self, key: &iced::keyboard::Key) -> Option<Task<Message>> {
        use iced::keyboard::{key::Named, Key};
        let b = self.share_browser.as_mut()?;
        let last = b.names.len().saturating_sub(1);
        Some(match key.as_ref() {
            Key::Named(Named::ArrowDown) => {
                b.cursor = (b.cursor + 1).min(last);
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                b.cursor = b.cursor.saturating_sub(1);
                Task::none()
            }
            Key::Named(Named::Enter | Named::ArrowRight) => self.share_chosen(self.share_browser.as_ref().unwrap().cursor),
            Key::Named(Named::Escape) => {
                self.share_browser = None;
                Task::none()
            }
            _ => Task::none(),
        })
    }

    // ShareBrowser.activateCursor: the share is the base plus its name, opened like any other place.
    pub(crate) fn share_chosen(&mut self, index: usize) -> Task<Message> {
        let Some(b) = self.share_browser.as_mut() else { return Task::none() };
        b.cursor = index;
        let Some(name) = b.names.get(index).cloned() else { return Task::none() };
        let base = if b.base.ends_with('/') { b.base.clone() } else { format!("{}/", b.base) };
        let effects = self.network.open(&format!("{base}{name}/"), false, &name);
        self.run_network(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The bookmarks path follows HOME, so a test points HOME at a scratch directory of its own.
    #[test]
    fn a_write_replaces_the_file_and_an_unreadable_one_is_refused_rather_than_emptied() {
        let root = std::env::temp_dir().join(format!("flea-iced-marks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // SAFETY: this is the only test in the crate that reads HOME, and it restores it.
        let saved = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", &root) };
        assert_eq!(read_bookmarks(), Ok(String::new()));
        assert!(write_bookmarks("smb://h/s S\n"));
        assert_eq!(read_bookmarks(), Ok("smb://h/s S\n".into()));
        // chmod 000 is one way in to the defect AGENTS.md records; a read that fails is an error.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bookmarks_path(), std::fs::Permissions::from_mode(0o000)).unwrap();
        let refused = read_bookmarks();
        std::fs::set_permissions(bookmarks_path(), std::fs::Permissions::from_mode(0o600)).unwrap();
        match saved {
            Some(h) => unsafe { std::env::set_var("HOME", h) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(refused, Err(()));
    }
}

// The rail: ui/Sidebar.qml's groups, ui/js/Places.js for what PLACES and FAVORITES hold, and
// ui/js/RailKeys.js plus Mounts.railMenu for what the keyboard and the menu do with a row.
use crate::devices::Device;
use crate::network::Share;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Home,
    Trash,
    // A favourite carries its record's own index and the reason it cannot be opened, if any.
    Favourite { index: usize, error: String },
    Share(usize),
    Device(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub label: String,
    pub path: String,
    pub glyph: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Group {
    Places,
    Favourites,
    Network,
    Devices,
}

impl Entry {
    pub fn group(&self) -> Group {
        match self.kind {
            Kind::Home | Kind::Trash => Group::Places,
            Kind::Favourite { .. } => Group::Favourites,
            Kind::Share(_) => Group::Network,
            Kind::Device(_) => Group::Devices,
        }
    }
}

// Icons.js SIDEBAR_GLYPHS.
fn sidebar_glyph(label: &str) -> &'static str {
    match label {
        "Home" => "house",
        "Downloads" => "download",
        "Documents" => "file-text",
        "Pictures" => "image",
        "Videos" => "film",
        "Music" => "music",
        "Projects" => "folder-git-2",
        _ => "folder",
    }
}

fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// Places.userDirs: ~/.config/user-dirs.dirs, $HOME expanded, home itself and empty paths skipped.
pub fn user_dirs(body: &str, home: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for line in body.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("XDG_") else { continue };
        let Some((key, value)) = rest.split_once('=') else { continue };
        if !key.ends_with("_DIR") || !key[..key.len() - 4].chars().all(|c| c.is_ascii_uppercase()) {
            continue;
        }
        let value = value.trim();
        let Some(inner) = value.strip_prefix('"').and_then(|v| v.split_once('"')).map(|(v, _)| v) else { continue };
        // String.replace with a string pattern replaces the first occurrence only, as Places.js does.
        let path = inner.replacen("$HOME", home, 1);
        let path = path.trim_end_matches('/');
        if path.is_empty() || path == home {
            continue;
        }
        out.push((path.to_string(), leaf(path).to_string()));
    }
    out
}

// Places.recordError: why a stored favourite cannot be opened, "" when it can.
fn record_error(record: &Value) -> String {
    let Some(obj) = record.as_object() else { return "invalid favorite record".into() };
    let label = obj.get("label").and_then(Value::as_str);
    if label.is_none_or(|l| l.trim().is_empty()) {
        return "label needs visible text".into();
    }
    let Some(path) = obj.get("path").and_then(Value::as_str).filter(|p| !p.chars().any(|c| c.is_control())) else {
        return "invalid path".into();
    };
    if path.starts_with('/') || path == "~" || path.starts_with("~/") {
        return String::new();
    }
    let schemes = ["smb://", "sftp://", "ftp://", "dav://", "davs://", "afp://", "nfs://", "file://"];
    if schemes.iter().any(|s| path.starts_with(s) && path.len() > s.len()) {
        return String::new();
    }
    "path must be absolute, ~/ relative, or a supported URI".into()
}

pub struct Inputs<'a> {
    pub home: &'a str,
    pub user_dirs: &'a str,
    pub favourites: &'a [Value],
    pub devices: &'a [Device],
    pub shares: &'a [Share],
    pub show_network: bool,
    pub show_trash: bool,
    pub show_devices: bool,
}

pub fn entries(i: &Inputs) -> Vec<Entry> {
    let mut out = vec![Entry { kind: Kind::Home, label: "Home".into(), path: i.home.into(), glyph: "house" }];
    for (path, label) in user_dirs(i.user_dirs, i.home) {
        out.push(Entry { kind: Kind::Home, glyph: sidebar_glyph(&label), label, path });
    }
    if i.show_trash {
        out.push(Entry { kind: Kind::Trash, label: "Trash".into(), path: "trash:///".into(), glyph: "trash" });
    }
    for (index, record) in i.favourites.iter().enumerate() {
        let path = record.get("path").and_then(Value::as_str).unwrap_or("");
        let label = record.get("label").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| record.to_string());
        let resolved = if path == "~" {
            i.home.to_string()
        } else if let Some(rest) = path.strip_prefix("~/") {
            format!("{}/{rest}", i.home)
        } else {
            path.to_string()
        };
        out.push(Entry {
            kind: Kind::Favourite { index, error: record_error(record) },
            label: if label.is_empty() { "Invalid favorite".into() } else { label },
            glyph: if path.contains("://") { "network" } else { "folder" },
            path: resolved,
        });
    }
    if i.show_network {
        for (index, s) in i.shares.iter().enumerate() {
            out.push(Entry { kind: Kind::Share(index), label: s.label.clone(), path: String::new(), glyph: "server" });
        }
    }
    if i.show_devices {
        for (index, d) in i.devices.iter().enumerate() {
            out.push(Entry { kind: Kind::Device(index), label: d.label.clone(), path: d.path.clone(), glyph: "drive" });
        }
    }
    out
}

// What activating a row asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Activation {
    Open(String),
    Device(usize),
    Share(usize),
    // A favourite that names a network place: opened through the network service.
    Remote(String, String),
    Say(String, bool),
}

// Mounts.decodePath is decodeURIComponent, which throws on a malformed escape and so hands back the
// raw text. Each escape is checked as two hex digits, because from_str_radix takes a leading "+".
pub(crate) fn decode_path(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let (Some(&h), Some(&l)) = (b.get(i + 1), b.get(i + 2)) else { return raw.to_string() };
        if !h.is_ascii_hexdigit() || !l.is_ascii_hexdigit() {
            return raw.to_string();
        }
        let hex = |c: u8| (c as char).to_digit(16).unwrap() as u8;
        out.push(hex(h) << 4 | hex(l));
        i += 3;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

pub fn activate(entry: &Entry) -> Activation {
    match &entry.kind {
        Kind::Home => Activation::Open(entry.path.clone()),
        Kind::Trash => Activation::Say("The Trash view is not in this build yet.".into(), false),
        Kind::Favourite { error, .. } if !error.is_empty() => {
            Activation::Say(format!("Could not open {} · {error}", entry.label), true)
        }
        Kind::Favourite { .. } if entry.path.starts_with("file://") => Activation::Open(decode_path(&entry.path[7..])),
        Kind::Favourite { .. } if entry.path.contains("://") => Activation::Remote(entry.path.clone(), entry.label.clone()),
        Kind::Favourite { .. } => Activation::Open(entry.path.clone()),
        Kind::Device(i) => Activation::Device(*i),
        Kind::Share(i) => Activation::Share(*i),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Eject,
    MountVolume,
    OpenVolume,
    UnmountVolume,
    // A network share's own rows: the release, then the two a saved place owns.
    Unmount,
    Edit,
    Rename,
    Remove,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Eject => "Eject",
            Action::MountVolume => "Mount",
            Action::OpenVolume => "Open",
            Action::UnmountVolume | Action::Unmount => "Unmount",
            Action::Edit => "Edit",
            Action::Rename => "Rename",
            Action::Remove => "Remove",
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Action::Eject | Action::UnmountVolume | Action::Unmount => "eject",
            Action::Edit => "sliders",
            Action::Rename => "rename",
            Action::Remove => "minus",
            Action::MountVolume => "drive",
            Action::OpenVolume => "folder",
        }
    }
}

// Mounts.railMenu, for the rows this stage carries.
pub fn menu(d: &Device) -> Vec<Action> {
    if d.kind == "volume" && d.volume_menu {
        if !d.mounted {
            return vec![Action::MountVolume];
        }
        let mut rows = vec![Action::OpenVolume, Action::UnmountVolume];
        if d.removable {
            rows.push(Action::Eject);
        }
        return rows;
    }
    if d.mounted && d.kind == "volume" && d.removable {
        return vec![Action::Eject];
    }
    vec![]
}

// Mounts.rowMenu for a share: Unmount while mounted, then Edit, Rename and Remove for any share.
pub fn share_menu(s: &Share) -> Vec<Action> {
    let mut rows = if s.mounted { vec![Action::Unmount] } else { vec![] };
    rows.extend([Action::Edit, Action::Rename, Action::Remove]);
    rows
}

// Places.sidebarWidth: the stored width snapped to the nearest of four stops.
pub fn width(value: Option<f64>) -> f32 {
    const STOPS: [f64; 4] = [160.0, 192.0, 224.0, 256.0];
    let Some(v) = value.filter(|v| v.is_finite()) else { return 192.0 };
    STOPS.iter().copied().fold(STOPS[0], |best, s| if (s - v).abs() < (best - v).abs() { s } else { best }) as f32
}

#[cfg(test)]
#[path = "rail_tests.rs"]
mod tests;

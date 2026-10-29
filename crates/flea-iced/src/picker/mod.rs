// The portal request tools/flea-portal puts in FLEA_PICKER, and the answer this window writes back:
// a port of ui/js/Picker.js, pure so its tests drive it with no window.
use crate::format;
use crate::wire::Row;
use serde_json::{json, Value};

// The portal's own response codes: 1 is the user's no, and this front end never sends 2, because a
// window that opened at all can only pick or refuse.
pub const RESPONSE_OK: u8 = 0;
pub const RESPONSE_CANCELLED: u8 = 1;

pub const ALL_FILES: &str = "All files";
// Recent is a location and not a directory, so it is named by a token no listing path can equal.
pub const RECENT: &str = "flea:recent";
pub const RECENT_LABEL: &str = "Recent";
pub mod app;
mod keys;
#[cfg(test)]
mod live_tests;
pub mod recent;
mod replies;
pub mod state;
mod view;

pub const NAME_REFUSED: &str = "a name cannot be empty, . or .. , or contain a separator";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Open,
    Save,
    SaveFiles,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter {
    pub label: String,
    pub globs: Vec<String>,
    pub mimes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Req {
    pub mode: Mode,
    pub title: String,
    pub app: String,
    pub accept: String,
    pub multiple: bool,
    pub directory: bool,
    pub folder: String,
    pub name: String,
    pub filters: Vec<Filter>,
    pub current: String,
    pub current_index: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub path: String,
    pub bytes: u64,
    pub uri: String,
}

fn text(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        // String(read.x || ""): a number or a true stringifies, anything falsy is empty.
        Some(Value::Number(n)) if n.as_f64() != Some(0.0) => n.to_string(),
        Some(Value::Bool(true)) => "true".into(),
        _ => String::new(),
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

// Every field defaulted, because a request that arrived short must still open a window.
pub fn request(body: &str) -> Req {
    let read: Value = serde_json::from_str(body).ok().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let mode = match read.get("mode").and_then(Value::as_str) {
        Some("save") => Mode::Save,
        Some("savefiles") => Mode::SaveFiles,
        _ => Mode::Open,
    };
    let filters = read.get("filters").and_then(Value::as_array).into_iter().flatten()
        .map(|f| Filter { label: text(f, "label"), globs: strings(f.get("globs")), mimes: strings(f.get("mimes")) })
        .collect();
    Req {
        mode,
        title: text(&read, "title"),
        app: text(&read, "app"),
        accept: text(&read, "accept"),
        multiple: read.get("multiple") == Some(&Value::Bool(true)),
        directory: read.get("directory") == Some(&Value::Bool(true)),
        folder: text(&read, "folder"),
        name: text(&read, "name"),
        filters,
        current: text(&read, "current"),
        current_index: read.get("currentIndex").and_then(Value::as_f64).map(|f| f as i64).unwrap_or(-1),
    }
}

impl Req {
    pub fn saving(&self) -> bool {
        self.mode == Mode::Save
    }
    // A folder request, or a save of several files, which is asked as the folder they go into.
    pub fn folder_mode(&self) -> bool {
        self.directory || self.mode == Mode::SaveFiles
    }
}

// The caller's own title wins; the fallback says what the window is for rather than naming Flea.
pub fn title(r: &Req) -> String {
    if !r.title.is_empty() {
        return r.title.clone();
    }
    match r.mode {
        Mode::Save | Mode::SaveFiles => "Save file".into(),
        Mode::Open if r.directory => "Choose folder".into(),
        Mode::Open => "Choose file".into(),
    }
}

// Only a portal identity is shown; a host application has none, and inventing one would put a name
// the desktop never vouched for under a trusted-looking label.
pub fn subtitle(r: &Req) -> String {
    if r.app.is_empty() { String::new() } else { format!("Requested by {}", r.app) }
}

fn default_accept(r: &Req) -> &'static str {
    match r.mode {
        Mode::Save => "Save",
        Mode::SaveFiles => "Choose folder",
        Mode::Open if r.directory => "Choose folder",
        Mode::Open => "Open",
    }
}

// The primary button; the count rides on it in the multiple case, the board's "Send 3".
pub fn accept_label(r: &Req, count: usize) -> String {
    let base = if r.accept.is_empty() { default_accept(r).to_string() } else { r.accept.clone() };
    if r.multiple && count > 1 { format!("{base} {count}") } else { base }
}

pub fn status_line(count: usize, bytes: u64) -> String {
    if count == 0 { "0 selected".into() } else { format!("{count} selected · {}", format::size(bytes)) }
}

// The footer's right half, which says only the keys this mode actually answers.
pub fn hints(r: &Req) -> String {
    if r.saving() {
        return "Enter save · Esc cancel".into();
    }
    let pick = if r.directory { "Enter open · Space mark folder" } else { "Space select · Enter open/send" };
    format!("{pick} · Esc cancel")
}

// The caller's filters, then All files, which is always explicit; no filters is no chip row at all.
pub fn chips(r: &Req) -> Vec<(String, i64)> {
    if r.filters.is_empty() {
        return vec![];
    }
    let mut out: Vec<(String, i64)> = r.filters.iter().enumerate().map(|(i, f)| (f.label.clone(), i as i64)).collect();
    out.push((ALL_FILES.into(), -1));
    out
}

pub fn current_chip(r: &Req) -> i64 {
    if r.filters.is_empty() {
        return -1;
    }
    if r.current_index >= 0 && (r.current_index as usize) < r.filters.len() {
        return r.current_index;
    }
    r.filters.iter().position(|f| f.label == r.current).map(|i| i as i64).unwrap_or(0)
}

pub fn total_bytes(marks: &[Mark]) -> u64 {
    marks.iter().map(|m| m.bytes).sum()
}

// The backend's reviewed marks, each keeping the URI it was first marked under.
pub fn reviewed_marks(previous: &[Mark], reviewed: &[(String, u64)]) -> Vec<Mark> {
    reviewed.iter().map(|(path, bytes)| Mark {
        path: path.clone(),
        bytes: *bytes,
        uri: previous.iter().find(|m| &m.path == path).map(|m| m.uri.clone()).unwrap_or_else(|| file_uri(path)),
    }).collect()
}

// Mirrored from src/backend/ops.rs valid_name() character for character, so the two cannot drift.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") }
}

// A Recent listing's base is "/" and its rows are named by their path under it.
pub fn row_path(location: &str, name: &str) -> String {
    if location == RECENT { format!("/{name}") } else { join(location, name) }
}

pub fn is_directory(row: &Row) -> bool {
    row.d || (format::is_symlink(row.p) && row.i == "folder")
}

pub fn parent_of(path: &str) -> String {
    match path.rfind('/') {
        Some(cut) if cut > 0 => path[..cut].to_string(),
        _ => "/".into(),
    }
}

// Format.fileUri: encodeURI, then # and ? by hand, because encodeURI leaves both literal.
pub fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        let keep = b.is_ascii_alphanumeric() || b";,/:@&=+$-_.!~*'()".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// What tools/flea-portal reads out of the reply file. A refusal carries no URI at all.
pub fn reply(response: u8, uris: &[String], filter: i64) -> String {
    if response != RESPONSE_OK {
        return format!(r#"{{"response":{response}}}"#);
    }
    // Written in Picker.js's own key order, which serde_json's sorted map would not keep.
    let uris = serde_json::to_string(uris).unwrap_or_else(|_| "[]".into());
    format!(r#"{{"response":{RESPONSE_OK},"uris":{uris},"filter":{filter}}}"#)
}

// The chooser window, as `flea --pick` opens one for tools/flea-portal: its own app id, so the one
// Hyprland rule `flea --picker` writes floats it, and nothing opens without a reply file to answer.
pub fn launch() -> iced::Result {
    use iced::window;
    if std::env::var_os("FLEA_PICKER").is_none() || std::env::var("FLEA_PICKER_REPLY").unwrap_or_default().is_empty() {
        eprintln!("flea-iced: --pick needs FLEA_PICKER and FLEA_PICKER_REPLY, which tools/flea-portal sets");
        std::process::exit(2);
    }
    iced::application(app::boot, app::Picking::update, app::Picking::view)
        .title(app::Picking::title)
        .subscription(app::Picking::subscription)
        .exit_on_close_request(false)
        .default_font(iced::Font::DEFAULT)
        .settings(iced::Settings { default_text_size: crate::TEXT_PX.into(), ..Default::default() })
        .window(window::Settings {
            size: iced::Size::new(720.0, 520.0),
            platform_specific: window::settings::PlatformSpecific {
                application_id: "com.thisisgm.flea.picker".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

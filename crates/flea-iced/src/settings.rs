// The settings panel's whole model, ui/js/Settings.js and its section files, with no toolkit in it:
// the sections, the rows each draws from the stored document, which rows the cursor can reach, and
// the one patch a change writes. Every write is a one-leaf patch through `flea --ui-state`, which is
// ViewState.changeSetting's rule, so a change names what it changed and nothing else.
use crate::places_rows;
use serde_json::{json, Value};

pub const SECTIONS: [(&str, &str, &str); 10] = [
    ("view", "View", "sliders"),
    ("places", "Places", "star"),
    ("shelf", "Shelf", "shelf"),
    ("preview", "Preview", "preview"),
    ("open", "File types", "app-window"),
    ("keys", "Keys", "keyboard"),
    ("display", "Display", "maximize"),
    ("theme", "Theme", "palette"),
    ("menus", "Menus", "list"),
    ("about", "About", "info"),
];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Kind {
    #[default]
    Group,
    Check,
    Choice,
    Action,
    Hint,
    Lock,
    Fact,
    Hero,
    Favourite,
    Ruler,
    KeyPreview,
    // A File types rule: its ending, and the application it opens in.
    OpenRule,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub kind: Kind,
    pub id: String,
    pub label: String,
    pub value: String,
    pub caption: String,
    pub glyph: Option<&'static str>,
    pub on: bool,
    // false greys a row out and takes it off the cursor's path.
    pub unavailable: bool,
    // A group heading that carries its group's master switch, its state and the ids it governs.
    pub master: bool,
    pub state: &'static str,
    pub ids: Vec<&'static str>,
    pub action: Option<&'static str>,
    pub values: Vec<Value>,
    pub labels: Vec<String>,
    pub selected: usize,
    pub error: String,
    pub role: &'static str,
    pub items: Vec<(String, String)>,
    pub stops: Vec<u32>,
}

pub fn row(kind: Kind, id: &str, label: &str) -> Row {
    Row { kind, id: id.into(), label: label.into(), ..Default::default() }
}

pub fn group(label: &str) -> Row {
    row(Kind::Group, "", label)
}

pub fn hint(label: &str) -> Row {
    row(Kind::Hint, "", label)
}

pub fn check(id: &str, label: &str, glyph: &'static str, on: bool) -> Row {
    Row { glyph: Some(glyph), on, ..row(Kind::Check, id, label) }
}

pub fn fact(label: &str, value: &str) -> Row {
    Row { value: value.into(), ..row(Kind::Fact, "", label) }
}

// Settings.choice: the value is the label of the stored value, the first one when it is unknown.
pub fn choice(id: &str, label: &str, glyph: Option<&'static str>, values: Vec<Value>, labels: &[&str], stored: &Value) -> Row {
    let selected = values.iter().position(|v| v == stored).unwrap_or(0);
    Row {
        glyph,
        value: labels[selected].to_string(),
        labels: labels.iter().map(|l| l.to_string()).collect(),
        values,
        selected,
        ..row(Kind::Choice, id, label)
    }
}

fn s(values: &[&str]) -> Vec<Value> {
    values.iter().map(|v| json!(v)).collect()
}

fn at<'a>(data: &'a Value, path: &str) -> &'a Value {
    data.pointer(path).unwrap_or(&Value::Null)
}

fn word(data: &Value, path: &str, default: &str) -> Value {
    json!(at(data, path).as_str().unwrap_or(default))
}

// Settings.focusable.
pub fn focusable(r: &Row) -> bool {
    if r.unavailable {
        return false;
    }
    match r.kind {
        Kind::Group => r.master || r.action.is_some(),
        Kind::Ruler => r.on,
        Kind::Check | Kind::Choice | Kind::Action | Kind::Favourite | Kind::OpenRule => true,
        _ => false,
    }
}

pub fn step_row(rows: &[Row], from: usize, delta: isize) -> usize {
    let mut i = from as isize + delta;
    while i >= 0 && (i as usize) < rows.len() {
        if focusable(&rows[i as usize]) {
            return i as usize;
        }
        i += delta;
    }
    from
}

pub fn first_row(rows: &[Row]) -> usize {
    if rows.first().is_some_and(focusable) { 0 } else { step_row(rows, 0, 1) }
}

#[derive(Debug, Clone)]
pub struct Facts {
    pub home: String,
    pub base_size: u32,
    pub monitor_scale: f64,
    pub version: String,
    pub handler: String,
    pub save_status: String,
    // The installed applications, asked for while the panel is open and dropped when it closes.
    pub apps: Vec<crate::openrules::App>,
}

pub fn rows(section: &str, data: &Value, facts: &Facts) -> Vec<Row> {
    match section {
        "view" => view_rows(data, facts),
        "columns" => column_rows(data),
        "places" => places_rows::rows(data, facts),
        "shelf" => places_rows::shelf_rows(data),
        "preview" => preview_rows(data),
        "open" => crate::openrules::rows(data, &facts.apps),
        "keys" => key_rows(),
        "display" => places_rows::display_rows(data, facts),
        "theme" => theme_rows(data),
        "menus" => places_rows::menu_rows(data),
        _ => about_rows(facts),
    }
}

fn view_rows(data: &Value, facts: &Facts) -> Vec<Row> {
    let columns: Vec<String> = at(data, "/columns").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["name".into(), "size".into(), "date".into()]);
    let shown = columns.iter().map(|c| { let mut ch = c.chars(); ch.next().map(|f| f.to_uppercase().chain(ch).collect::<String>()).unwrap_or_default() }).collect::<Vec<_>>().join(", ");
    let save = &facts.save_status;
    vec![
        group("View"),
        choice("view", "Last-used view", None, s(&["list", "columns", "grid", "dual"]), &["List", "Columns", "Grid", "Dual pane"], &word(data, "/view", "list")),
        choice("density", "Row density", Some("list"), s(&["compact", "normal", "comfortable"]), &["Compact", "Normal", "Comfortable"], &word(data, "/density", "normal")),
        Row { glyph: Some("columns"), value: shown, ..row(Kind::Action, "columns", "Columns") },
        choice("addressBar", "Address bar", None, s(&["path", "breadcrumb"]), &["Path", "Breadcrumb"], &word(data, "/addressBar", "breadcrumb")),
        group("Sorting"),
        choice("sort.key", "Sort by", Some("sort"), s(&["name", "size", "date", "kind"]), &["Name", "Size", "Date", "Kind"], &word(data, "/sort/key", "name")),
        choice("sort.reverse", "Order", None, vec![json!(false), json!(true)], &["Ascending", "Descending"], &json!(at(data, "/sort/reverse").as_bool() == Some(true))),
        check("foldersFirst", "Folders first", "folders-first", at(data, "/foldersFirst").as_bool() != Some(false)),
        Row { caption: "folders, photos, files".into(), ..check("groupByKind", "Group by kind", "grid", at(data, "/groupByKind").as_bool() == Some(true)) },
        check("hidden", "Show hidden files", "eye", at(data, "/hidden").as_bool() == Some(true)),
        group("Cursor"),
        Row { caption: "arrow-up at the top".into(), ..check("wrapAtEnds", "Wrap at list ends", "arrow-up", at(data, "/wrapAtEnds").as_bool() == Some(true)) },
        group("Opening"),
        choice("startIn", "Flea opens in", Some("house"), s(&["home", "last", "folder"]), &["Home", "Last folder", "Chosen folder"], &word(data, "/startIn", "home")),
        Row {
            glyph: Some("folder"),
            value: at(data, "/startFolder").as_str().filter(|v| !v.is_empty()).unwrap_or("Use this folder").into(),
            ..row(Kind::Action, "startFolder", "Chosen folder")
        },
        choice("newTab", "New tabs open in", Some("columns"), s(&["current", "home", "start"]), &["Current folder", "Home", "Start folder"], &word(data, "/newTab", "current")),
        Row {
            role: if save.starts_with("Could not") { "error" } else { "accent" },
            ..hint(if save.is_empty() { "Saved · applied in this process" } else { save })
        },
    ]
}

fn column_rows(data: &Value) -> Vec<Row> {
    let columns: Vec<&str> = at(data, "/columns").as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_else(|| vec!["name", "size", "date"]);
    let mut out = vec![group("Columns"), Row { glyph: Some("file"), ..row(Kind::Lock, "", "Name") }];
    for (id, label) in [("mode", "Mode"), ("size", "Size"), ("items", "Items"), ("date", "Date"), ("kind", "Kind")] {
        out.push(Row { on: columns.contains(&id), ..row(Kind::Check, &format!("column:{id}"), label) });
    }
    out.push(Row { value: "Back".into(), ..row(Kind::Action, "backView", "Back to View") });
    out
}

fn preview_rows(data: &Value) -> Vec<Row> {
    let column = at(data, "/preview/column").as_bool() != Some(false);
    let manual = at(data, "/preview/loadOn").as_str() == Some("manual");
    vec![
        group("Preview column"),
        check("preview.column", "Show the preview column", "columns", column),
        Row { unavailable: !column, ..choice("preview.loadOn", "Load", Some("eye"), s(&["automatic", "manual"]), &["Automatic", "Manual"], &word(data, "/preview/loadOn", "automatic")) },
        hint(if manual { "Ctrl+Space loads the current selection." } else { "Automatic follows the cursor." }),
        group("Thumbnails"),
        choice("preview.thumbnails", "Thumbnails", Some("image"), s(&["off", "images", "media"]), &["Off", "Images", "Images and video"], &word(data, "/preview/thumbnails", "media")),
        choice("preview.thumbSize", "Thumbnail size", Some("maximize"), s(&["small", "medium", "large", "xlarge"]), &["Small", "Medium", "Large", "Extra large"], &word(data, "/preview/thumbSize", "medium")),
        check("preview.ctrlZoom", "Zoom the grid with ctrl and scroll", "move-horizontal", at(data, "/preview/ctrlZoom").as_bool() != Some(false)),
    ]
}

// Settings.keyRows over the one preset: its own keyPreview items are the Default keymap's.
fn key_rows() -> Vec<Row> {
    let items = [("arrows", "move cursor"), ("enter", "open"), ("space", "quick look"), ("ctrl-c", "copy"), ("ctrl-v", "paste"), ("delete", "trash")];
    vec![
        group("Preset"),
        Row { glyph: Some("keyboard"), labels: vec!["Default".into()], value: "Default".into(), ..row(Kind::Choice, "preset", "Keybinding preset") },
        hint("Keys change, actions do not."),
        group("This preset"),
        Row { items: items.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect(), ..row(Kind::KeyPreview, "keyPreview", "") },
        hint("Press ? for the keyboard sheet."),
    ]
}

fn theme_rows(data: &Value) -> Vec<Row> {
    let chosen = crate::theme::resolve(at(data, "/display/theme").as_str().unwrap_or("omarchy"));
    let mut out = vec![
        group("Theme"),
        Row { value: if chosen.is_none() { "the desktop's own".into() } else { String::new() }, ..check("theme:omarchy", "Follow Omarchy", "sliders", chosen.is_none()) },
        hint("Every window follows omarchy-theme-set until you pick one below."),
    ];
    out.extend(colour_rows(data));
    out.extend([
        Row { value: format!("{} themes", crate::theme::entries().count()), ..group("Bundled palettes") },
    ]);
    for (id, name) in crate::theme::entries() {
        out.push(check(&format!("theme:{id}"), name, "palette", chosen == Some(id)));
    }
    out.push(hint("Vendored from Strata, MIT. Most are Tinted Base16 palettes."));
    out
}

// The swatches a Colours row steps through, "" first, which is the palette's own.
pub const SWATCHES: [(&str, &str); 13] = [
    ("", "Theme"), ("#7aa2f7", "Blue"), ("#7dcfff", "Sky"), ("#2ac3de", "Teal"), ("#9ece6a", "Green"),
    ("#e0af68", "Yellow"), ("#ff9e64", "Orange"), ("#f7768e", "Red"), ("#ff007c", "Pink"),
    ("#bb9af7", "Violet"), ("#c0caf5", "White"), ("#737aa2", "Grey"), ("#3b4261", "Slate"),
];

// Settings > Theme > Colours: one choice per role over the swatches, and a hand-written "#rrggbb"
// the swatches do not carry kept as one more segment, so a step never silently replaces it.
fn colour_rows(data: &Value) -> Vec<Row> {
    let labels = [
        ("cursor", "Cursor and accent", "palette"),
        ("marked", "Marked rows", "check"),
        ("folder", "Folders", "folder"),
        ("file", "Files", "file"),
        ("executable", "Programs", "terminal"),
        ("symlink", "Links", "symlink"),
        ("mark", "File marks", "image"),
    ];
    let mut out = vec![group("Colours")];
    for (id, label, glyph) in labels {
        let stored = at(data, &format!("/display/colors/{id}")).as_str().unwrap_or("").to_lowercase();
        let mut values: Vec<Value> = SWATCHES.iter().map(|(v, _)| json!(v)).collect();
        let mut names: Vec<&str> = SWATCHES.iter().map(|(_, n)| *n).collect();
        if !SWATCHES.iter().any(|(v, _)| *v == stored) {
            values.push(json!(stored));
            names.push(&stored);
        }
        out.push(Row { role: "colour", ..choice(&format!("color:{id}"), label, Some(glyph), values, &names, &json!(stored)) });
    }
    out.push(hint("Left and Right step a colour; Theme keeps the palette's own. Folders also colour the folder mark."));
    out
}

fn about_rows(f: &Facts) -> Vec<Row> {
    let or = |v: &str| if v.is_empty() { "Not reported".to_string() } else { v.to_string() };
    vec![
        Row { value: "A file manager for Omarchy".into(), ..row(Kind::Hero, "", "Flea") },
        fact("Version", &or(&f.version)),
        fact("Built", "Not recorded in this build"),
        fact("Installed from", "Not reported"),
        fact("Package", "Not reported"),
        fact("Licence", "MIT, © 2026 GM"),
        group("Language"),
        Row { glyph: Some("globe"), ..fact("Language", "English") },
        group("Updates"),
        Row { glyph: Some("download"), ..fact("Update owner", "Omarchy") },
        group("This box"),
        Row { glyph: Some("folder"), ..fact("File manager", &or(&f.handler)) },
        Row { glyph: Some("keyboard"), value: "?".into(), ..row(Kind::Action, "keyboardSheet", "Keyboard sheet") },
        Row { glyph: Some("network"), value: "Open".into(), ..row(Kind::Action, "reportIssue", "Report an issue") },
        Row { value: "GitHub Sponsors".into(), ..row(Kind::Action, "support", "Support Flea") },
    ]
}

// ViewState.changeSetting: the rail is a word, a Theme row's id carries its palette, a dotted id is
// one leaf of its group, and anything else is a key of its own.
pub fn patch(id: &str, value: Value) -> Value {
    if id == "places.rail" {
        return json!({ "places": { "rail": if value == json!(true) { "shown" } else { "hidden" } } });
    }
    if let Some(role) = id.strip_prefix("color:") {
        return json!({ "display": { "colors": { role: value } } });
    }
    if let Some(theme) = id.strip_prefix("theme:") {
        return json!({ "display": { "theme": theme } });
    }
    match id.split_once('.') {
        Some((group, leaf)) => json!({ group: { leaf: value } }),
        None => json!({ id: value }),
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

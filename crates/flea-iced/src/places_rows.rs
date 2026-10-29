// The Settings sections whose rows are the longest: Places (Settings.placesRows), Shelf
// (SettingsShelf.js), Display (SettingsDisplay.js) and Menus (Settings.menuRows and its master).
// Split out of settings.rs at the file budget; the model and its rules are that file's.
use crate::settings::{check, choice, fact, group, hint, row, Facts, Kind, Row};
use serde_json::{json, Value};

fn at<'a>(data: &'a Value, path: &str) -> &'a Value {
    data.pointer(path).unwrap_or(&Value::Null)
}

pub fn rows(data: &Value, facts: &Facts) -> Vec<Row> {
    let places = at(data, "/places");
    let on = |k: &str| places.get(k).and_then(Value::as_bool) != Some(false);
    let set = |k: &str| places.get(k).and_then(Value::as_bool) == Some(true);
    let mut out = vec![Row { id: "addFavourite".into(), action: Some("addFavourite"), value: "Add this folder".into(), ..group("Favorites") }];
    let favourites = places.get("favourites").and_then(Value::as_array).cloned().unwrap_or_default();
    for (i, f) in favourites.iter().enumerate() {
        let path = f.get("path").and_then(Value::as_str).unwrap_or("");
        let label = f.get("label").and_then(Value::as_str).unwrap_or("Invalid favorite");
        let _ = &facts.home;
        out.push(Row {
            value: path.into(),
            glyph: Some(if path.contains("://") { "network" } else { "folder" }),
            ..row(Kind::Favourite, &format!("favourite:{i}"), label)
        });
    }
    out.push(group("Built in"));
    for (k, label, glyph) in [("showHome", "Home", "house"), ("showNetwork", "Network", "network"), ("showDevices", "Devices", "drive"), ("showTrash", "Trash", "trash")] {
        out.push(check(&format!("places.{k}"), label, glyph, on(k)));
    }
    out.push(group("Rail"));
    for (k, label, glyph) in [("driveSize", "Show drive size", "drive"), ("trashCount", "Show Trash count", "trash"), ("showUnmounted", "Show unmounted drives", "drive"), ("autoHide", "Auto-hide sidebar", "maximize")] {
        out.push(check(&format!("places.{k}"), label, glyph, set(k)));
    }
    out.push(Row { unavailable: set("autoHide"), ..check("places.rail", "Show sidebar", "columns", places.get("rail").and_then(Value::as_str) != Some("hidden")) });
    let width = crate::rail::width(places.get("sidebarWidth").and_then(Value::as_f64)) as u64;
    out.push(choice("places.sidebarWidth", "Sidebar width", Some("maximize"), vec![json!(160), json!(192), json!(224), json!(256)], &["160 px", "192 px", "224 px", "256 px"], &json!(width)));
    out.push(group("Network"));
    out.push(Row { caption: "in the login keyring".into(), ..check("places.rememberPasswords", "Remember passwords", "lock", set("rememberPasswords")) });
    out.push(hint("Off, a password is used for the one connection and never stored. On, the system keyring keeps it, the same store Nautilus and GNOME use, and the next connection needs no password at all."));
    out.push(group("Trash"));
    out.push(Row { caption: "permanently".into(), ..check("trashAutoEmpty", "Empty after 30 days", "history", at(data, "/trashAutoEmpty").as_bool() == Some(true)) });
    out
}

pub fn shelf_rows(data: &Value) -> Vec<Row> {
    let shelf = at(data, "/shelf");
    let enabled = shelf.get("enabled").and_then(Value::as_bool) == Some(true);
    let on = |k: &str| shelf.get(k).and_then(Value::as_bool) != Some(false);
    let edge = shelf.get("rail").and_then(Value::as_str).filter(|e| ["off", "left", "right", "bottom"].contains(e)).unwrap_or("off");
    let recent = shelf.get("recent").and_then(Value::as_u64).filter(|n| [1, 2, 3, 5].contains(n)).unwrap_or(3);
    vec![
        Row { id: "shelf.enabled".into(), master: true, state: if enabled { "all" } else { "none" }, value: if enabled { "on" } else { "off" }.into(), ..group("Shelf") },
        Row { unavailable: !enabled, ..check("shelf.bar", "Show in bar", "flea", on("bar")) },
        Row { unavailable: !enabled, ..choice("shelf.rail", "Edge rail", Some("maximize"), vec![json!("off"), json!("left"), json!("right"), json!("bottom")], &["Off", "Left", "Right", "Bottom"], &json!(edge)) },
        hint("Super+D opens it from anywhere; the README carries the bind."),
        group("Recent captures"),
        Row { unavailable: !enabled, ..check("shelf.screenshots", "Screenshots", "image", on("screenshots")) },
        Row { unavailable: !enabled, ..check("shelf.recordings", "Recordings", "film", on("recordings")) },
        Row { unavailable: !enabled, ..choice("shelf.recent", "Show", Some("list"), vec![json!(1), json!(2), json!(3), json!(5)], &["1", "2", "3", "5"], &json!(recent)) },
        Row { id: "pinFolder".into(), action: Some("pinFolder"), value: "Pin this folder".into(), ..group("Pinned") },
    ]
}

// TextSize: the seven Omarchy stops, and "system" or a stop is the only stored shape.
pub const STOPS: [u32; 7] = [9, 10, 11, 12, 14, 16, 20];

pub fn nearest(px: u32) -> u32 {
    STOPS.iter().copied().fold(STOPS[0], |best, s| if s.abs_diff(px) < best.abs_diff(px) { s } else { best })
}

pub fn following(data: &Value) -> bool {
    at(data, "/display/textSize/mode").as_f64().is_none_or(|n| n <= 0.0)
}

pub fn display_rows(data: &Value, f: &Facts) -> Vec<Row> {
    let follows = following(data);
    let effective = if follows { f.base_size } else { nearest(at(data, "/display/textSize/mode").as_f64().unwrap_or(0.0) as u32) };
    let near = nearest(effective);
    let note = if !follows { "Your override.".to_string() } else if near == effective { "Omarchy's own size.".into() } else { format!("Omarchy's own size. The ruler marks {near}, the nearest stop.") };
    let scale = if f.monitor_scale > 0.0 { format!("{}x", (f.monitor_scale * 100.0).round() / 100.0) } else { "not reported".into() };
    let theme = at(data, "/display/theme").as_str().unwrap_or("omarchy");
    vec![
        group("Text size"),
        Row { glyph: Some("type"), labels: vec!["Follow Omarchy".into(), "Override".into()], value: if follows { "Follow Omarchy" } else { "Override" }.into(), ..row(Kind::Choice, "textMode", "Text size") },
        Row { stops: STOPS.to_vec(), on: !follows, selected: STOPS.iter().position(|s| *s == near).unwrap_or(0), ..row(Kind::Ruler, "textStop", "") },
        hint("Ctrl+Shift +/- walks them, Ctrl+Shift+0 follows."),
        Row { role: "live", caption: note, ..fact("Effective", &format!("{effective} px")) },
        group("Scale"),
        Row { glyph: Some("maximize"), ..fact("Scale", &scale) },
        hint("Flea follows the compositor value and does not step or cycle it."),
        group("Appearance"),
        check("display.hyprlandIcons", "Hyprland-aware icons", "hyprland", at(data, "/display/hyprlandIcons").as_bool() == Some(true)),
        check("display.fileTypeColors", "File type marks and colours", "palette", at(data, "/display/fileTypeColors").as_bool() == Some(true)),
        hint("Marks are Flea's own; the colours are yazi's table."),
        group("Palette"),
        Row { glyph: Some("palette"), role: "live", ..fact("Theme", crate::theme::name_of(theme).unwrap_or("Omarchy")) },
        hint("The Theme section carries the catalog."),
    ]
}

const BASIC: [&str; 6] = ["cut", "copy", "paste", "duplicate", "rename", "trash"];
const GROUPS: [(&str, &str, &[&str]); 4] = [
    ("basic", "Basic file actions", &BASIC),
    ("destructive", "Destructive", &["delete"]),
    ("openInspect", "Open and inspect", &["openwith", "openTerminal", "moveto", "copyto", "properties", "permissions", "copypath"]),
    ("extras", "Extras", &["shelf", "compress", "extract", "convert", "taildrop", "localsend", "dropbox", "sharelink", "runScript", "placeMenu"]),
];

fn label(id: &str) -> &str {
    match id {
        "cut" => "Cut", "copy" => "Copy", "paste" => "Paste", "duplicate" => "Duplicate", "rename" => "Rename",
        "trash" => "Move to Trash", "openTerminal" => "Open in terminal", "copypath" => "Copy path", "permissions" => "Permissions",
        "delete" => "Delete permanently", "openwith" => "Open with", "moveto" => "Move to", "copyto" => "Copy to", "properties" => "Properties",
        "compress" => "Compress", "extract" => "Extract", "localsend" => "Send with LocalSend", "convert" => "Convert",
        "taildrop" => "Send with Taildrop", "dropbox" => "Move to Dropbox", "sharelink" => "Copy Share Link", "open" => "Open",
        "toggleHidden" => "Show hidden files", "shelf" => "Enable shelf", "placeMenu" => "Places row menu", "runScript" => "Run script",
        other => other,
    }
}

fn glyph(id: &str) -> Option<&'static str> {
    Some(match id {
        "cut" => "scissors", "copy" | "copyto" => "copy", "paste" => "clipboard", "duplicate" => "file-plus", "rename" => "rename",
        "trash" | "delete" => "trash", "openTerminal" | "runScript" => "terminal", "copypath" => "file-text", "permissions" => "lock",
        "compress" => "archive", "openwith" => "app-window", "moveto" => "folder-plus", "properties" => "info", "extract" => "archive-out",
        "convert" => "sliders", "sharelink" => "network", "open" | "placeMenu" => "folder-open", "toggleHidden" => "eye",
        _ => return None,
    })
}

pub fn hidden_set(data: &Value) -> Vec<String> {
    at(data, "/menu/hidden").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

// The shelf's row in Extras is the shelf's own switch, shelf.enabled, and never a menu id.
fn shown(data: &Value, hidden: &[String], id: &str) -> bool {
    if id == "shelf" {
        return at(data, "/shelf/enabled").as_bool() == Some(true);
    }
    !hidden.iter().any(|h| h == id)
}

pub fn menu_rows(data: &Value) -> Vec<Row> {
    let hidden = hidden_set(data);
    let mut out = vec![];
    for (id, name, ids) in GROUPS {
        let master = ids.len() > 1;
        let on = ids.iter().filter(|i| shown(data, &hidden, i)).count();
        let state = if on == ids.len() { "all" } else if on == 0 { "none" } else { "some" };
        out.push(Row {
            id: if master { id.into() } else { String::new() },
            master,
            ids: ids.to_vec(),
            state: if master { state } else { "" },
            value: if master { format!("{on} of {}", ids.len()) } else { String::new() },
            ..group(name)
        });
        for i in ids {
            out.push(Row {
                glyph: glyph(i),
                role: if *i == "delete" { "error" } else { "" },
                value: if *i == "delete" { "off by default".into() } else { String::new() },
                on: shown(data, &hidden, i),
                ..row(Kind::Check, i, label(i))
            });
        }
    }
    out.push(group("Shortcuts"));
    out.push(check("keyHints", "Show keyboard hints", "keyboard", at(data, "/keyHints").as_bool() == Some(true)));
    out.push(hint("Every key stays bound either way."));
    out.push(group("Always shown"));
    for i in ["open", "toggleHidden"] {
        out.push(Row { glyph: glyph(i), ..row(Kind::Lock, i, label(i)) });
    }
    out
}

// toggleMenuAction and toggleMenuGroup: the hidden set is the only state, a master turning its group
// on removes the group's ids from it, and the shelf rides its own boolean beside the set.
pub fn menu_patch(data: &Value, ids: &[&str], master: bool) -> Value {
    let mut hidden = hidden_set(data);
    let mut out = serde_json::Map::new();
    let enable = if master {
        ids.iter().filter(|i| shown(data, &hidden, i)).count() != ids.len()
    } else {
        !shown(data, &hidden, ids[0])
    };
    if ids.contains(&"shelf") {
        out.insert("shelf".into(), json!({ "enabled": enable }));
    }
    let menu_ids: Vec<&&str> = ids.iter().filter(|i| **i != "shelf").collect();
    if !menu_ids.is_empty() {
        hidden.retain(|h| !menu_ids.iter().any(|i| **i == h));
        if !enable {
            hidden.extend(menu_ids.iter().map(|i| i.to_string()));
        }
        out.insert("menu".into(), json!({ "hidden": hidden }));
    }
    Value::Object(out)
}

#[cfg(test)]
#[path = "places_rows_tests.rs"]
mod tests;

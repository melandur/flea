// The keys of ui.json this front end reads, through `flea --ui-state`, which is the one update path
// AGENTS.md "The state file" allows: the read prints the merged, validated document, and a write
// is a patch naming only what changed, so another window's settings are never written back.
use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    pub hidden: bool,
    pub sort_by: String,
    pub sort_desc: bool,
    pub folders_first: bool,
    pub group_by_kind: bool,
    pub wrap_at_ends: bool,
    pub start_in: String,
    pub start_folder: String,
    pub last_path: String,
    pub thumbnails: String,
    pub file_type_colours: bool,
    // The places group, Settings > Places.
    pub favourites: Vec<Value>,
    pub show_trash: bool,
    pub show_devices: bool,
    pub show_network: bool,
    pub remember_passwords: bool,
    pub show_unmounted: bool,
    pub rail_shown: bool,
    pub sidebar_width: Option<f64>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            hidden: false,
            sort_by: "name".into(),
            sort_desc: false,
            folders_first: true,
            group_by_kind: false,
            wrap_at_ends: false,
            start_in: "home".into(),
            start_folder: String::new(),
            last_path: String::new(),
            thumbnails: "media".into(),
            file_type_colours: false,
            favourites: vec![],
            show_trash: true,
            show_devices: true,
            show_network: true,
            remember_passwords: false,
            show_unmounted: false,
            rail_shown: true,
            sidebar_width: None,
        }
    }
}

impl Prefs {
    pub fn from_json(body: &str) -> Self {
        let d = Prefs::default();
        let Ok(v) = serde_json::from_str::<Value>(body) else { return d };
        let b = |k: &str, f: bool| v.get(k).and_then(Value::as_bool).unwrap_or(f);
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let key = v.pointer("/sort/key").and_then(Value::as_str).unwrap_or("name");
        Prefs {
            hidden: b("hidden", d.hidden),
            // ui/Backend.qml resetSort: the stored "date" is the wire's "mtime".
            sort_by: if key == "date" { "mtime".into() } else { key.into() },
            sort_desc: v.pointer("/sort/reverse").and_then(Value::as_bool).unwrap_or(false),
            // ui/Backend.qml sends `foldersFirst !== false`, so only an explicit false turns it off.
            folders_first: v.get("foldersFirst").and_then(Value::as_bool) != Some(false),
            group_by_kind: b("groupByKind", d.group_by_kind),
            wrap_at_ends: b("wrapAtEnds", d.wrap_at_ends),
            start_in: Some(s("startIn")).filter(|x| !x.is_empty()).unwrap_or(d.start_in),
            start_folder: s("startFolder"),
            last_path: s("lastPath"),
            // ViewState.qml: preview.thumbnails, "media" when absent; display.fileTypeColors, off.
            thumbnails: v.pointer("/preview/thumbnails").and_then(Value::as_str).unwrap_or("media").to_string(),
            file_type_colours: v.pointer("/display/fileTypeColors").and_then(Value::as_bool) == Some(true),
            favourites: v.pointer("/places/favourites").and_then(Value::as_array).cloned().unwrap_or_default(),
            // Sidebar.qml reads each switch as `!== false`, so only an explicit false hides a group.
            show_trash: v.pointer("/places/showTrash").and_then(Value::as_bool) != Some(false),
            show_devices: v.pointer("/places/showDevices").and_then(Value::as_bool) != Some(false),
            show_network: v.pointer("/places/showNetwork").and_then(Value::as_bool) != Some(false),
            remember_passwords: v.pointer("/places/rememberPasswords").and_then(Value::as_bool) == Some(true),
            show_unmounted: v.pointer("/places/showUnmounted").and_then(Value::as_bool) == Some(true),
            rail_shown: v.pointer("/places/rail").and_then(Value::as_str) != Some("hidden"),
            sidebar_width: v.pointer("/places/sidebarWidth").and_then(Value::as_f64),
        }
    }

    pub fn load(bin: &str) -> Self {
        match Command::new(bin).arg("--ui-state").output() {
            Ok(o) if o.status.success() => Prefs::from_json(&String::from_utf8_lossy(&o.stdout)),
            _ => Prefs::default(),
        }
    }

    // ui/js/Startup.js startPath: a named path outranks every setting, and an empty one is home.
    pub fn start_path(&self, home: &str, argv: Option<&str>) -> String {
        if let Some(p) = argv.filter(|p| !p.is_empty()) {
            return p.to_string();
        }
        let or_home = |p: &str| if p.is_empty() { home.to_string() } else { p.to_string() };
        match self.start_in.as_str() {
            "last" => or_home(&self.last_path),
            "folder" => or_home(&self.start_folder),
            _ => home.to_string(),
        }
    }
}

pub fn save_rail(bin: &str, shown: bool) -> bool {
    let patch = serde_json::json!({ "places": { "rail": if shown { "shown" } else { "hidden" } } }).to_string();
    Command::new(bin).arg("--ui-state").arg(patch).output().map(|o| o.status.success()).unwrap_or(false)
}

// Exit 0 is the only proof a save landed; see AGENTS.md "A refused write reaches the operator".
pub fn save_hidden(bin: &str, hidden: bool) -> bool {
    let patch = serde_json::json!({ "hidden": hidden }).to_string();
    Command::new(bin).arg("--ui-state").arg(patch).output().map(|o| o.status.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_shape_reads_as_the_wire_wants_it() {
        let p = Prefs::from_json(r#"{"sort":{"key":"date","reverse":true},"foldersFirst":false,"wrapAtEnds":true,"hidden":true}"#);
        assert_eq!(p.sort_by, "mtime");
        assert!(p.sort_desc && !p.folders_first && p.wrap_at_ends && p.hidden);
        assert!(Prefs::from_json("{}").folders_first);
        assert_eq!(Prefs::from_json("not json"), Prefs::default());
    }

    #[test]
    fn the_start_path_follows_startup_js() {
        let mut p = Prefs::default();
        assert_eq!(p.start_path("/h", None), "/h");
        assert_eq!(p.start_path("/h", Some("/x")), "/x");
        p.start_in = "last".into();
        assert_eq!(p.start_path("/h", None), "/h");
        p.last_path = "/l".into();
        assert_eq!(p.start_path("/h", None), "/l");
    }
}

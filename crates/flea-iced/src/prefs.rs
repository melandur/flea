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
    // open.rules: an ending and the application the operator chose for it.
    pub open_rules: Vec<(String, String)>,
    // view and dual: a window left split reopens split, with the pane that held the keyboard.
    pub view: String,
    pub dual_from: String,
    pub dual_focus: u8,
    pub theme_id: String,
    // menu.hidden, the Menus section's whole visibility state, read by menu::entries alone.
    pub menu_hidden: Vec<String>,
    // shelf.enabled: the shelf's own switch, which is what the menu's shelf row follows.
    pub shelf_enabled: bool,
    // preview.thumbSize, the grid's tile mark and thumbnail size.
    pub thumb_size: String,
    // ViewState.previewColumn and previewAutomatic: the columns view's file pane, and whether the
    // cursor row loads into it on its own once the cursor settles.
    pub preview_column: bool,
    pub preview_automatic: bool,
    // places.trashCount, the rail's Trash badge; trashAutoEmpty and trashSweptOn, the 30 day sweep.
    pub trash_count_shown: bool,
    pub trash_auto_empty: bool,
    pub trash_swept_on: i64,
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
            open_rules: vec![],
            view: "list".into(),
            dual_from: "list".into(),
            dual_focus: 0,
            theme_id: "omarchy".into(),
            // src/uischema.rs DEFAULT's own set, for a window whose read of the file failed.
            menu_hidden: ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copypath"]
                .map(String::from)
                .to_vec(),
            shelf_enabled: false,
            thumb_size: "medium".into(),
            preview_column: true,
            preview_automatic: true,
            trash_count_shown: false,
            trash_auto_empty: false,
            trash_swept_on: 0,
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
            open_rules: v.pointer("/open/rules").and_then(Value::as_array).into_iter().flatten()
                .filter_map(|r| Some((r.get("ends")?.as_str()?.to_lowercase(), r.get("app")?.as_str()?.to_string())))
                .collect(),
            view: Some(s("view")).filter(|x| !x.is_empty()).unwrap_or_else(|| "list".into()),
            dual_from: v.pointer("/dual/from").and_then(Value::as_str).unwrap_or("list").to_string(),
            theme_id: v.pointer("/display/theme").and_then(Value::as_str).unwrap_or("omarchy").to_string(),
            dual_focus: u8::from(v.pointer("/dual/focus").and_then(Value::as_u64) == Some(1)),
            menu_hidden: match v.pointer("/menu/hidden").and_then(Value::as_array) {
                Some(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                None => d.menu_hidden,
            },
            shelf_enabled: v.pointer("/shelf/enabled").and_then(Value::as_bool) == Some(true),
            thumb_size: v.pointer("/preview/thumbSize").and_then(Value::as_str).unwrap_or("medium").to_string(),
            preview_column: v.pointer("/preview/column").and_then(Value::as_bool) != Some(false),
            preview_automatic: v.pointer("/preview/loadOn").and_then(Value::as_str) != Some("manual"),
            trash_count_shown: v.pointer("/places/trashCount").and_then(Value::as_bool) == Some(true),
            trash_auto_empty: v.pointer("/trashAutoEmpty").and_then(Value::as_bool) == Some(true),
            trash_swept_on: v.pointer("/trashSweptOn").and_then(Value::as_i64).unwrap_or(0),
        }
    }

    pub fn load(bin: &str) -> Self {
        match Command::new(bin).arg("--ui-state").output() {
            Ok(o) if o.status.success() => Prefs::from_json(&String::from_utf8_lossy(&o.stdout)),
            _ => Prefs::default(),
        }
    }

    // SettingsOpen.chosen: the longest ending wins, case-folded, and a name that IS the ending is a
    // dotfile. Nav.openCursor asks this before the archive route, so a claimed .nii.gz goes to its
    // application rather than to Flea's own archive view.
    pub fn claimed(&self, name: &str) -> bool {
        let folded = name.to_lowercase();
        self.open_rules.iter().any(|(ends, _)| folded.len() > ends.len() && folded.ends_with(ends.as_str()))
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

// ViewState.recordTrashSweep: the day the sweep ran to its end, so it runs again tomorrow.
pub fn save_swept(bin: &str, day: i64) -> bool {
    let patch = serde_json::json!({ "trashSweptOn": day }).to_string();
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
    fn a_claimed_ending_is_case_folded_and_a_dotfile_is_not_it() {
        let p = Prefs::from_json(r#"{"open":{"rules":[{"ends":".nii.gz","app":"itksnap.desktop"}]}}"#);
        assert!(p.claimed("BRAIN.NII.GZ") && p.claimed("a.nii.gz"));
        assert!(!p.claimed(".nii.gz") && !p.claimed("a.gz"));
    }

    #[test]
    fn the_menu_hidden_set_is_read_as_stored_and_defaults_to_the_shipped_one() {
        assert_eq!(Prefs::from_json(r#"{"menu":{"hidden":["trash"]}}"#).menu_hidden, vec!["trash".to_string()]);
        assert_eq!(Prefs::from_json(r#"{"menu":{"hidden":[]}}"#).menu_hidden, Vec::<String>::new());
        assert!(Prefs::from_json("{}").menu_hidden.contains(&"delete".to_string()));
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

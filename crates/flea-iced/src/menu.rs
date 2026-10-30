// The listing's context menu, as ui/js/Menu.js builds it: one inventory in the order the Menus
// settings section shares, the file rows over a row and the background rows over empty space, each
// row's own availability rule, and the stored hidden set as the only visibility state. Toolkit free,
// so every rule here is a unit test rather than a window.
//
// The archive, convert, script and send rows are probed (Facts), and absent when the probe finds
// nothing, as the QML's are; a send row whose service answered no is greyed and drawn red instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Background,
    // TrashHost.menuAt: the Trash's own rows, and a selection inside the Trash view.
    Trash,
    TrashSelection,
    // PlaceMenu: a Places or Favorites row's own folder menu; true for a favourite.
    Place(bool),
    // Menu.headerEntries: the list header's own, the columns to show and the dotfile toggle.
    Header,
}

// What the rules read. `targets` is ui/js/Ops.js targetIndices' length: the selection when there is
// one, the cursor row otherwise, so a file menu never counts zero.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub path: String,
    pub targets: usize,
    pub row_mode: u32,
    pub clipboard: bool,
    pub show_hidden: bool,
    // The backend's `formats` answer: what bsdtar writes, whether it reads, whether ImageMagick runs.
    pub archive_formats: Vec<String>,
    pub can_extract: bool,
    pub can_convert: bool,
    // Archive.isArchive over the cursor row's name, and its icon being image-x-generic.
    pub row_is_archive: bool,
    pub row_is_image: bool,
    // ~/.config/flea/scripts as (id, label), sorted, read when the menu opens.
    pub scripts: Vec<(String, String)>,
    // The send rows' services, asked again whenever a menu opens; a row is greyed while they answer.
    pub providers_refreshing: bool,
    pub taildrop_installed: bool,
    pub taildrop_peers: Vec<(String, String)>,
    pub localsend_installed: bool,
    pub localsend_checking: bool,
    pub localsend_peers: Vec<(String, String)>,
    pub dropbox_installed: bool,
    // The account folder, only once its status said it is up; and whether the cursor row is inside it.
    pub dropbox_ready: bool,
    pub row_in_dropbox: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: &'static str,
    pub action: &'static str,
    pub label: &'static str,
    pub glyph: &'static str,
    pub disabled: bool,
    pub danger: bool,
    pub hint: &'static str,
    // A greyed row that is grey because its service refused, not because it is checking: drawn red.
    pub errored: bool,
    // A flyout's rows as (id, label): Sort by's orders, Compress's formats, Run script's scripts.
    pub sub: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Separator,
    Row(Entry),
}

// id, label, glyph, kinds (F file, B background, P places, T trash), group, action when not the id.
const INVENTORY: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("open", "Open", "folder-open", "FTP", "open", ""),
    ("openTab", "Open in new tab", "app-window", "P", "open", ""),
    ("openwith", "Open with", "app-window", "F", "open", "openWith"),
    ("newFolder", "New Folder", "folder-plus", "B", "open", ""),
    ("newFile", "New File", "file-plus", "B", "open", ""),
    ("cut", "Cut", "scissors", "F", "basic", ""),
    ("copy", "Copy", "copy", "F", "basic", ""),
    ("paste", "Paste", "clipboard", "FB", "basic", ""),
    ("duplicate", "Duplicate", "file-plus", "F", "basic", ""),
    ("rename", "Rename", "rename", "F", "basic", ""),
    ("selectAll", "Select all", "check", "B", "basic", ""),
    ("compress", "Compress", "archive", "F", "archive", ""),
    ("extract", "Extract", "archive-out", "F", "archive", ""),
    ("convert", "Convert", "sliders", "F", "archive", ""),
    ("shelf", "Add to shelf", "file", "F", "share", "addToShelf"),
    ("taildrop", "Send with Taildrop", "tailscale", "F", "share", ""),
    ("localsend", "Send with LocalSend", "localsend", "F", "share", ""),
    ("dropbox", "Move to Dropbox", "dropbox", "F", "share", ""),
    ("sharelink", "Copy Share Link", "network", "F", "share", ""),
    ("trash", "Move to Trash", "trash", "F", "trash", ""),
    ("delete", "Delete permanently", "trash", "F", "trash", "deletePermanently"),
    ("openTerminal", "Open in terminal", "terminal", "FBP", "inspect", ""),
    ("moveto", "Move to", "folder-plus", "F", "inspect", "moveTo"),
    ("copyto", "Copy to", "copy", "F", "inspect", "copyTo"),
    ("properties", "Properties", "info", "F", "inspect", ""),
    ("permissions", "Permissions", "lock", "F", "inspect", ""),
    ("copypath", "Copy path", "file-text", "FP", "inspect", ""),
    ("runScript", "Run script", "terminal", "F", "inspect", ""),
    ("addFavourite", "Add to Favorites", "star", "FBP", "inspect", ""),
    ("removeFavourite", "Remove from Favorites", "minus", "P", "inspect", ""),
    ("sort", "Sort by", "sort", "B", "view", ""),
    ("toggleHidden", "Show hidden files", "eye", "FB", "view", ""),
    ("settings", "Settings", "sliders", "B", "settings", ""),
    ("restoreAll", "Restore all", "undo", "T", "restore", ""),
    ("emptyTrash", "Empty Trash", "trash", "T", "empty", ""),
];


// The rows this build performs; every other row that survives is offered and says it is not here.
pub const PORTED: &[&str] = &[
    "open", "newFolder", "cut", "copy", "paste", "duplicate", "rename", "selectAll", "trash", "openTerminal", "copypath",
    "addFavourite", "sort", "toggleHidden", "settings", "properties", "newFile", "deletePermanently", "moveTo", "copyTo",
    "permissions", "openWith", "compress", "extract", "convert", "runScript", "addToShelf", "taildrop", "localsend",
    "dropbox", "sharelink",
];

// Sort.ORDERS with Menu.js SORT_LABELS: only an order the backend produces is ever offered.
pub const SORTS: &[(&str, &str)] = &[("name", "Name"), ("size", "Size"), ("mtime", "Modified"), ("kind", "Kind")];

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;

// Open and the hidden toggle are never hidden, so a hand-edited state file cannot empty the menu.
pub fn is_hidden(hidden: &[String], id: &str) -> bool {
    id != "open" && id != "toggleHidden" && hidden.iter().any(|h| h == id)
}

// Mounts.trashable: an MTP or gphoto2 mount has no trash directory, so it never offers the row.
pub fn trashable(path: &str) -> bool {
    let lower = path.to_lowercase();
    !(lower.contains("/gvfs/mtp:") || lower.contains("/gvfs/gphoto2:"))
}

pub fn entries(kind: Kind, f: &Facts, hidden: &[String]) -> Vec<Item> {
    let letter = if kind == Kind::File { 'F' } else { 'B' };
    let mut out = vec![];
    let mut group = "";
    for &(id, label, glyph, kinds, grp, action) in INVENTORY {
        if !kinds.contains(letter) || is_hidden(hidden, id) {
            continue;
        }
        let action = if action.is_empty() { id } else { action };
        let mut e = Entry { id, action, label, glyph, disabled: false, danger: false, hint: "", errored: false, sub: vec![] };
        if !available(&mut e, f, kind) {
            continue;
        }
        if !out.is_empty() && group != grp {
            out.push(Item::Separator);
        }
        group = grp;
        out.push(Item::Row(e));
    }
    out
}

// Menu.trashEntries and TrashHost's selection menu. Restore all and Empty Trash grey on an empty or
// busy Trash; a selection's two rows grey while a request is in flight, and only Delete can be hidden.
pub fn trash_entries(kind: Kind, total: usize, busy: bool, hidden: &[String]) -> Vec<Item> {
    let entry = |id, action, label, glyph, disabled| Entry { id, action, label, glyph, disabled, danger: false, hint: "", errored: false, sub: vec![] };
    if kind == Kind::TrashSelection {
        let mut out = vec![Item::Row(entry("restoreTrashSelection", "restoreTrashSelection", "Restore", "undo", busy))];
        if !is_hidden(hidden, "delete") {
            out.push(Item::Separator);
            out.push(Item::Row(Entry { danger: true, ..entry("delete", "deletePermanently", "Delete permanently", "trash", busy) }));
        }
        return out;
    }
    let mut out = vec![];
    let mut group = "";
    for &(id, label, glyph, kinds, grp, _) in INVENTORY {
        if !kinds.contains('T') || is_hidden(hidden, id) {
            continue;
        }
        if !out.is_empty() && group != grp {
            out.push(Item::Separator);
        }
        group = grp;
        out.push(Item::Row(entry(id, id, label, glyph, id != "open" && (total == 0 || busy))));
    }
    out
}

// Menu.headerEntries: Hide or Show for each optional column the list can draw, then the hidden-files
// row.
pub fn header_entries(columns: &[String], show_hidden: bool) -> Vec<Item> {
    let entry = |id, label, glyph| Entry { id, action: id, label, glyph, disabled: false, danger: false, hint: "", errored: false, sub: vec![] };
    let shown = |k: &str| columns.iter().any(|c| c == k);
    let rows: [(&'static str, &'static str, &'static str, &'static str, &'static str); 5] = [
        ("col:mode", "Hide Mode", "Show Mode", "lock", "mode"),
        ("col:size", "Hide Size", "Show Size", "drive", "size"),
        ("col:items", "Hide Items", "Show Items", "list", "items"),
        ("col:date", "Hide Modified", "Show Modified", "download", "date"),
        ("col:kind", "Hide Kind", "Show Kind", "type", "kind"),
    ];
    let mut out: Vec<Item> = rows.iter().map(|&(id, hide, show, glyph, key)| Item::Row(entry(id, if shown(key) { hide } else { show }, glyph))).collect();
    out.push(Item::Separator);
    out.push(Item::Row(if show_hidden { entry("toggleHidden", "Hide hidden files", "eye-off") } else { entry("toggleHidden", "Show hidden files", "eye") }));
    out
}

// PlaceMenu.entries: the folder menu while its Extras row is on, the favourite's one Remove while it
// is off, and nothing for Home then; the last row adds the favourite or removes it, never both.
pub fn place_entries(favourite: bool, hidden: &[String]) -> Vec<Item> {
    let entry = |id, action, label, glyph| Entry { id, action, label, glyph, disabled: false, danger: false, hint: "", errored: false, sub: vec![] };
    if is_hidden(hidden, "placeMenu") {
        return if favourite { vec![Item::Row(entry("removeFavourite", "removeFavourite", "Remove", "minus"))] } else { vec![] };
    }
    let mut out = vec![];
    let mut group = "";
    for &(id, label, glyph, kinds, grp, _) in INVENTORY {
        if !kinds.contains('P') || is_hidden(hidden, id) || (id == "addFavourite" && favourite) || (id == "removeFavourite" && !favourite) {
            continue;
        }
        if !out.is_empty() && group != grp {
            out.push(Item::Separator);
        }
        group = grp;
        out.push(Item::Row(entry(id, id, label, glyph)));
    }
    out
}

fn available(e: &mut Entry, f: &Facts, kind: Kind) -> bool {
    let one = f.targets == 1;
    match e.action {
        "addFavourite" if kind == Kind::File => e.disabled = !one || f.row_mode & S_IFMT != S_IFDIR,
        "paste" => e.disabled = !f.clipboard,
        "duplicate" | "rename" | "properties" | "openWith" => e.disabled = !one,
        "permissions" => {
            let t = f.row_mode & S_IFMT;
            e.disabled = !(one && (t == S_IFREG || t == S_IFDIR));
            e.hint = if !one { "Unavailable" } else if t == S_IFLNK { "Symlink target not changed" } else if e.disabled { "Unavailable" } else { "" };
        }
        "trash" if !trashable(&f.path) => return false,
        "sort" => e.sub = SORTS.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect(),
        "runScript" if f.scripts.is_empty() => return false,
        "runScript" => e.sub = f.scripts.clone(),
        // The submenu is exactly the table the backend probed, never a fixed list.
        "compress" if f.archive_formats.is_empty() => return false,
        "compress" => e.sub = f.archive_formats.iter().map(|x| (x.clone(), format!(".{x}"))).collect(),
        "extract" if !(f.row_is_archive && f.can_extract && one) => return false,
        "convert" if !(f.row_is_image && f.can_convert && one) => return false,
        "addToShelf" => e.glyph = "flea",
        "taildrop" if !f.taildrop_installed => return false,
        "taildrop" => {
            e.sub = f.taildrop_peers.clone();
            e.disabled = f.providers_refreshing || e.sub.is_empty();
            e.errored = e.disabled && !f.providers_refreshing;
        }
        "localsend" if !f.localsend_installed => return false,
        "localsend" => {
            e.sub = f.localsend_peers.clone();
            e.disabled = f.localsend_checking || e.sub.is_empty();
            e.errored = e.disabled && !f.localsend_checking;
        }
        // Move to Dropbox outside the account folder, Copy Share Link inside it, never both.
        "dropbox" | "sharelink" if !f.dropbox_installed || (e.action == "dropbox") == f.row_in_dropbox => return false,
        "dropbox" | "sharelink" => {
            e.disabled = f.providers_refreshing || !f.dropbox_ready;
            e.errored = e.disabled && !f.providers_refreshing;
        }
        // The label flips with the state; MenuRow draws no checkmark.
        "toggleHidden" if f.show_hidden => {
            e.label = "Hide hidden files";
            e.glyph = "eye-off";
        }
        _ => {}
    }
    e.danger = matches!(e.action, "trash" | "deletePermanently");
    true
}

// ContextMenu.stepCursor: a separator or a disabled row is never where the cursor rests, and the
// ends hold rather than wrap.
pub fn step(items: &[Item], from: usize, delta: isize) -> usize {
    let mut at = from as isize;
    loop {
        at += delta;
        if at < 0 || at as usize >= items.len() {
            return from;
        }
        if matches!(&items[at as usize], Item::Row(e) if !e.disabled) {
            return at as usize;
        }
    }
}

pub fn first(items: &[Item]) -> Option<usize> {
    items.iter().position(|i| matches!(i, Item::Row(e) if !e.disabled))
}

// Menu.clamp: flip at the far edge first, then shift only when neither side fits.
pub fn clamp(point: f32, size: f32, bounds: f32) -> f32 {
    let start = if point + size <= bounds { point } else { point - size };
    start.min((bounds - size).max(0.0)).max(0.0)
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;

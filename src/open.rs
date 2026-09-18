use crate::backend::mime;
use crate::openrules;
use crate::thp;
use crate::uistore;
use crate::userfile::data_file;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// The exit statuses ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;
pub const IS_DIRECTORY: i32 = 3;

// Canonical, so a file named --output=/etc/x cannot be read as a flag by the child.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// gio open is the OEM route: it asks the desktop database, so Terminal=true is honoured; see AGENTS.md "Opening a file".
pub fn open(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that file could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if target.is_dir() {
        return IS_DIRECTORY;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // Settings > File types first, because it is the only thing on this box that knows an ending
    // from a type: `gio open` sniffs, and a sniffer reads brain.nii.gz as the gzip stream it is. A
    // chosen application that is no longer installed says so and the desktop is asked anyway, so a
    // stale rule cannot make a file unopenable.
    if let Some(app) = chosen_app(&target) {
        if launch(&app, &target).is_some() {
            return 0;
        }
        eprintln!("flea: {app} could not be launched, so this file went to the desktop's own handler instead");
    }
    // corner: waited for, not detached, and on an archive that wait is a cold handler start; see AGENTS.md "Opening a file".
    let finished = Command::new("gio")
        .arg("open")
        .arg(&target)
        // The handler outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the opened program.
        .process_group(0)
        .status();
    match finished {
        Ok(status) if status.success() => 0,
        // A launcher that refused, which a spawn nobody waited on used to report as a clean handoff.
        Ok(_) => {
            // A file with no bytes is exactly what GIO abstains on, so this refusal is the database
            // holding no handler for "empty document" rather than the file being unopenable; see
            // AGENTS.md "When the sniffer abstains". Nothing else reaches the second launch, so an
            // ordinary open still spawns exactly one process.
            if empty(&target) && open_by_name(&target).is_some() {
                return 0;
            }
            eprintln!("flea: gio open refused that file, so no application on this system took it");
            FAILED
        }
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open that file");
            FAILED
        }
    }
}

// The one condition the two sniffer sentinels stand for, asked of the file rather than of a string:
// `gio open` sniffs the type itself and takes no override, so unlike Open with, the rule cannot be a
// different type here and has to become a second launch instead. A file that vanished between the
// canonicalize above and here is not empty, it is gone, and the caller's sentence covers it.
fn empty(target: &Path) -> bool {
    std::fs::metadata(target).map(|meta| meta.len() == 0).unwrap_or(false)
}

// The application Settings > File types names for this file's ending, or None when the table says
// nothing about it, which is every file on a box whose operator has added no rule.
fn chosen_app(target: &Path) -> Option<String> {
    let name = target.file_name()?.to_str()?;
    let state = uistore::Store::user().ok()?.read();
    openrules::chosen(&state, name)
}

// The desktop entry the name's own type names, launched the way menu_registry::launch launches a
// chosen one. Every step answers Option rather than an error: the caller already holds the sentence
// to print, and a fallback that could not be built says the same thing as one that was refused.
fn open_by_name(target: &Path) -> Option<()> {
    let name = target.file_name()?.to_str()?;
    let db = mime::Db::load();
    // No glob for this name either: nothing is known about the file, and there is nothing to launch.
    let kind = db.lookup(name)?;
    launch(&default_handler(kind)?, target)
}

// One desktop id, launched through the registry the desktop itself reads. The id is held to
// openrules::is_app's shape before it becomes a path component, because both callers reach here
// with a string out of a file: one out of ui.json and one out of `gio mime`.
fn launch(id: &str, target: &Path) -> Option<()> {
    if !openrules::is_app(id) {
        return None;
    }
    let entry = data_file(&format!("applications/{id}"))?;
    // THP was handed back before the first spawn and is inherited here, so this child needs no hook.
    let launched = Command::new("gio")
        .arg("launch")
        .arg(&entry)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .status()
        .ok()?;
    launched.success().then_some(())
}

// Sample GIO mime output, first line: "Default application for \u{201c}text/plain\u{201d}: micro.desktop".
// LC_ALL=C because that prefix is what is matched; the registry rows under it are not read here.
fn default_handler(kind: &str) -> Option<String> {
    let out = Command::new("gio")
        .arg("mime")
        .arg(kind)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .find(|line| line.starts_with("Default application"))?
        .rsplit(':')
        .next()?
        .trim()
        .to_string();
    // The id becomes a path component above, so it is held to the shape menu_registry::resolve holds one to.
    (id.ends_with(".desktop") && !id.contains('/') && !id.contains('\0')).then_some(id)
}

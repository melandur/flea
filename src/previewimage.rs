// flea --preview-image <path> <size>: an image rendered for the preview by the jailed thumbnailer,
// never by the window itself. Qt picks its decoder by content, so a photo.jpg that is PostScript
// inside reached kimageformats' EPS plugin, which runs Ghostscript, in the UI process with the user's
// full rights; the one file built to make the jailed thumbnailer fail was exactly the one then
// decoded unjailed. Here the same thumbnailer the cache uses, under the same bwrap and prlimit,
// renders a PNG at the size the preview wants, and the window only ever draws that PNG.
use crate::backend::aliases::Aliases;
use crate::backend::child::{run_with_timeout, Ran};
use crate::backend::mime::Db;
use crate::backend::sandbox;
use crate::backend::thumbargv::argv;
use crate::backend::thumbcache::uri_for;
use crate::backend::thumbspec::Thumbnailers;
use crate::backend::thumbwrite::exclusive_temp;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

// The thumbnail pool's own ceiling: a decode past it is a runaway, not a slow file.
const TIMEOUT: Duration = Duration::from_secs(20);
// Enough for a 4K window's long edge; the preview asks for less on a smaller one.
const MAX_SIZE: u32 = 4096;
// Renders kept for going back and forth between photos; the oldest go first.
const KEEP: usize = 48;
const DIR: &str = "flea-preview";

pub fn command(args: &[String]) -> i32 {
    let (path, size) = match args {
        [_, _, path, size] => match size.parse::<u32>() {
            Ok(n) if n > 0 => (PathBuf::from(path), n.min(MAX_SIZE)),
            _ => return refuse("--preview-image takes a path and a size"),
        },
        _ => return refuse("--preview-image takes a path and a size"),
    };
    match render(&path, size) {
        Ok(png) => {
            println!("{}", png.display());
            0
        }
        Err(why) => refuse(&format!("{}: {}", path.display(), why)),
    }
}

fn refuse(message: &str) -> i32 {
    eprintln!("flea: {}", message);
    1
}

pub fn render(path: &Path, size: u32) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| crate::error::io_message(&e))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if !sandbox::available() {
        return Err("the sandbox is unavailable: bwrap or prlimit is not on PATH".into());
    }
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("the name is not text")?;
    let aliases = Aliases::load();
    let specs = Thumbnailers::load(&aliases);
    let mime = Db::load();
    let kind = mime.lookup(name).ok_or("its type is not known")?;
    let spec = specs.for_mime(kind, &aliases).ok_or("no thumbnailer renders this type")?;
    let dir = private_dir()?;
    // Keyed on what the render depends on, so a file edited since is rendered again.
    let key = format!("{}\n{}\n{}\n{}\n{}", uri_for(path), meta.mtime(), meta.mtime_nsec(), meta.len(), size);
    let done = dir.join(format!("{}.png", crate::backend::md5::hex(key.as_bytes())));
    if done.metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
        return Ok(done);
    }
    let temp = exclusive_temp(&dir).ok_or("no private file to render into")?;
    let result = (|| {
        let (abs, inner) = argv(spec, path, &temp, size).ok_or("the thumbnailer cannot be called for this file")?;
        match run_with_timeout(&sandbox::wrap(&inner, &abs, &temp), TIMEOUT) {
            Ran::Succeeded if temp.metadata().is_ok_and(|m| m.len() > 0) => {}
            _ => return Err("the jailed decoder could not read it".to_string()),
        }
        std::fs::rename(&temp, &done).map_err(|e| crate::error::io_message(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    sweep(&dir);
    Ok(done)
}

// $XDG_RUNTIME_DIR is this user's own tmpfs, gone at logout. The folder is made 0700 and anything
// already at the name that is not this user's own 0700 directory, a symlink included, is refused.
fn private_dir() -> Result<PathBuf, String> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).ok_or("XDG_RUNTIME_DIR is not set")?;
    let dir = Path::new(&runtime).join(DIR);
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => return Ok(dir),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(crate::error::io_message(&e)),
    }
    let meta = dir.symlink_metadata().map_err(|e| crate::error::io_message(&e))?;
    // SAFETY: getuid cannot fail.
    let me = unsafe { getuid() };
    if !meta.is_dir() || meta.uid() != me || meta.permissions().mode() & 0o077 != 0 {
        return Err(format!("{} is not this user's private folder", dir.display()));
    }
    Ok(dir)
}

extern "C" {
    fn getuid() -> u32;
}

// Only finished renders count, oldest first, so the folder never grows past KEEP of them.
fn sweep(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut renders: Vec<(std::time::SystemTime, PathBuf)> = read.flatten()
        .filter(|e| e.file_name().to_string_lossy().len() == 36 && e.path().extension().is_some_and(|x| x == "png"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if renders.len() <= KEEP {
        return;
    }
    renders.sort();
    for (_, old) in &renders[..renders.len() - KEEP] {
        let _ = std::fs::remove_file(old);
    }
}

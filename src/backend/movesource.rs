// The second half of a move across filesystems: the source goes only once its copy exists, and only
// the parts of it the copy actually carried. remove_dir_all on the source took with it anything that
// landed there while the copy ran, a file a sync client or a download was still writing into the
// folder, so the tree is photographed before the copy and only what is still exactly as photographed,
// and has its copy in place, is removed afterwards. Whatever is new or changed stays where it was.
use crate::error::{from_io, FleaError};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// What one entry was before the copy: the same file, not yet written to since, is the same inode with
// the same size and the same change time, which any write or rename inside it moves on.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Seen {
    dev: u64,
    ino: u64,
    kind: u32,
    len: u64,
    changed: (i64, i64),
}

fn seen(meta: &std::fs::Metadata) -> Seen {
    Seen { dev: meta.dev(), ino: meta.ino(), kind: meta.mode() & 0o170000, len: meta.len(), changed: (meta.ctime(), meta.ctime_nsec()) }
}

// Every entry under the source, keyed by its path relative to the source's root, symlinks not followed.
pub struct Snapshot(HashMap<PathBuf, Seen>);

pub fn snapshot(root: &Path) -> Result<Snapshot, FleaError> {
    let mut map = HashMap::new();
    walk(root, Path::new(""), &mut map)?;
    Ok(Snapshot(map))
}

// root.join("") is "root/", which stats as ENOTDIR when the root is a file, so the root is itself.
fn under(root: &Path, rel: &Path) -> PathBuf {
    if rel.as_os_str().is_empty() { root.to_path_buf() } else { root.join(rel) }
}

fn walk(root: &Path, rel: &Path, map: &mut HashMap<PathBuf, Seen>) -> Result<(), FleaError> {
    let path = under(root, rel);
    let meta = path.symlink_metadata().map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
    map.insert(rel.to_path_buf(), seen(&meta));
    if meta.is_dir() {
        for entry in std::fs::read_dir(&path).map_err(|e| from_io("move", &path.to_string_lossy(), &e))? {
            let entry = entry.map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
            walk(root, &rel.join(entry.file_name()), map)?;
        }
    }
    Ok(())
}

// Removes what the copy carried and nothing else, and answers how many entries it had to leave: one
// that appeared after the photograph, one written to since, or one whose copy is not there.
pub fn remove_copied(src: &Path, dst: &Path, before: &Snapshot) -> Result<usize, FleaError> {
    remove_at(src, dst, Path::new(""), before)
}

fn remove_at(src: &Path, dst: &Path, rel: &Path, before: &Snapshot) -> Result<usize, FleaError> {
    let path = under(src, rel);
    let meta = path.symlink_metadata().map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
    let unchanged = before.0.get(rel) == Some(&seen(&meta));
    let copy = under(dst, rel).symlink_metadata().ok();
    let carried = copy.as_ref().is_some_and(|c| c.mode() & 0o170000 == meta.mode() & 0o170000
        && (!meta.is_file() || c.len() == meta.len()));
    if meta.is_dir() {
        // A directory's own ctime moves whenever an entry inside it comes or goes, which is exactly
        // what removing its children does, so a directory is judged by its contents and then emptied.
        if before.0.get(rel).map(|s| (s.dev, s.ino)) != Some((meta.dev(), meta.ino())) || !carried {
            return Ok(1);
        }
        let mut kept = 0;
        for entry in std::fs::read_dir(&path).map_err(|e| from_io("move", &path.to_string_lossy(), &e))? {
            let entry = entry.map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
            kept += remove_at(src, dst, &rel.join(entry.file_name()), before)?;
        }
        if kept == 0 {
            std::fs::remove_dir(&path).map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
        }
        return Ok(kept);
    }
    if !unchanged || !carried {
        return Ok(1);
    }
    std::fs::remove_file(&path).map_err(|e| from_io("move", &path.to_string_lossy(), &e))?;
    Ok(0)
}

// The sentence a move that had to leave something behind ends with; the copy is complete and kept.
pub fn kept_error(src: &Path, kept: usize) -> FleaError {
    let what = if kept == 1 { "1 item".to_string() } else { format!("{} items", kept) };
    FleaError {
        where_: "move".to_string(),
        path: src.to_string_lossy().to_string(),
        msg: format!("{} appeared or changed there while it was copied, so the original was kept beside the copy", what),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::copyfile::{copy_any, Progress};
    use crate::backend::testdir::TestDir;
    use std::time::{Duration, SystemTime};

    fn copied(src: &Path, dst: &Path) {
        let flag = std::sync::atomic::AtomicBool::new(false);
        let mut sink = |_: u64, _: u64| {};
        let mut p = Progress { cancel: &flag, on_bytes: &mut sink, tree: None, partial: None };
        copy_any(src, dst, &mut p).expect("copy");
    }

    // The ops review's finding: remove_dir_all after the copy took what a sync client wrote meanwhile.
    #[test]
    fn what_lands_or_changes_in_the_source_during_the_copy_is_kept() {
        let d = TestDir::new("movesourcekept");
        let src = d.dir("album");
        std::fs::create_dir(src.join("nested")).unwrap();
        std::fs::write(src.join("a.jpg"), "a").unwrap();
        std::fs::write(src.join("nested/b.jpg"), "b").unwrap();
        std::fs::write(src.join("growing.part"), "1").unwrap();
        let before = snapshot(&src).unwrap();
        let dst = d.join("moved");
        copied(&src, &dst);
        // The copy has run; meanwhile a new file arrived and one it had copied grew.
        std::fs::write(src.join("nested/late.jpg"), "late").unwrap();
        std::fs::write(src.join("growing.part"), "12").unwrap();
        let kept = remove_copied(&src, &dst, &before).unwrap();
        assert_eq!(kept, 2, "the late file and the grown one stay");
        assert!(!src.join("a.jpg").exists(), "an untouched file the copy carried goes");
        assert!(!src.join("nested/b.jpg").exists());
        assert_eq!(std::fs::read_to_string(src.join("nested/late.jpg")).unwrap(), "late");
        assert_eq!(std::fs::read_to_string(src.join("growing.part")).unwrap(), "12");
        assert_eq!(std::fs::read_to_string(dst.join("nested/b.jpg")).unwrap(), "b");
    }

    #[test]
    fn an_untouched_tree_goes_whole_and_a_single_file_root_works() {
        let d = TestDir::new("movesourceclean");
        let src = d.dir("tree");
        std::fs::create_dir(src.join("empty")).unwrap();
        std::fs::write(src.join("f"), "f").unwrap();
        std::os::unix::fs::symlink("f", src.join("link")).unwrap();
        let before = snapshot(&src).unwrap();
        copied(&src, &d.join("copy"));
        assert_eq!(remove_copied(&src, &d.join("copy"), &before).unwrap(), 0);
        assert!(!src.exists(), "the whole source goes once every part of it was carried");
        let file = d.file("one.txt", "x");
        let before = snapshot(&file).unwrap();
        copied(&file, &d.join("two.txt"));
        assert_eq!(remove_copied(&file, &d.join("two.txt"), &before).unwrap(), 0);
        assert!(!file.exists());
    }

    #[test]
    fn a_source_whose_copy_is_missing_is_never_removed() {
        let d = TestDir::new("movesourcemissing");
        let file = d.file("only.txt", "only copy");
        let before = snapshot(&file).unwrap();
        assert_eq!(remove_copied(&file, &d.join("never-written.txt"), &before).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "only copy");
    }

    // The ops review's other finding: every copy was stamped with today's date.
    #[test]
    fn a_copy_keeps_its_source_modification_time_for_files_and_folders() {
        let d = TestDir::new("copykeepstimes");
        let src = d.dir("old");
        std::fs::write(src.join("photo.jpg"), "p").unwrap();
        let then = SystemTime::UNIX_EPOCH + Duration::from_secs(978_307_200); // 2001-01-01
        let times = std::fs::FileTimes::new().set_modified(then).set_accessed(then);
        std::fs::File::options().write(true).open(src.join("photo.jpg")).unwrap().set_times(times).unwrap();
        std::fs::File::open(&src).unwrap().set_times(times).unwrap();
        copied(&src, &d.join("new"));
        let mtime = |p: &Path| p.metadata().unwrap().modified().unwrap();
        assert_eq!(mtime(&d.join("new/photo.jpg")), then);
        assert_eq!(mtime(&d.join("new")), then, "the folder's own time is set after its children landed");
    }
}

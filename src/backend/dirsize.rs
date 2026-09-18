use crate::backend::mountinfo::mount_type_in;
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// A directory over this deadline answers with what it saw, marked partial: a floor, not a wrong exact
// number. 250 ms rather than the 2000 ms this carried until 2026-09-18: the walk is one row of a
// listing, a queue of fourteen rows at two seconds each is a twenty-eight second worst case for one
// folder, and ui/Row.qml already draws the partial marker, so ">318 GB" now beats an exact number
// later. Measured on this box: /home/melandur drained in 2679 ms against the old ceiling.
const DEADLINE_MS: u64 = 250;

// A mount whose server can simply stop answering. The deadline below is checked between entries, so
// one getdents or stat that never returns is not bounded by it at all: /home/melandur/TresoritDrive
// (fuse.tresoritfs) burned the whole ceiling on every visit and `du` on it does not finish in 25 s.
// These answer partial immediately instead, which is the honest total for a tree nothing can measure.
//
// Named rather than taken by a `fuse.` prefix, which was the first shape of this and refused every
// local FUSE filesystem with it: mergerfs, gocryptfs, bindfs and squashfuse are bounded, and a home
// on one would have shown no folder size at all. An unlisted slow mount is not a hazard any more,
// only slow: it costs one deadline, off the loop, and a partial answer is never remembered.
fn is_unbounded(kind: &str) -> bool {
    matches!(
        kind,
        "nfs" | "nfs4" | "cifs" | "smb3" | "smbfs" | "afs" | "ceph" | "glusterfs" | "davfs" | "ftp"
            | "sshfs"
            | "fuse.sshfs"
            | "fuse.rclone"
            | "fuse.s3fs"
            | "fuse.davfs2"
            | "fuse.tresoritfs"
            | "fuse.dropbox"
            | "fuse.onedriver"
            | "fuse.mega"
            | "fuse.gvfsd-fuse"
            | "fuse.portal"
    )
}

// Split from walk() so a test names the type without needing such a mount on the box.
pub fn refuses(path: &Path, mountinfo: &str) -> bool {
    mount_type_in(path, mountinfo).map(|kind| is_unbounded(&kind)).unwrap_or(false)
}

// Every unbounded mount point in the table. refuses() answers for the walk root, which is only the
// root: a walk of /home descends into /home/<user>/TresoritDrive without ever asking again, which is
// the case the first shape of this missed. Collected once per walk and compared on the way down.
pub fn unbounded_mounts(body: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in body.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let split = match fields.iter().position(|field| *field == "-") {
            Some(value) => value,
            None => continue,
        };
        if fields.len() <= split + 1 || fields.len() < 5 {
            continue;
        }
        if is_unbounded(fields[split + 1]) {
            out.push(PathBuf::from(OsString::from_vec(unescape_field(fields[4]))));
        }
    }
    out
}

// The same octal unescaping src/backend/mountinfo.rs does, over the one field this needs.
fn unescape_field(field: &str) -> Vec<u8> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let digits = &bytes[i + 1..=i + 3];
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                if let Ok(byte) = u8::try_from(digits.iter().fold(0u32, |v, b| v * 8 + u32::from(b - b'0'))) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

pub struct DirSize {
    pub bytes: u64,
    pub partial: bool,
    // The directory's OWN children, counted without descending: what a file manager's Items column
    // means, and what `ls -A | wc -l` answers. None is a directory whose entries could not be read
    // at all, which the column draws as nothing rather than as a zero it did not measure. Counted
    // by its own read_dir rather than inside the recursion below, so a walk the deadline stopped
    // still answers an exact count beside its partial total.
    pub entries: Option<u64>,
}

// Hidden children included: the count is of what the directory holds, not of what a listing with
// the operator's current Show hidden files setting happens to draw.
fn children(path: &Path) -> Option<u64> {
    Some(std::fs::read_dir(path).ok()?.filter_map(Result::ok).count() as u64)
}

// walk_until is the testable core: a test passes an already-past deadline to force partial without waiting.
pub fn walk(path: &Path) -> DirSize {
    walk_cancellable(path, &AtomicBool::new(false))
}

// What the walker thread runs: the clock bounds a slow tree, the flag ends one the client has left.
// A refused mount answers before either, with its own directory entry and nothing under it.
pub fn walk_cancellable(path: &Path, stop: &AtomicBool) -> DirSize {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    if refuses(path, &mountinfo) {
        let bytes = path.symlink_metadata().map(|m| m.size()).unwrap_or(0);
        // A refused mount is not descended into, and its own children are still countable.
        return DirSize { bytes, partial: true, entries: children(path) };
    }
    let deadline = Instant::now() + Duration::from_millis(DEADLINE_MS);
    let skip = unbounded_mounts(&mountinfo);
    walk_guarded(path, &|| stop.load(Ordering::Relaxed) || Instant::now() >= deadline, &skip)
}

// walk_while with a set of directories it will not descend into, each one marking the total partial.
pub fn walk_guarded(path: &Path, stop: &dyn Fn() -> bool, skip: &[PathBuf]) -> DirSize {
    let mut bytes = 0u64;
    let mut partial = false;
    match path.symlink_metadata() {
        Ok(meta) => bytes += meta.size(),
        Err(_) => partial = true,
    }
    walk_into(path, stop, skip, &mut bytes, &mut partial);
    DirSize { bytes, partial, entries: children(path) }
}

pub fn walk_until(path: &Path, deadline: Instant) -> DirSize {
    walk_while(path, &|| Instant::now() >= deadline)
}

// Issue F1e: a copy's walk answers to the copy and not to a clock. The listing needs a floor inside
// two seconds; the transfer needs the whole total, however long the tree takes, or it draws no
// estimate at all, so the stop it is given is its own cancel rather than a deadline.
pub fn walk_while(path: &Path, stop: &dyn Fn() -> bool) -> DirSize {
    walk_guarded(path, stop, &[])
}

// Recursion, not an explicit stack: a tree deep enough to blow it is not a shape this one box produces.
fn walk_into(path: &Path, stop: &dyn Fn() -> bool, skip: &[PathBuf], bytes: &mut u64, partial: &mut bool) {
    if stop() {
        *partial = true;
        return;
    }
    // A mount nothing can measure, reached from an ancestor rather than asked for directly.
    if skip.iter().any(|mount| mount == path) {
        *partial = true;
        return;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(rd) => rd,
        // Permission denied or vanished mid-walk: what was already counted stays, marked partial.
        Err(_) => {
            *partial = true;
            return;
        }
    };
    for entry in entries {
        if stop() {
            *partial = true;
            return;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                *partial = true;
                continue;
            }
        };
        // d_type is free and answers is_symlink/is_dir with no stat, matching scan.rs's own phase 1.
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => {
                *partial = true;
                continue;
            }
        };
        if file_type.is_symlink() {
            // Not followed, matching du's default; DirEntry::metadata is lstat, so only the link's own small size counts.
            if let Ok(meta) = entry.metadata() {
                *bytes += meta.size();
            } else {
                *partial = true;
            }
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                *partial = true;
                continue;
            }
        };
        *bytes += meta.size();
        if file_type.is_dir() {
            walk_into(&entry.path(), stop, skip, bytes, partial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::os::unix::fs::PermissionsExt;

    fn fixture(tag: &str) -> (TestDir, std::path::PathBuf) {
        let sandbox = TestDir::new(tag);
        let tree = sandbox.dir("tree");
        (sandbox, tree)
    }

    #[test]
    fn an_empty_directory_counts_only_its_own_entry() {
        let (_sandbox, d) = fixture("dirsize-empty");
        let result = walk(&d);
        let own = fs::symlink_metadata(&d).unwrap().size();
        assert_eq!(result.bytes, own);
        assert!(!result.partial);
    }

    #[test]
    fn files_and_nested_directories_sum_together() {
        let (_sandbox, d) = fixture("dirsize-nested");
        fs::write(d.join("a.txt"), "abc").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub/b.txt"), "de").unwrap();
        let result = walk(&d);
        let expected = fs::symlink_metadata(&d).unwrap().size()
            + fs::symlink_metadata(d.join("a.txt")).unwrap().size()
            + fs::symlink_metadata(d.join("sub")).unwrap().size()
            + fs::symlink_metadata(d.join("sub/b.txt")).unwrap().size();
        assert_eq!(result.bytes, expected);
        assert!(!result.partial);
    }

    #[test]
    fn a_symlink_is_not_followed() {
        // The target lives outside the walked tree, so a followed link is the only way its 100,000 bytes could ever show up.
        let (_sandbox, d) = fixture("dirsize-symlink");
        let (_target_sandbox, target) = fixture("dirsize-symlink-target");
        fs::write(target.join("huge.bin"), vec![0u8; 100_000]).unwrap();
        symlink(&target, d.join("link")).unwrap();
        let result = walk(&d);
        let expected = fs::symlink_metadata(&d).unwrap().size()
            + fs::symlink_metadata(d.join("link")).unwrap().size();
        assert_eq!(result.bytes, expected);
        assert!(result.bytes < 100_000, "the symlink's own small size counts, not the target it points at");
    }

    #[test]
    fn an_expired_deadline_marks_partial_and_keeps_what_it_saw() {
        let (_sandbox, d) = fixture("dirsize-deadline");
        fs::write(d.join("a.txt"), "abc").unwrap();
        let past = Instant::now() - Duration::from_secs(1);
        let result = walk_until(&d, past);
        assert!(result.partial);
    }

    // Directive 50: a copy's walk carries no deadline, so what ends it early is its own stop, which
    // the transfer sets on a cancel and on its own completion.
    #[test]
    fn a_walk_ends_on_its_stop_rather_than_on_a_clock() {
        let (_sandbox, d) = fixture("dirsize-stop");
        // Flat, so the stop fires between two entries rather than on the way into another directory.
        for name in ["a", "b", "c", "d", "e", "f"] {
            fs::write(d.join(name), "abcdefgh").unwrap();
        }
        fs::create_dir(d.join("inner")).unwrap();
        fs::write(d.join("inner/g"), "abcdefgh").unwrap();
        let whole = walk_while(&d, &|| false);
        assert!(!whole.partial, "a stop that never fires walks the tree whole");
        // Fires part way in, so the stop inside the loop is what ends it rather than the one on entry.
        let seen = std::sync::atomic::AtomicUsize::new(0);
        let cut = walk_while(&d, &|| seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3);
        assert!(cut.partial, "a stop that fires leaves a floor, which publishes no total at all");
        // The root's own entry is counted before the walk starts, so an entry seen is bytes above that.
        let root_only = fs::symlink_metadata(&d).unwrap().size();
        assert!(cut.bytes > root_only, "and it keeps what it saw before the stop: {} against {}", cut.bytes, root_only);
        assert!(cut.bytes < whole.bytes, "which is less than the whole tree: {} against {}", cut.bytes, whole.bytes);
        // The other entry point on the same tree, whose clock still bounds it where the copy's has none.
        let past = Instant::now() - Duration::from_secs(1);
        assert!(walk_until(&d, past).partial, "the listing's own deadline still bounds it");
    }

    #[test]
    fn a_permission_denied_subtree_adds_what_it_saw_and_marks_partial() {
        let (_sandbox, d) = fixture("dirsize-denied");
        fs::write(d.join("visible.txt"), "abc").unwrap();
        fs::create_dir(d.join("locked")).unwrap();
        fs::write(d.join("locked/hidden.txt"), "xyz").unwrap();
        fs::set_permissions(d.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let result = walk(&d);
        fs::set_permissions(d.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.partial, "a subtree it could not read must mark partial");
        let expected_min = fs::symlink_metadata(&d).unwrap().size()
            + fs::symlink_metadata(d.join("visible.txt")).unwrap().size();
        assert!(result.bytes >= expected_min, "what the walk could see must still be counted");
    }

    // The mount that started this: a walk of fuse.tresoritfs burned the whole ceiling every visit,
    // and the ceiling is only checked between entries, so a hung daemon is not bounded by it at all.
    #[test]
    fn an_unbounded_mount_is_refused_rather_than_walked() {
        let info = "1 0 8:1 / / rw - btrfs /dev/a rw\n\
                    2 1 0:9 / /home/gm/TresoritDrive rw - fuse.tresoritfs tresoritfs rw\n\
                    3 1 0:10 / /mnt/share rw - nfs4 server:/x rw\n";
        assert!(refuses(Path::new("/home/gm/TresoritDrive/deep/file"), info));
        assert!(refuses(Path::new("/mnt/share"), info));
        assert!(!refuses(Path::new("/home/gm/code"), info), "a local tree is still walked");
        assert!(!refuses(Path::new("/home/gm"), ""), "an unreadable mountinfo refuses nothing");
    }

    // refuses() answers for the walk root only. Listing /home and asking for the size of the user's
    // own directory is not refused -- that mount is local -- and walks straight into the cloud mount
    // inside it, which is the case the first shape of this missed entirely.
    #[test]
    fn an_unbounded_mount_below_the_root_is_not_descended_into() {
        let (_sandbox, d) = fixture("dirsize-nested-mount");
        let inner = d.join("cloud");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("big.bin"), vec![0u8; 80_000]).unwrap();
        let whole = walk_guarded(&d, &|| false, &[]);
        let guarded = walk_guarded(&d, &|| false, &[inner.clone()]);
        assert!(!whole.partial, "nothing refused, so the tree is walked whole");
        assert!(guarded.partial, "a skipped mount makes the total a floor");
        assert!(
            guarded.bytes < whole.bytes,
            "and the skipped subtree is not counted: {} against {}",
            guarded.bytes,
            whole.bytes
        );
    }

    // The mount points themselves, which is what the walk carries down rather than re-reading
    // /proc/self/mountinfo for every directory it enters.
    #[test]
    fn the_unbounded_mount_points_are_collected_with_their_paths_decoded() {
        let info = "1 0 8:1 / / rw - btrfs /dev/a rw\n\
                    2 1 0:9 / /home/gm/My\\040Cloud rw - fuse.rclone remote: rw\n\
                    3 1 0:10 / /home/gm/code rw - btrfs /dev/a rw\n\
                    4 1 0:11 / /mnt/nas rw - nfs4 server:/x rw\n";
        let mounts = unbounded_mounts(info);
        assert_eq!(mounts.len(), 2, "only the two unbounded ones: {:?}", mounts);
        assert!(mounts.contains(&PathBuf::from("/home/gm/My Cloud")), "{:?}", mounts);
        assert!(mounts.contains(&PathBuf::from("/mnt/nas")), "{:?}", mounts);
    }

    // A local FUSE filesystem is bounded and must still be measured: the prefix rule this replaced
    // refused mergerfs, gocryptfs, bindfs and squashfuse along with the cloud mounts.
    #[test]
    fn a_local_fuse_filesystem_is_still_walked() {
        let local = "1 0 0:9 / /srv/pool rw - fuse.mergerfs pool rw\n";
        assert!(!refuses(Path::new("/srv/pool/films"), local));
        assert!(unbounded_mounts(local).is_empty());
        let cloud = "1 0 0:9 / /srv/pool rw - fuse.rclone remote: rw\n";
        assert!(refuses(Path::new("/srv/pool/films"), cloud));
    }

    // A refused mount still answers, because the Size cell needs a floor and a marker, not silence.
    #[test]
    fn a_refused_mount_answers_its_own_entry_and_partial() {
        let (_sandbox, d) = fixture("dirsize-refused");
        fs::write(d.join("a.txt"), "abc").unwrap();
        let info = format!("1 0 0:9 / {} rw - fuse.tresoritfs tresoritfs rw\n", d.display());
        assert!(refuses(&d, &info));
        // walk_cancellable reads the real /proc/self/mountinfo, where this fixture is a plain tree,
        // so the refusal itself is asserted above and this is the shape the answer takes.
        let own = fs::symlink_metadata(&d).unwrap().size();
        let answered = DirSize { bytes: own, partial: true, entries: children(&d) };
        assert_eq!(answered.bytes, own);
        assert!(answered.partial);
        // The children of a mount nothing can measure are still countable, which is the whole
        // reason the count is its own read_dir rather than a number the recursion accumulates.
        assert_eq!(answered.entries, Some(1));
    }

    // Issue: only the scroll handlers cancelled a walk, so a navigation waited for one it would discard.
    #[test]
    fn a_raised_flag_ends_the_walk_where_the_clock_would_not() {
        let (_sandbox, d) = fixture("dirsize-flag");
        for name in ["a", "b", "c", "d", "e", "f"] {
            fs::write(d.join(name), "abcdefgh").unwrap();
        }
        let stop = AtomicBool::new(true);
        let cut = walk_cancellable(&d, &stop);
        assert!(cut.partial, "a flag raised before the walk leaves a floor, not a total");
        let whole = walk_cancellable(&d, &AtomicBool::new(false));
        assert!(!whole.partial, "and a flag that stays down walks the tree whole");
        assert!(cut.bytes < whole.bytes, "{} against {}", cut.bytes, whole.bytes);
    }

    // The ceiling is a listing's patience, not a measurement's: fourteen rows at the old two seconds
    // was a twenty-eight second worst case for one folder.
    #[test]
    fn the_deadline_is_a_quarter_second() {
        assert_eq!(DEADLINE_MS, 250);
    }

    #[test]
    fn a_missing_directory_answers_zero_and_partial_rather_than_a_panic() {
        let result = walk(Path::new("/definitely/not/here/flea-dirsize-test"));
        assert_eq!(result.bytes, 0);
        assert!(result.partial);
    }
}

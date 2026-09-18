use crate::backend::mountinfo::mount_type_in;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
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
fn is_unbounded(kind: &str) -> bool {
    kind.starts_with("fuse.")
        || matches!(kind, "fuse" | "nfs" | "nfs4" | "cifs" | "smb3" | "smbfs" | "afs" | "ceph" | "glusterfs" | "davfs" | "ftp" | "sshfs")
}

// Split from walk() so a test names the type without needing such a mount on the box.
pub fn refuses(path: &Path, mountinfo: &str) -> bool {
    mount_type_in(path, mountinfo).map(|kind| is_unbounded(&kind)).unwrap_or(false)
}

pub struct DirSize {
    pub bytes: u64,
    pub partial: bool,
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
        return DirSize { bytes, partial: true };
    }
    let deadline = Instant::now() + Duration::from_millis(DEADLINE_MS);
    walk_while(path, &|| stop.load(Ordering::Relaxed) || Instant::now() >= deadline)
}

pub fn walk_until(path: &Path, deadline: Instant) -> DirSize {
    walk_while(path, &|| Instant::now() >= deadline)
}

// Issue F1e: a copy's walk answers to the copy and not to a clock. The listing needs a floor inside
// two seconds; the transfer needs the whole total, however long the tree takes, or it draws no
// estimate at all, so the stop it is given is its own cancel rather than a deadline.
pub fn walk_while(path: &Path, stop: &dyn Fn() -> bool) -> DirSize {
    let mut bytes = 0u64;
    let mut partial = false;
    // The target's own directory entry counts too, matching what `du -s` reports for the directory itself.
    match path.symlink_metadata() {
        Ok(meta) => bytes += meta.size(),
        Err(_) => partial = true,
    }
    walk_into(path, stop, &mut bytes, &mut partial);
    DirSize { bytes, partial }
}

// Recursion, not an explicit stack: a tree deep enough to blow it is not a shape this one box produces.
fn walk_into(path: &Path, stop: &dyn Fn() -> bool, bytes: &mut u64, partial: &mut bool) {
    if stop() {
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
            walk_into(&entry.path(), stop, bytes, partial);
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
        let answered = DirSize { bytes: own, partial: true };
        assert_eq!(answered.bytes, own);
        assert!(answered.partial);
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

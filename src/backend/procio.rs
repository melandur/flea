// How many bytes a process tree has read so far, which is how an archive job reports progress: bsdtar
// prints nothing while it works, but the kernel counts every byte it reads. The tree, because the
// child we spawn is prlimit-then-bwrap and the tool itself is two forks further down, inside the
// jail's own PID namespace, where /proc from outside still sees it under its outer pid.
use std::path::Path;

// Sample /proc/<pid>/io: "rchar: 411717444\nwchar: 1220\nsyscr: 12573\n..."
fn rchar_of(pid: u32) -> u64 {
    let text = std::fs::read_to_string(format!("/proc/{}/io", pid)).unwrap_or_default();
    text.lines()
        .find_map(|l| l.strip_prefix("rchar:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

// Every pid below this one, read from each thread's children list rather than by scanning all of
// /proc, so the cost is the tree and not the box.
fn children_of(pid: u32, out: &mut Vec<u32>) {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{}/task", pid)) else { return };
    for task in tasks.flatten() {
        let list = std::fs::read_to_string(task.path().join("children")).unwrap_or_default();
        for child in list.split_whitespace().filter_map(|c| c.parse::<u32>().ok()) {
            out.push(child);
            children_of(child, out);
        }
    }
}

// corner: rchar counts every read(2), so the loader's and bwrap's own reads are in it too. They are
// tens of kilobytes against a selection measured in megabytes, and the caller never reports past
// the total it was given, so they move nothing anyone can see.
pub fn tree_rchar(pid: u32) -> u64 {
    let mut pids = vec![pid];
    children_of(pid, &mut pids);
    pids.into_iter().map(rchar_of).sum()
}

// The bytes a compress of these names under parent will read: every regular file, walked without
// following a symlink, because bsdtar stores a link as a link and reads nothing behind it.
pub fn selection_bytes(parent: &Path, names: &[String]) -> u64 {
    let mut stack: Vec<std::path::PathBuf> = names.iter().map(|n| parent.join(n)).collect();
    let mut total = 0;
    while let Some(path) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.is_file() {
            total += meta.len();
        } else if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // A grandchild's reads are what a jailed tool's look like from here, so the test reads through
    // a shell that forks the reader rather than reading in the child itself. The reader stays alive
    // after its read, as bsdtar does mid-job: a reaped process's count leaves the tree with it.
    #[test]
    fn a_grandchilds_reads_are_counted_in_the_tree() {
        let d = TestDir::new("procio");
        let file = d.file("five.bin", &"x".repeat(5 * 1024 * 1024));
        let reader = format!("import time; open('{}', 'rb').read(); time.sleep(2)", file.display());
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("/usr/bin/python3 -c \"{}\"; true", reader)])
            .spawn()
            .expect("sh");
        let mut seen = 0;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            seen = tree_rchar(child.id());
            if seen >= 5 * 1024 * 1024 { break; }
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(seen >= 5 * 1024 * 1024, "the tree read the whole file, saw {}", seen);
    }

    #[test]
    fn a_selection_counts_its_files_and_nothing_behind_a_link() {
        let d = TestDir::new("procsel");
        d.file("a.txt", "12345");
        d.dir("sub/deeper");
        d.file("sub/b.txt", "123");
        d.file("sub/deeper/c.txt", "1");
        std::os::unix::fs::symlink("/usr/bin", d.join("link")).unwrap();
        let names = ["a.txt", "sub", "link", "gone"].map(String::from);
        assert_eq!(selection_bytes(d.path(), &names), 9, "5 + 3 + 1, the link and the missing name nothing");
    }
}

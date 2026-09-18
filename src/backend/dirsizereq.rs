// The dirsize queue and its walker, which runs on a thread of its own rather than on the read loop.
// Until 2026-09-18 walk_one_dirsize ran inline, so a single slow row made the backend deaf to every
// other request: measured on this box, a `list` issued while /home/melandur's queue was walking
// waited 1858.8 ms for an answer whose own work was 0.036 ms. Thumbnails already had this shape, a
// worker and an Event, so this is that shape and not a new one.
use crate::backend::dirsize;
use crate::backend::events::Event;
use crate::backend::proto::dirsized_line;
use crate::backend::state::State;
use crate::backend::timing::since;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

// Answered paths are kept across listings, so returning to a directory costs nothing; bounded because
// a long session walking many trees would otherwise grow it without end. Whole rather than LRU: the
// map exists to make the folder you keep returning to free, and that survives a rare clear.
const REMEMBERED: usize = 512;

// How long an answer is believed. Nothing tells this process that a tree it measured has changed:
// the watch is on the directory being listed, not on what is under each row, so a copy into a
// subfolder moves a number nothing here can see. Keying by path bought back the re-walk on every
// listing and, with it, the only invalidation there was -- a directory measured at 1 kB read back
// as an exact 1 kB after 50 MB was written into it. This is the bound on being wrong: long enough
// that leaving a folder and coming back is still free, short enough to be worth trusting.
const FRESH: Duration = Duration::from_secs(10);

// What the walker hands back. The path travels with it because the listing can change underneath a
// walk, and a row index alone would then name a different directory than the one measured.
pub struct DirSizeDone {
    pub row: usize,
    pub path: PathBuf,
    pub bytes: u64,
    pub partial: bool,
    pub ms: f64,
    pub cancelled: bool,
    // The generation the walk started in. A cancel moves it on, so a result from before that is a
    // result for a question already withdrawn.
    pub generation: u64,
    // The directory's own mtime, read before the walk. A cached answer is re-checked against it, so
    // anything written or removed directly inside that directory retires the measurement at once
    // rather than waiting out FRESH.
    pub mtime: Option<SystemTime>,
}

// The mtime of a directory entry, or None if it cannot be read; None never matches, so a directory
// that has become unreadable is re-walked rather than answered from a measurement of what it was.
fn mtime_of(path: &std::path::Path) -> Option<SystemTime> {
    path.symlink_metadata().and_then(|m| m.modified()).ok()
}

// What the watch saw changed: every row of the directory being listed, because a size that moved is
// a size this process was never told about. The watch is not recursive, so this catches a change in
// the listed directory itself; FRESH above is what bounds everything deeper.
pub fn forget_dirsizes_under(st: &mut State, base: &std::path::Path) {
    st.dirsizes.retain(|path, _| path.parent() != Some(base));
}

// An answer still worth giving back without walking again: measured recently, and of a directory
// whose own mtime has not moved since. The mtime catches the case that matters and FRESH bounds the
// one it cannot -- a file written into a directory changes that directory's mtime, but a file
// written deeper down changes only its own parent's, which nothing here is holding.
fn fresh(st: &State, path: &PathBuf) -> Option<u64> {
    let (bytes, at, mtime) = st.dirsizes.get(path)?;
    if at.elapsed() >= FRESH || *mtime != mtime_of(path) {
        return None;
    }
    Some(*bytes)
}

// Answered rows are re-answered at once, matching thumb's own cache-hit shape; only a directory can be asked for.
pub fn queue_dirsizes(out: &mut BufWriter<io::Stdout>, st: &mut State, rows: &[usize]) {
    for &row in rows {
        if row >= st.listing.len() || !st.listing.is_dir(row) {
            continue;
        }
        let path = st.base.join(st.listing.name(row));
        // Only whole walks are ever remembered, so an answer from here is never partial.
        if let Some(bytes) = fresh(st, &path) {
            writeln!(out, "{}", dirsized_line(row, bytes, false, 0.0)).ok();
            continue;
        }
        if st.dirsize_queue.contains(&row) {
            continue;
        }
        st.dirsize_queue.push(row);
    }
    out.flush().ok();
}

// One directory at a time still, which is the rule AGENTS.md states; what changed is that the one is
// not the read loop. Called after every event, so a queue that gained a row while a walk ran starts
// the next one the moment that walk reports.
pub fn pump_dirsize(out: &mut BufWriter<io::Stdout>, st: &mut State, tx: &Sender<Event>) {
    if st.dirsize_running {
        return;
    }
    // A row the listing has since shrunk past is skipped and the next one tried, not returned on:
    // the loop blocks between events now, so starting nothing here would strand the rest of the
    // queue until some unrelated request happened to arrive. Inline, the spin hid that.
    let row = loop {
        if st.dirsize_queue.is_empty() {
            return;
        }
        let candidate = st.dirsize_queue.remove(0);
        if candidate < st.listing.len() {
            break candidate;
        }
    };
    let path = st.base.join(st.listing.name(row));
    // Re-checked here and not only at enqueue: a row asked for twice while the first walk was out
    // queued twice and was walked twice, 217 ms and then 154 ms for the same directory.
    if let Some(bytes) = fresh(st, &path) {
        writeln!(out, "{}", dirsized_line(row, bytes, false, 0.0)).ok();
        out.flush().ok();
        return pump_dirsize(out, st, tx);
    }
    let stop = Arc::clone(&st.dirsize_stop);
    let generation = st.dirsize_generation;
    let tx = tx.clone();
    st.dirsize_running = true;
    thread::spawn(move || {
        // Read before the walk, so a change made while it ran retires the answer rather than being
        // baked into it.
        let mtime = mtime_of(&path);
        let t = Instant::now();
        let result = dirsize::walk_cancellable(&path, &stop);
        let done = DirSizeDone {
            row,
            path,
            bytes: result.bytes,
            partial: result.partial,
            ms: since(t),
            // Raised by a navigation or a scroll, which means the floor this walk reached answers a
            // question nobody is asking any more; it is neither reported nor remembered.
            cancelled: stop.load(Ordering::Relaxed),
            generation,
            mtime,
        };
        let _ = tx.send(Event::DirSize(done));
    });
}

// The loop's own half: a walk that finished against a listing that has since changed is dropped,
// because the row it names now names a different directory.
pub fn report_dirsize(out: &mut BufWriter<io::Stdout>, st: &mut State, done: DirSizeDone) {
    // A walk from before a cancel: the flag was freed then, and a later walk may already hold it.
    if done.generation != st.dirsize_generation {
        return;
    }
    st.dirsize_running = false;
    if done.cancelled {
        return;
    }
    // A partial is a floor, and which floor depends on what the page cache held that second: the
    // same /usr/lib answered 1.4 GB cold and 3.3 GB warm. Remembering one would freeze an arbitrary
    // number for the session and never try again, so only a whole walk is kept. A refusal is partial
    // too, which is what lets a mount that goes away be measured properly the next time it is asked.
    if !done.partial {
        if st.dirsizes.len() >= REMEMBERED {
            st.dirsizes.clear();
        }
        st.dirsizes.insert(done.path.clone(), (done.bytes, Instant::now(), done.mtime));
    }
    let still_named = done.row < st.listing.len() && st.base.join(st.listing.name(done.row)) == done.path;
    if !still_named {
        return;
    }
    writeln!(out, "{}", dirsized_line(done.row, done.bytes, done.partial, done.ms)).ok();
    out.flush().ok();
}

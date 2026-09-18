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
use std::time::Instant;

// Answered paths are kept across listings, so returning to a directory costs nothing; bounded because
// a long session walking many trees would otherwise grow it without end. Whole rather than LRU: the
// map exists to make the folder you keep returning to free, and that survives a rare clear.
const REMEMBERED: usize = 512;

// What the walker hands back. The path travels with it because the listing can change underneath a
// walk, and a row index alone would then name a different directory than the one measured.
pub struct DirSizeDone {
    pub row: usize,
    pub path: PathBuf,
    pub bytes: u64,
    pub partial: bool,
    pub ms: f64,
    pub cancelled: bool,
}

// Answered rows are re-answered at once, matching thumb's own cache-hit shape; only a directory can be asked for.
pub fn queue_dirsizes(out: &mut BufWriter<io::Stdout>, st: &mut State, rows: &[usize]) {
    for &row in rows {
        if row >= st.listing.len() || !st.listing.is_dir(row) {
            continue;
        }
        let path = st.base.join(st.listing.name(row));
        if let Some(&(bytes, partial)) = st.dirsizes.get(&path) {
            writeln!(out, "{}", dirsized_line(row, bytes, partial, 0.0)).ok();
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
pub fn pump_dirsize(st: &mut State, tx: &Sender<Event>) {
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
    let stop = Arc::clone(&st.dirsize_stop);
    let tx = tx.clone();
    st.dirsize_running = true;
    thread::spawn(move || {
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
        };
        let _ = tx.send(Event::DirSize(done));
    });
}

// The loop's own half: a walk that finished against a listing that has since changed is dropped,
// because the row it names now names a different directory.
pub fn report_dirsize(out: &mut BufWriter<io::Stdout>, st: &mut State, done: DirSizeDone) {
    st.dirsize_running = false;
    if done.cancelled {
        return;
    }
    if st.dirsizes.len() >= REMEMBERED {
        st.dirsizes.clear();
    }
    st.dirsizes.insert(done.path.clone(), (done.bytes, done.partial));
    let still_named = done.row < st.listing.len() && st.base.join(st.listing.name(done.row)) == done.path;
    if !still_named {
        return;
    }
    writeln!(out, "{}", dirsized_line(done.row, done.bytes, done.partial, done.ms)).ok();
    out.flush().ok();
}

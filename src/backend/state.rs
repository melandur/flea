// Everything the read loop holds: the tables it parses once, and the listing state it mutates.
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::mime::Db;
use crate::backend::search::Search;
use crate::backend::thumbspec::Thumbnailers;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

// Read once for the process: a per-window load would put a file read inside the viewport path.
pub struct Tables {
    pub mime: Db,
    pub icons: Names,
    pub aliases: Arc<Aliases>,
    pub thumbs: Arc<Thumbnailers>,
    // A type's Kind text never changes for the process's life, unlike State's per-listing caches; RefCell because the loop is single-threaded.
    pub kinds: RefCell<Kinds>,
    // Probed once at startup, the same discipline thumbspec.rs applies to a thumbnailer's program.
    pub formats: Arc<Formats>,
}

// Everything the loop mutates, gathered so a handler takes one borrow instead of ten arguments.
pub struct State {
    pub listing: Listing,
    pub base: PathBuf,
    // Only the rows a client named, so this never grows with the directory; see AGENTS.md "Thumbnail requests".
    pub asked: Vec<(PathBuf, usize)>,
    pub outstanding: usize,
    // Answered directories, keyed by path rather than by row so a list or a sort does not discard
    // them: returning to a folder used to re-walk it whole, 2679 ms every time on this box. The
    // Instant is what bounds how wrong that can get, because nothing here is told when a tree
    // changes: only a complete walk is kept, and only until it goes stale.
    pub dirsizes: HashMap<PathBuf, (u64, Instant, Option<std::time::SystemTime>)>,
    // Rows still to walk, one at a time; dirsizecancel empties this without touching dirsizes.
    pub dirsize_queue: Vec<usize>,
    // Whether a walker thread is out. One at a time is still the rule; the one is no longer the loop.
    pub dirsize_running: bool,
    // Bumped by every cancel, and carried by the walk it started. A worker wedged in a getdents on a
    // mount that never answers can then be abandoned rather than believed: its result is ignored on
    // the way back, and the flag above is freed at once so the queue is never stranded on it.
    pub dirsize_generation: u64,
    // Raised to end the walk in flight. Replaced rather than lowered on a cancel, so the thread that
    // was told to stop cannot be un-told by the next walk arming the same flag.
    pub dirsize_stop: Arc<AtomicBool>,
    // The subtree walk the loop ticks; None means no search is running.
    pub search: Option<Search>,
    // When the running walk last announced its count, so SEARCH_REPORT can throttle the stream.
    pub search_reported: Instant,
}


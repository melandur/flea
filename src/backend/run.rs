use crate::backend::aliases::Aliases;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::meta::stat_range;
use crate::backend::archive::Formats;
use crate::backend::archivereq::{formats_line, start_archive, start_convert};
use crate::backend::convert;
use crate::backend::peek::peek_line;
use crate::backend::metareq::spawn as spawn_meta;
use crate::backend::opsdispatch::{cancel_transfer, do_mkdir, do_newfile, do_rename, do_undo, report_op, resolve_rows, start_duplicate, start_trash, start_transfer, start_menu_transfer, start_redo, Ops};
use crate::backend::opsreq::OpMsg;
use crate::backend::mime::Db;
use crate::backend::dirsizereq::{forget_dirsizes_under, pump_dirsize, queue_dirsizes, report_dirsize};
use crate::backend::events::{spawn_forwarder, spawn_op_forwarder, spawn_reader, Event};
use crate::backend::fsinfo::{fsinfo_line, read as read_fsinfo};
use crate::backend::fsinfo::dev_of;
use crate::backend::listpaths;
use crate::backend::proto::{error_line, error_line_with_mode, listed_line, parse_request, paths_line, thumbed_line, Request};
use crate::backend::rows::rows_line;
use crate::backend::sandbox;
use crate::backend::scan::{mode_of, scan};
use crate::backend::listing::Listing;
use crate::backend::search::Search;
use crate::backend::state::{State, Tables};
use crate::backend::searchreq::{finish_search, step_search};
use crate::backend::ordering;
use crate::backend::thumbcache::{default_root, Cache};
use crate::backend::thumbreq::{cancel_row, forget_one, report_done, thumb_rows};
use crate::backend::thumbs::{Done, Pool};
use crate::backend::thumbwrite::sweep_own_temps;
use crate::backend::watch::{changed_line, Watch};
use crate::backend::thumbspec::Thumbnailers;
use crate::error::FleaError;
use crate::heap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Wider pools settle sooner and answer input later, and 4 is the widest that costs neither the first thumbnail nor the scroll; see AGENTS.md "Thumbnail requests".
const THUMB_WORKERS: usize = 4;
// The whole shutdown budget: a running job is killed at the pool's own 20 s deadline, so waiting longer than that can never cut one short.
const DRAIN_LIMIT: Duration = Duration::from_secs(25);

// The loop stops on Quit; every other request continues it, because errors are responses.
#[derive(PartialEq)]
enum Control {
    Continue,
    Quit,
}

// Errors are responses, so the loop never exits on a bad request.
pub fn run() -> i32 {
    // Before the first listing, because a threshold glibc has already ratcheted strands the next arena on the heap.
    heap::pin_mmap_threshold();
    let mut out = BufWriter::new(io::stdout());
    let aliases = Arc::new(Aliases::load());
    let thumbs = Arc::new(Thumbnailers::load(&aliases));
    let tb = Tables {
        mime: Db::load(),
        icons: Names::load(),
        aliases,
        thumbs,
        kinds: RefCell::new(Kinds::new()),
        formats: Arc::new(Formats::probe()),
    };
    let mut st = State {
        listing: Listing::new(),
        base: PathBuf::new(),
        asked: Vec::new(),
        outstanding: 0,
        dirsizes: HashMap::new(),
        dirsize_queue: Vec::new(),
        dirsize_running: false,
        dirsize_generation: 0,
        dirsize_stop: Arc::new(AtomicBool::new(false)),
        search: None,
        search_reported: Instant::now(),
    };

    let (tx, rx) = channel::<Event>();
    let (results, done) = channel::<Done>();
    let (op_tx, op_rx) = channel::<OpMsg>();
    let mut ops = Ops::new(op_tx);
    // The pool shares this process's one parse of both tables rather than reading the same two files again.
    let pool = Pool::new(THUMB_WORKERS, results, default_root(), Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
    let cache = Cache::new();
    // Every thumbnail job fails closed without these two, so the reason is said once here rather than never; see AGENTS.md "Thumbnail sandbox".
    if !sandbox::available() {
        eprintln!("flea: thumbnails are disabled, bwrap or prlimit is not on PATH");
    }
    // The workers hold senders too, so no exit can come from a disconnect and every exit is an explicit event; see AGENTS.md "Thumbnail requests".
    spawn_forwarder(done, tx.clone());
    spawn_op_forwarder(op_rx, tx.clone());
    spawn_reader(tx.clone(), Arc::clone(&ops.live));
    // The walker threads answer on this, so it outlives the move into Watch::start below.
    let walkers = tx.clone();
    // Armed before the first request, so no listing is ever answered with nothing watching it.
    let mut watch = Watch::start(tx);
    loop {
        // The folder-size walk runs on its own thread now, so only a search still makes the loop
        // spin; with neither running this is the plain blocking recv, see docs/protocol.md "dirsize".
        let event = if st.search.is_none() {
            match rx.recv() {
                Ok(e) => e,
                Err(_) => break,
            }
        } else {
            match rx.try_recv() {
                Ok(e) => e,
                // No event waiting, so it is the walker's turn; looping back lets a meanwhile dirsizecancel be seen before the next row.
                Err(TryRecvError::Empty) => {
                    tick_walkers(&mut out, &mut st, &pool);
                    continue;
                }
                Err(TryRecvError::Disconnected) => break,
            }
        };
        match event {
            Event::Request(line) => {
                if handle_line(&line, &mut out, &mut st, &tb, &pool, &cache, &mut ops, &mut watch) == Control::Quit {
                    break;
                }
            }
            Event::Thumb(d) => report_done(&mut out, &mut st, d),
            Event::DirSize(d) => report_dirsize(&mut out, &mut st, d),
            // The one line no client asked for, and only ever for the directory being listed now.
            Event::Changed(wd, sizes) => {
                if watch.is_current(wd) {
                    // The rows of this directory may have changed size, and no measurement here
                    // would know: dropping them is what makes the Size column follow a copy or a
                    // delete instead of showing what was true when the folder was first opened.
                    //
                    // Unless the burst was attrib-only, which cannot move a size at all. That arm
                    // is not a micro-optimisation: a box where one directory in the listing is
                    // chmod'd on a timer (a VM shared folder, measured every three seconds on this
                    // one) otherwise re-walks every folder in the parent for the life of the
                    // window, and the Size column visibly blanks and refills each time.
                    if sizes {
                        let base = st.base.clone();
                        forget_dirsizes_under(&mut st, &base);
                    }
                    say(&mut out, &changed_line(&st.base, sizes));
                }
            }
            Event::Op(m) => report_op(&mut out, &mut ops, m),
            Event::ReadError(e) => {
                // The framing cannot be trusted past a decode failure, so this reports and stops, as before.
                writeln!(out, "{}", error_line(&e)).ok();
                out.flush().ok();
                break;
            }
            Event::Closed => break,
        }
        // After the event and not before it: a list that just landed has already cleared the queue,
        // and a walk that just reported has already lowered the running flag.
        pump_dirsize(&mut out, &mut st, &walkers);
    }
    // Nothing waits on a walker, but a raised flag ends the one in flight inside an entry rather
    // than at the end of whatever tree it is in.
    st.dirsize_stop.store(true, Ordering::Relaxed);
    drain(&mut out, &mut st, &mut ops, &rx, &pool, &cache);
    0
}

// One answer, written and flushed: the four read-only requests below differ only in what they say.
fn say(out: &mut BufWriter<io::Stdout>, line: &str) {
    writeln!(out, "{}", line).ok();
    out.flush().ok();
}

fn handle_line(
    line: &str,
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    tb: &Tables,
    pool: &Pool,
    cache: &Cache,
    ops: &mut Ops,
    watch: &mut Watch,
) -> Control {
    match parse_request(line) {
        Request::Permissions { line } => say(out, &ops.permissions.handle(&line)),
        Request::Picker { line } => {
            let replies = ops.tx.clone();
            ops.picker.get_or_insert_with(|| super::picker::Picker::new(replies)).request(line);
        }
        Request::MenuAction { line, rows } => {
            let paths = resolve_rows(Vec::new(), &rows, &st.base, &st.listing);
            let cursor = crate::json::field_usize(&line, "cursor").map(|index|
                resolve_rows(Vec::new(), &[index], &st.base, &st.listing).into_iter().next().unwrap_or_default());
            super::opsdispatch::request_menu_action(out, ops, line, paths, cursor);
        }
        // Directive 71: a CLI run of a second or more, so it answers on its own thread.
        Request::LocalSend { op, peer, paths, id } => super::localsend::request(op, peer, paths, id, ops.tx.clone()),
        Request::TrashBrowse { line } => {
            let replies = ops.tx.clone();
            ops.trashbrowser.get_or_insert_with(|| super::trashbrowse::TrashBrowser::new(replies)).request(line);
        }
        Request::List { path, first, hidden } => {
            // A new listing replaces whatever the walk was filling, so the walk ends before the scan starts.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            // Before the scan, because a change readdir raced is missing from the rows this answers with.
            watch.begin(Path::new(&path));
            match scan(&path, hidden) {
                Ok((mut l, read_ms)) => {
                    super::picker::filter_listing(&mut l, &tb.mime, line);
                    let (pass_ms, sort_ms) = match ordering::request(&mut l, Path::new(&path), &tb.mime, line) {
                        Ok(timing) => timing,
                        Err(msg) => {
                            watch.abandon();
                            say(out, &error_line(&FleaError { where_: "sort".into(), path: path.clone(), msg: msg.into() }));
                            return Control::Continue;
                        }
                    };
                    // base and listing only move together, so a failed list cannot mix them.
                    st.base = PathBuf::from(&path);
                    st.listing = l;
                    watch.commit();
                    forget_rows(st, pool);
                    // Said once per listing, because a folder nobody can watch goes stale in silence.
                    if watch.refused() {
                        eprintln!("flea: {} will not follow outside changes, inotify refused a watch on it", path);
                    }
                    writeln!(out, "{}", listed_line(st.listing.len(), read_ms + pass_ms, sort_ms, dev_of(&st.base), &st.base.to_string_lossy())).ok();
                    // Rides along unasked: asking costs a 60 ms round trip at first paint.
                    write_window(out, st, 0, first, tb);
                }
                Err(e) => {
                    // The listing did not move, so neither does its watch.
                    watch.abandon();
                    // A typed path reaches the denial with no parent row to remember the mode from,
                    // so the stat that survives the refused read is the pane's only source for it.
                    writeln!(out, "{}", error_line_with_mode(&e, mode_of(&path))).ok();
                }
            }
            out.flush().ok();
        }
        // A set of named paths is not a directory, so the watch stops rather than following its base.
        Request::ListPaths { paths, first } => {
            watch.stop();
            listpaths::answer(out, st, pool, tb, &paths, first, line)
        }
        Request::Window { start, count } => {
            write_window(out, st, start, count, tb);
            out.flush().ok();
        }
        Request::Search { path, query, hidden, shallow } => {
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            st.base = PathBuf::from(&path);
            st.listing = Listing::new();
            // A walk's matches are not a directory either, so nothing is watched until list asks again.
            watch.stop();
            forget_rows(st, pool);
            // The client is told at once that its old rows are gone, then the count grows as matches arrive.
            writeln!(out, "{}", listed_line(0, 0.0, 0.0, dev_of(&st.base), &st.base.to_string_lossy())).ok();
            st.search = Some(Search::new(&path, &query, hidden, shallow));
            st.search_reported = Instant::now();
            out.flush().ok();
        }
        Request::SearchCancel => {
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
        }
        Request::Sort { by, desc: _ } => {
            // The walk owns the listing sort would reorder, so it ends first rather than racing it.
            if finish_search(out, st, true) {
                forget_rows(st, pool);
            }
            // A key that names no order is refused by name, so a client's sort mark can only describe the order it got.
            match ordering::request(&mut st.listing, &st.base, &tb.mime, line) {
                Err(msg) => {
                    let e = FleaError { where_: "sort".to_string(), path: by.clone(), msg: msg.to_string() };
                    writeln!(out, "{}", error_line(&e)).ok();
                }
                Ok((pass_ms, sort_ms)) => {
                    forget_rows(st, pool);
                    writeln!(out, "{}", listed_line(st.listing.len(), pass_ms, sort_ms, dev_of(&st.base), &st.base.to_string_lossy())).ok();
                }
            }
            out.flush().ok();
        }
        Request::Thumb { rows } => {
            thumb_rows(out, &rows, st, tb, pool, cache);
            out.flush().ok();
        }
        Request::ThumbCancel { rows } => {
            if rows.is_empty() {
                // An empty rows cancels everything queued, and every job it drops has to leave the map with it; see AGENTS.md "Thumbnail requests".
                for job in pool.cancel_all() {
                    forget_one(st, &job.path);
                }
            } else {
                for row in &rows {
                    cancel_row(st, pool, *row);
                }
            }
        }
        Request::DirSize { rows } => {
            queue_dirsizes(out, st, &rows);
        }
        // No rows form: a stale row from a scrolled-past viewport would delay the rows the new one wants, see docs/protocol.md "dirsizecancel".
        Request::DirSizeCancel => cancel_dirsizes(st),
        Request::Transfer { op, paths, rows, dest, menu_id, shelf } => {
            if !shelf.is_empty() {
                crate::backend::shelfdrop::start(out, ops, &shelf, &dest)
            } else if menu_id != 0 {
                start_menu_transfer(out, ops, &op, menu_id, &dest)
            } else {
                let named = resolve_rows(paths, &rows, &st.base, &st.listing);
                start_transfer(out, ops, &op, named, &dest)
            }
        }
        Request::TransferCancel { id } => cancel_transfer(ops, id),
        Request::Trash { paths, rows, menu_id } => {
            let named = resolve_rows(paths, &rows, &st.base, &st.listing);
            start_trash(out, ops, named, menu_id)
        }
        Request::Rename { path, to, menu_id } => {
            if menu_id == 0 { do_rename(out, ops, &path, &to); }
            else { super::opsdispatch::do_menu_rename(out, ops, &path, &to, menu_id); }
        }
        Request::MkDir { path, name } => do_mkdir(out, ops, &path, &name),
        Request::NewFile { path, name, id } => do_newfile(out, ops, &path, &name, id),
        Request::Duplicate { path, menu_id } => start_duplicate(out, ops, &path, menu_id),
        Request::Undo => do_undo(out, ops),
        Request::Redo => start_redo(out, ops),
        // Never touches st.listing, which is the whole point: a column is not the pane's own listing.
        Request::Peek { path, first, hidden, focus } =>
            say(out, &peek_line(&path, first, hidden, &focus, &tb.mime, &tb.icons)),
        // A compress names absolute paths and no path; an extract names the one archive in path.
        Request::Archive { op, paths, path, dest, format, menu_id } => start_archive(
            out, ops, Arc::clone(&tb.formats), &op,
            paths, format, PathBuf::from(&path), PathBuf::from(&dest), menu_id),
        Request::Convert { path, dest, strip, menu_id, request_id, check } =>
            start_convert(out, ops, PathBuf::from(&path), PathBuf::from(&dest), strip, menu_id, request_id, check),
        Request::Formats { id } => {
            let mut line = formats_line(&tb.formats, convert::available());
            line.insert_str(line.len() - 1, &format!(r#", "id":{},"providers":{}"#, id, super::providers::facts()));
            say(out, &line);
        }
        Request::FsInfo => say(out, &fsinfo_line(&read_fsinfo(&st.base), &st.base.to_string_lossy())),
        // One row, only when a client asked: the same no-sweep rule thumb and dirsize already follow.
        Request::Meta { row, text, media, archive, token } => {
            if row < st.listing.len() {
                let want = if archive { Some(Arc::clone(&tb.formats)) } else { None };
                spawn_meta(row, st.base.join(st.listing.name(row)), text, media, want, token, ops.tx.clone())
            }
        }
        Request::Paths { rows } =>
            say(out, &paths_line(&resolve_rows(Vec::new(), &rows, &st.base, &st.listing))),
        Request::Locate { path } => {
            let index = st.listing.index_of(&st.base, Path::new(&path));
            say(out, &super::proto::located_line(&st.base.to_string_lossy(), &path, index));
        }
        Request::LocateMany { paths, id, menu_id, transfer_id } => {
            let mut matches = st.listing.indices_of(&st.base, &paths);
            let error = if transfer_id > 0 {
                if ops.transfer_retry.0 != transfer_id {
                    Some("Transfer retry identities expired; select the items again.".to_string())
                } else {
                    super::opsreq::retain_retry(&ops.transfer_retry.1, &mut matches);
                    None
                }
            } else if menu_id == 0 { None } else {
                ops.menuactions.as_ref().ok_or_else(|| "Deletion survivor identities expired; select the items again.".to_string())
                    .and_then(|menu| menu.retain_survivors(menu_id, &mut matches)).err()
            };
            if error.is_some() { matches.clear(); }
            say(out, &super::proto::located_many_line(&st.base.to_string_lossy(), id, transfer_id, &matches, error.as_deref()));
        }
        Request::Quit => return Control::Quit,
        // corner: an unrecognised line is answered with silence, see AGENTS.md.
        Request::Unknown => {}
    }
    Control::Continue
}

// Ends the walk in flight as well as the queue behind it. The flag is replaced rather than lowered,
// because the thread that was told to stop holds the old one: lowering it would un-tell that thread
// and let a walk the client has navigated away from go on answering.
pub fn cancel_dirsizes(st: &mut State) {
    st.dirsize_stop.store(true, Ordering::Relaxed);
    st.dirsize_stop = Arc::new(AtomicBool::new(false));
    // The generation moves, so the walk this cancelled is abandoned rather than waited for: a worker
    // stuck in a call that never returns would otherwise hold dirsize_running true for the life of
    // the process, and no dirsize would ever be answered again. Every list and every navigation
    // reaches here, so the queue always has a way back.
    st.dirsize_generation = st.dirsize_generation.wrapping_add(1);
    st.dirsize_running = false;
    st.dirsize_queue.clear();
}

// A new row order invalidates every outstanding index, so the queue goes and no result can be reported against the new listing.
pub fn forget_rows(st: &mut State, pool: &Pool) {
    st.outstanding = st.outstanding.saturating_sub(pool.cancel_all().len());
    st.asked.clear();
    // st.dirsizes survives: it is keyed by path, so a sort renames no entry in it and a return to a
    // directory already measured is answered from it rather than walked again.
    cancel_dirsizes(st);
}

// Only the search walk ticks here now; the folder-size walk has a thread and reports as an event.
fn tick_walkers(out: &mut BufWriter<io::Stdout>, st: &mut State, pool: &Pool) {
    // A finished walk hands back its rows in ranked order, which renames every outstanding index.
    if step_search(out, st) {
        forget_rows(st, pool);
    }
}

// A worker inside a child owns a temp file in the shared cache that only its own return publishes or removes; see AGENTS.md "Thumbnail requests".
fn drain(
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    ops: &mut Ops,
    rx: &Receiver<Event>,
    pool: &Pool,
    cache: &Cache,
) {
    let deadline = Instant::now() + DRAIN_LIMIT;
    // A clean shutdown cancels the operation rather than abandoning it: a cancelled copy removes its own
    // partial destination, a file by copy_file and a tree by copy_dir, so quitting leaves nothing behind.
    if let Some(id) = ops.live.running() {
        ops.live.cancel(id);
    }
    while st.outstanding > 0 || ops.live.running().is_some() {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Event::Thumb(d)) => report_done(out, st, d),
            Ok(Event::Op(m)) => report_op(out, ops, m),
            Ok(_) => {}
            Err(_) => break,
        }
    }
    pool.cancel_all();
    // The queue is empty now, so no worker can start a new job and the temps still on disk are exactly the abandoned ones.
    if st.outstanding > 0 {
        sweep_own_temps(&cache.large_dir());
    }
    // corner: a row the deadline cut short is answered empty rather than left unanswered, see AGENTS.md "Thumbnail requests".
    for (_, row) in std::mem::take(&mut st.asked) {
        writeln!(out, "{}", thumbed_line(row, "", 0.0)).ok();
    }
    out.flush().ok();
}

pub fn write_window(out: &mut impl Write, st: &State, start: usize, count: usize, tb: &Tables) {
    let (metas, ms) = stat_range(&st.base, &st.listing, start, count);
    let start = start.min(st.listing.len());
    let mut kinds = tb.kinds.borrow_mut();
    let line = rows_line(&st.listing, &metas, start, ms, &tb.mime, &tb.icons, &tb.aliases, &tb.thumbs, &mut kinds);
    writeln!(out, "{}", line).ok();
}

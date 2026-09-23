# Safety review 2026-09-23 and fix plan

Question asked: is Flea safe to use as an everyday file explorer?

**Answer: yes for local browsing, with known gaps. It is not yet hardened for untrusted downloads or for
cloud and network mounts.** No finding runs attacker code through ordinary browsing of a local folder.
What remains is two ways to lose data (a case-insensitive rclone rename, and a cross-device move
regression), a class of GUI freezes reachable from a file's name or contents, in-process decoders
that still see untrusted bytes (PDF, media cover art), and a boundary with sandboxed apps (the
portal picker) that trusts too much of what the app sends.

Every finding, with its scenario, evidence and suggested fix, is in
[safety-review-2026-09-findings.md](safety-review-2026-09-findings.md). IDs below point there.

## How it was reviewed

- Four earlier reviews covered file operations, untrusted previews, command execution and IPC, and
  network, sharing and secrets. The safety fixes they led to are committed alongside this document.
- A multi-agent workflow then reviewed those fixes through six lenses, and swept ten further areas
  over three rounds until the rounds stopped turning up much new. A completeness critic followed.
- Every finding was attacked by three independent verifiers (correctness, security, reproduction).
  A finding was kept only when most of them failed to refute it. 562 agents in all.
- Result: 41 kept findings on the fixes (`C`), 127 on the rest of Flea (`S`); 7 refuted.
  The lists overlap: several findings are one root cause seen from different sides, and the plan
  below merges them into one ticket each.

| | High | Medium | Low | Info |
|---|---|---|---|---|
| `C` the fixes in this commit | 2 | 19 | 17 | 3 |
| `S` the rest of Flea | 5 | 40 | 73 | 9 |

## Fix plan

Ordered by what to do first. Each ticket names every finding it closes, so ticking the ticket
closes those IDs. A regression from this commit comes before any older issue.

### Phase 0: regressions from this commit

- [ ] **P0.1 Cross-device move: hard links, kept entries and removal errors.**
  C0, C4, C5, C6, C36, C37, S24, S67, S83, S84, S85.
  Unlinking one name of a multi-link inode bumps the other names' ctime, so `movesource.rs` keeps
  them and the move fails every time on trees with hard links (rsync `--link-dest`, `cp -al`,
  deduplicated folders). Any `kept > 0` or removal error then returns `Err` with a complete copy
  that is not journaled.
  Fix: record `nlink` and judge a multi-link inode already unlinked in this pass by kind, length and
  mtime; on FAT/exFAT compare without the inode. Treat `kept > 0` as partial success: journal a
  `Copied` step, report the kept count as a warning, never offer a retry that cannot succeed.
  Count a failed child removal (including `DirectoryNotEmpty`) as kept and keep walking. Take the
  snapshot during the copy instead of in a separate pre-walk, and check the cancel flag.
  Test: two hard links to one file in `movesource`'s tests.
- [ ] **P0.2 Quick Look image render.** C2, C7, C11, C12, C14, C16, C17, C18, C21, C22, C26, C27,
  C29, C33, C34, C39, C40, S32, S89.
  A render killed for the same path (resize, `j` then `k`) latches "This image could not be read."
  with no retry; the old photo stays under the failure sentence; killed runs leak temp files no
  sweep reclaims; 48 full-size renders sit in tmpfs until logout; the jailed output is not checked
  to be a PNG before Qt sniffs it; and a photo now takes 0.9-5 s instead of 10-90 ms.
  Fix: a generation counter in `PreviewImage.qml` (re-run whenever path, size or generation
  changed; only a run that ended on its own can fail); clear the frame on path change; separate
  sentences for "no jailed renderer for this type" and a decoder failure, plus a start-failure
  arm; sweep dead-pid temps and budget the folder by bytes, removed on exit; require the PNG
  signature and IHDR before publishing; render at 2048 unless expanded and emit PNG at compression
  0-1 (or PPM/QOI) inside the jail; pre-render the cursor's neighbours. Add tests for
  `private_dir`/`sweep` and a `--preview-image` refusal case.
- [ ] **P0.3 Spreadsheet helper memory.** C1, C10, S126, C38, C23, C28.
  ODF `<text:s>` repeats expand before the byte budget applies: a 107 KB `.ods` costs `flea --sheet`
  533 MB, a 290 KB `.xlsx` 1.1 GB, about 3 GB within reach, on cursor arrival.
  Fix: cap each cell's accumulator at `cell_chars` bytes before every push; give shared strings a
  count and byte budget. Signal a budget cut the same way as a row cut ("First N rows"). Show an
  ellipsis when `Delimited.js` caps a record. Make the sheet end-to-end test skip without bwrap.
- [ ] **P0.4 Undo to Trash.** C3, C9, C24, C25, C35, S111, S122, S123.
  Undo of a copy spawns three `gio` per item, including two full Trash listings, synchronously on
  the request loop; it fails on GVFS and phones and leaves copies in `.Trash-1000` on sticks;
  production's trash branch is not compiled by any unit test, and `tests/ops.sh` trashes into the
  real session Trash.
  Fix: one `trash::trash` call per journal entry, run on the operation thread with a watchdog.
  Trash only when the item shares the home trash's device; otherwise delete what the undo created
  (identity already checked), and say where it went. Inject the discard step instead of
  `cfg`-swapping it. Run `ops.sh`'s backend under a private `HOME`/`XDG_DATA_HOME` with
  `dbus-run-session`.
- [ ] **P0.5 LocalSend.** S108, C8, C15, S72, C19, C20, C30, C31.
  Discovery still announces on the LAN on every right-click of a fresh install; the inbox is a
  guessable name in shared `/tmp` and files a peer sends during a run are deleted with it.
  Fix: opt-in (ship `localsend` in `menu.hidden`); inbox under `$XDG_RUNTIME_DIR` with a random
  suffix, swept on start, and say when received files were discarded; skip discovery for folder
  rows; reset `_askedAt` when hidden. Move the pure gate and labels into `ui/js/LocalSend.js` with
  tests, and update `tests/ui.sh`'s stub to the new argv.
- [ ] **P0.6 Trash list parsing.** C13, S98, S16.
  A newline-split line's first half still passes as an entry with a forged original, and one bad
  entry disables the whole Trash view, restore, empty and the 30-day sweep.
  Fix: parse per item from raw bytes, split only where the next line starts with `trash:///`,
  cross-check the URI's basename against the original's, and skip (and count) an unreadable item
  instead of failing the listing.
- [ ] **P0.7 Documentation.** C32: add the LocalSend and `Delimited.js` fixes to AGENTS.md's
  design log and reword `kept_error`.

### Phase 1: High

- [ ] **P1.1 Previews only for regular files.** S0, S1, S46, S47, S65.
  A FIFO named `*.pdf` freezes the GUI thread for good when the cursor rests on it; the same under
  a media name wedges teardown, stalls the jailed `ffprobe`, and hangs Convert and Extract.
  Quick Look also ignores the symlink gate the column enforces.
  Fix: in `Facts.state`, `Kinds.quickLookKind` and `Preview.open`, return Unsupported unless
  `(row.p & S_IFMT) === S_IFREG` (for a symlink, the target's mode from the meta reply). Gate
  `metareq.rs`'s probe and `archivework.rs`'s inputs with `regfile::open_if_regular`.
- [ ] **P1.2 Untrusted bytes in the GUI process.** S87, S2, S3, S44, S93, S4, S5, S6, and the
  known "PDFs rendered in-process unjailed" and "`.m3u` may fetch remote URLs".
  Qt Multimedia decodes embedded cover art unjailed (an EPS cover spawns Ghostscript); QtPdf loads
  synchronously on the GUI thread (a broken xref blocks for seconds) and its teardown mid-render is
  still unguarded; a sandboxed app can open a window whose cursor already sits on a PDF it wrote.
  Fix: skip attached-picture decoding in the preview player or route art through the jailed
  thumbnailer; open a PDF in-process only after a jailed check (size ceiling, header, xref) or
  after its jailed thumbnail succeeded; keep one `PreviewPdf` alive until its render completes;
  windows opened by FileManager1 do not auto-preview the first row. Set
  `QT_FFMPEG_PROTOCOL_WHITELIST=file,crypto,data` in `gui.rs`. TUI: validate Sixel frames and do
  not auto-start `mpv` on cursor movement.
- [ ] **P1.3 Deep trees crash the backend.** S103, S104, S68.
  About 1,200 nested folders overflow the 2 MiB thread stack in `copy_dir_at` and abort the whole
  backend; the into-itself guard is string-based, so a bind mount or a different-case spelling on
  FAT reaches it.
  Fix: make `copy_dir_at`/`copy_dir_entries` iterative with an explicit frame stack (as
  trashdelete's review is); compare `(dev, ino)` of the destination's ancestors with the source.
- [ ] **P1.4 Case-insensitive rename on rclone remotes.** S120, S26.
  The copy-then-remove fallback trusts exclusive `mkdir`; Dropbox and OneDrive treat it as
  idempotent and case-insensitive, so renaming `Photos` to `photos` can delete the contents.
  Fix: refuse the fallback when names are equal under case folding or the parent already holds a
  case variant; require the new top folder to be empty before copying; run the fallback on the
  operation thread with progress and cancel.
- [ ] **P1.5 CSV/TSV walk is quadratic.** S125.
  `recordStarts()` re-scans for the newline after every quote that does not open a field: a 1 MiB
  file freezes the GUI for about 14 s. Fix: reuse the next-newline index while it is ahead, stop at
  `MAX_RECORD_CHARS`, and consider a `WorkerScript`.

### Phase 2: wrong file, lost data (Medium)

- [ ] **P2.1 Copy durability.** S41, S76, known "no fsync before removing source" and "failed copy
  leaves truncated file under real name".
  `close(2)` and flush errors are discarded, so a move can delete its source after a write the
  mount reported failed. Fix: `sync_data()` then an explicit, checked close before
  `remove_copied`; write to a temp name and rename into place; the shelf removes or journals
  partials so `z` can clean them.
- [ ] **P2.2 Operations addressed by row index.** S100, S106, S90, known "Delete during a
  background re-list can trash the neighbouring row".
  A search's final rank reorders rows while the client still acts on old indices: Esc then Delete
  trashes a different file. Fix: a listing generation token echoed in `listed`/`searched`/`rows`
  and required on `trash`, `paths`, `transfer` and `menuaction`; the backend refuses a stale one.
  TUI: clear the cursor when a filter hides it.
- [ ] **P2.3 Key auto-repeat on destructive actions.** S27, S69, S107, S92.
  Holding Delete, Enter, Ctrl+Z or Ctrl+V repeats the action; held `x` in Settings strips
  favourites and open rules. Fix: in `Focus.handleKey` and `SettingsPanel`, ignore
  `event.isAutoRepeat` for everything but cursor movement; TUI edge-triggers the same keys, and
  hover never moves keyboard focus onto a destructive choice.
- [ ] **P2.4 Trash and permanent delete.** S57, S58, S15, S17, S59, S60, S112.
  The refreshed confirmation re-targets a reused trash name with no identity check; the
  quarantine lives inside `Trash/files`, so GIO lists it and one survivor breaks the view and the
  sweep; an interrupted delete is recovered only when the Trash view opens.
  Fix: carry the reviewed identity through refresh and refuse a mismatch; move the quarantine to
  `<Trash>/.flea-quarantine/`; run recovery at backend start; offer a resolution for journals
  that fail validation; resolve a symlinked parent before the `O_NOFOLLOW` walk; drop thumbnail
  cache entries after a permanent delete.
- [ ] **P2.5 Drag between two GVFS locations moves.** S121.
  Phone to NAS is treated as one volume, so the originals are removed. Fix: one volume per first
  path component under `/run/user/UID/gvfs`.
- [ ] **P2.6 Partial directory reads look complete.** S99.
  `EIO`/`ENOTCONN` partway through `scan()` shows a short or empty folder. Fix: return the error, or
  a `partial` flag the client draws.
- [ ] **P2.7 Shelf acts on more than was chosen.** S115, S116, S37, S38, S74, S75, S80, S81, S36,
  S39, S40, S51, S77, S78, S79, S117, S118, S119, S52.
  With nothing chosen, Move/Copy/Zip/Send also take pinned rows; the chosen set is re-read when
  the destination is picked and widens to the whole pile; `z` can reverse an old unrelated move;
  pile paths are not identity-checked; zips land in the state folder the README says to `rm -rf`.
  Fix: "whole" means the loose pile only; snapshot the path list at `actOn`; journal drag-out
  moves and bound `undo.json` by age and session; record dev/ino at add and verify before acting;
  write zips somewhere durable and 0600; NUL- or JSON-separated CLI replies; strip only the
  trailing newline from `choose`; refuse symlinks in the plugin refresh and keep user edits.
- [ ] **P2.8 Config writes.** S109, S110, S29.
  One refused open rule makes the launch-time settle erase the whole File types table; a
  symlinked `ui.json` stops the TUI. Fix: drop only invalid entries and back up the original;
  treat a symlink to a regular file as regular; `sync_all()` in `userfile::write_new`.

### Phase 3: hangs on slow mounts and resource use

- [ ] **P3.1 Backend's single request thread.** S42, S101, S18, S63, S19, S21, S62, S20, S102,
  S124, S82, S25, S14, S86.
  A mount that stops answering blocks the only request loop (scan, stat, statfs, search, peek,
  undo of a cross-device move, trash); quit then orphans a backend still transferring.
  Fix: move `scan`, `stat_range`, `peek`, search and undo's reverse copy onto workers with a
  generation counter and watchdog; stop walks at device boundaries (autofs included) and refuse
  unbounded mounts by `st_dev`, not by lexical path; cap search results and guard `Listing`'s u32
  offsets; make duplicate cancellable; give `gio` children and eject/unmount a timeout.
- [ ] **P3.2 Bounded helpers and cleanup.** S64, S66, S22, S61, S88, S7.
  `run_boxed` buffers a jailed tool's stderr with no budget (80 KB archive, hundreds of MB);
  quit orphans `.flea-work-*` folders; inotify overflow drains at 8 KB per 100 ms; closing the
  terminal kills a TUI transfer mid-write. Fix: keep only the last 64 KiB of stderr; register and
  sweep work folders by dead pid; read inotify until `EAGAIN` with a larger buffer; the backend
  ignores SIGHUP/SIGINT and drains on stdin EOF; `gio open` off the TUI loop.

### Phase 4: the boundary with sandboxed apps (portal and picker)

- [ ] **P4.1 Portal picker.** S9, S10, S11, S12, S13, S53, S54, S55, S56, S94, S95, S96, S97, and
  known "FileManager1 leaks path existence and has no window cap".
  An app-supplied folder padded with `/.` hides its real target, so one Enter saves into
  `~/.config/autostart`; any accept label pushes the real Cancel off-screen; requests are not
  bound to `xdg-desktop-portal`; there is no cap on concurrent pickers.
  Fix: `normpath` app-supplied folders and files; elide and sanitise the accept label and refuse
  "Cancel"-like labels; check the sender owns `org.freedesktop.portal.Desktop`; one live chooser
  per app and a small global cap; Enter on a file row in folder mode does nothing; name a
  symlink's target in the save collision review; pass requests through a file instead of one
  environment variable; show resolved targets and parent folders in the footer.

### Phase 5: privacy and local exposure

- [ ] **P5.1 Secrets and state on argv, in files, in dumps.** S28, S71, S114, S33, S35, S113,
  known "passwords cached in memory for the session".
  UI state and a Dropbox share link ride on child argv visible to every local user; the
  production window exposes a read-everything IPC seam; a crash core dumps the password cache.
  Fix: send JSON and the share link on stdin; load `Flea.Ipc` only when `FLEA_IPC=1`;
  `RLIMIT_CORE=0` or `PR_SET_DUMPABLE=0` in `gui.rs` and the backend; thumbnail cache 0700;
  compress work folders 0700 and archives no wider than their inputs.
- [ ] **P5.2 Share link and remote thumbnails.** S73, S34, S45, known "Copy Share Link is public
  with no confirmation".
  Fix: confirm before publishing a link, with stronger wording for a folder; a "local files only"
  thumbnail setting and no thumbnails for network/FUSE mounts by default; release previews that
  hold a file on a mount before ejecting it.

### Phase 6: the rest (Low and Info)

- [ ] **P6.1 Small correctness fixes.** S8 (`--select file://host/`), S23 (`@` names in bsdtar),
  S105 (7z wildcards and symlinks: `-spd -snl --`), S30 and S70 (Run script reports), S31
  (symlinked scripts), S43 (Ctrl+E opens instead of ejects), S48 (TUI rename writes control
  characters: run names through `render::clean`), S49 (newline in the terminal's directory), S50
  (bulk rename kills a suspended `$EDITOR`), S91 (TUI cut not spent by its paste).
- [ ] **P6.2 Older known items still open.** SSH host-key trust not pinned to the shown
  fingerprint; FTPS/WebDAV certificate warning shown as "first connection"; `--default off` and
  `--picker off` do not restore prior values; bidi and newline filename spoofing; invalid UTF-8
  names collide; extraction has no disk-size limit; sparse files are written full size.

### Phase 7: surface no review covered

Review before calling Flea hardened: `backend/menu_actions.rs`, `menu_registry.rs` and
`providers.rs` (the largest backend code with no sweep); Enter on a downloaded `.desktop`, `.sh` or
AppImage, which `open.rs` hands to `gio open` with no confirmation of Flea's own; the 500 MiB text
preview gate trusting a stale row size; `redo.rs` against the new trash-based undo; `mime.rs`,
`thumbwrite.rs`/`thumbcache.rs`, `packaging/*.service`, `flea.portal` and `tools/flea-gio-auth`.
The critic's full list is at the end of the findings file.

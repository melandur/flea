# Safety review 2026-09-23: every finding

The full record behind [safety-review-2026-09.md](safety-review-2026-09.md). Every entry below survived three independent adversarial verifiers; the refuted ones are not listed. IDs are stable and the plan cites them: `C` findings are about the safety fixes committed alongside this file, `S` findings are about the rest of Flea. Line numbers are as of that commit.

Severity is the verifiers' majority vote. CONFIRMED means a verifier reproduced it or traced it end to end; PLAUSIBLE means the code path is real but it was not driven.


## The safety fixes in this commit

### C16. Killing a same-path render is reported as "This image could not be read." and never retried

**High**, CONFIRMED (votes: Medium, Medium, Medium). `ui/PreviewImage.qml:33`

- **What:** render() cancels an in-flight run with renderer.running = false and relies on onExited to restart, but onExited only restarts when forPath !== root.path; a kill for the SAME path (onRenderSizeChanged at line 42, or the Qt.callLater(render) at line 52 landing after a path change already started a fresh run) reaches line 53 with code 15 and an empty answer, sets renderFailed = true, shows the failure sentence and never renders again for that path.
- **Scenario:** Space on a large photo; while the jailed thumbnailer is still rendering, the window is resized across a 512 px step (Hyprland tiling another window into the workspace, 2560 wide -> 1280 wide: renderSize 3072 -> 1536). render() kills the run, onExited sees forPath === root.path, renderFailed = true: the overlay shows the alert and "This image could not be read." for a perfectly good file until the cursor leaves and comes back. Second path: exit of a stale run queues Qt.callLater(render); followSettle's timer moves path to D and starts D's run in the same event-loop pass; the deferred render() then kills D's own run and onExited marks D failed.
- **Fix:** Track cancellation explicitly: a generation counter (renderer.gen) or a `cancelled` flag set in render() before running = false; in onExited, if cancelled or forPath !== root.path or forSize !== root.renderSize, re-run and return; only a run that finished on its own maps a non-zero exit to renderFailed. Alternatively do not kill on size change at all (the cache is keyed by size, so letting it finish costs nothing).

### C33. Space on a photo now costs 0.9-5 s of glycin decode+PNG encode instead of ~10-90 ms in-process

**High**, CONFIRMED (votes: High, High, High). `src/previewimage.rs:73`

- **What:** Every Quick Look image (and every j/k rest) spawns flea --preview-image, which runs glycin-thumbnailer at 2048-4096 px; glycin decodes the full image and PNG-encodes the result, so the first view of any photo takes 0.85 s (2 MP) to 1.3 s (24 MP) at 1080p/1440p windows and 3.3-5 s at 4K (renderSize 4096, ui/PreviewImage.qml:30), while the old in-process scaled JPEG decode was ~7-94 ms.
- **Scenario:** User on a 4K display presses Space on a 24 MP JPEG: the crawl (Preview.qml:380) shows for ~4.7 s before the image appears; stepping j/k through a photo folder at one press per second, the shown image lags several photos behind because each rest kills the in-flight render and starts a new ~5 s one. At 1080p it is ~1.3 s per photo.
- **Fix:** Keep the jail but stop paying glycin's full-size PNG encode: render through an inner tool that can emit a cheap format (PPM/QOI, or PNG at compression level 0-1) under the same sandbox::wrap, or cap renderSize at 2048 unless the window is expanded; additionally pre-render the cursor's neighbours (row +/-1) in the background after a Space so j/k hits the 2 ms cache path, and consider showing the 256 px cache thumbnail scaled as a placeholder instead of the previous photo while the render runs.

### C0. Cross-device move of a tree holding hard links always ends in a 'kept' error and leaves a source skeleton behind

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/movesource.rs:62`

- **What:** remove_at judges a file unchanged by (dev, ino, kind, len, ctime); unlinking one name of a multi-link inode bumps that inode's ctime (POSIX unlink semantics), so every further hard link to it fails the snapshot compare at line 62 and is kept at line 82-83, together with every directory above it, and the move is reported as failed with a misleading 'appeared or changed there while it was copied' message (kept_error, line 90-97).
- **Scenario:** Move a folder containing a.jpg and sub/a-link.jpg (hard links to one inode) to another filesystem. copy_any succeeds; remove_copied unlinks a.jpg, then sub/a-link.jpg stats with a newer ctime -> kept=1. move_any (copyfile.rs:296-299) returns Err, opsreq::one_item journals nothing (p.partial is None), the item is marked failed, and the user is left with a complete unjournaled copy at the destination plus album/sub/a-link.jpg still in the source. Same path via renamecompat::copy_then_remove (KEPT kind) for the FUSE fallback and for undo of a move. Trees produced by rsync --link-dest, cp -al, jdupes/hardlink dedup or some package caches hit this every time.
- **Fix:** Record nlink in Seen. For an inode with nlink > 1, either (a) verify all of its snapshot paths once and then unlink them all, treating the ctime bump caused by this pass as its own (keep a HashSet<(dev, ino)> of inodes already unlinked and, for those, compare kind+len+mtime instead of ctime), or (b) compare such files on (kind, len, mtime) from the start. Add a test with two hard links to one file to movesource's tests.

### C1. ODF <text:s> repeats expand ~12x in memory before any budget applies; a 256 MiB part can cost ~3 GiB in the unjailed --sheet helper

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/sheetods.rs:105`

- **What:** The 'text.len() >= limits.bytes' guard at line 111 sits after the 's', 'tab', 'line-break' and 'p'/'h' arms, so a <text:s text:c="256"/> element (22 bytes of XML) appends 256 spaces to the cell's text without limit until the cell closes; clip() only cuts the finished cell. The AGENTS.md addition claims the reader is 'held to a budget of 4096 characters a cell and 64 MiB in all', but the per-cell accumulator is bounded only by MAX_PART_BYTES x ~12, and flea --sheet itself runs with no prlimit (only bsdtar is jailed).
- **Scenario:** A crafted .ods (a few hundred KB zipped, content.xml inflating to the 256 MiB cap) or .fods whose one cell holds millions of <text:s text:c="256"/> elements is selected in the preview column; flea --sheet allocates ~3 GiB for `text` plus the 256 MiB part twice (bytes + lossy String) before clipping the cell to 4096 chars, thrashing or being OOM-killed on a modest box. The same class, at a smaller factor, applies to a single huge Tok::Text in xlsx `value` (bounded by 64 MiB + one token, then cloned by shown()).
- **Fix:** Bound the accumulators by what a cell can ever show, not by the output budget: stop appending to `text`/`value` once they exceed limits.cell_chars * 4 bytes (a clipped cell can never use more), and apply that check in every text-producing arm ('s', 'tab', 'line-break', 'p'/'h', Text, Cdata) by hoisting the guard above them; in the 's' arm take n.min(remaining). Consider running flea --sheet under the same prlimit --as as the jailed tools, since it parses attacker-controlled XML in-process.

### C2. A window resize during a Quick Look render kills the render and shows 'This image could not be read.' with no retry

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/PreviewImage.qml:33`

- **What:** render() kills a running renderer and returns without starting a new one, relying on onExited to restart it, but onExited (line 51-55) restarts only when forPath differs from root.path; when the trigger was onRenderSizeChanged the path is unchanged, so the killed run's non-zero exit / empty answer is taken as renderFailed = true and nothing ever re-renders.
- **Scenario:** Open Space on a large photo; while flea --preview-image is still running, the Quick Look window crosses a 512 px step (tiling WM re-lays out, or the window is resized). renderSize changes -> render() sets renderer.running = false and returns -> onExited fires with forPath === root.path -> renderFailed = true -> the alert glyph and 'This image could not be read.' are shown for a perfectly readable image until the user navigates to another file and back.
- **Fix:** Track the size a run was started for (renderer.forSize = root.renderSize) and treat a mismatch like a path mismatch in onExited: `if (renderer.forPath !== root.path || renderer.forSize !== root.renderSize) { Qt.callLater(root.render); return }`. Alternatively set a `restart` flag when render() kills a run and honour it in onExited.

### C4. Hard links inside a moved tree make every cross-device move end in 'kept' with a half-emptied source

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/movesource.rs:62`

- **What:** Seen (lines 14-24) keys an entry on ctime. Unlinking the first path of a multiply-linked inode (remove_file, line 85) bumps the ctime of the surviving links, so when the walk reaches the second path `before.0.get(rel) == Some(&seen(&meta))` is false, it is counted as kept (line 82-83), its directory is not removed, and move_any/copy_then_remove return kept_error: "1 item appeared or changed there while it was copied, so the original was kept beside the copy". Nothing appeared or changed; the source is left half-removed (some files gone, the linked one and its directories remaining) while the destination holds a complete copy. Before the diff remove_dir_all took the whole tree. rsync --link-dest style backups, git worktrees with hard-linked objects, and any tree deduplicated with hardlink/jdupes hit this on every move to another disk.
- **Scenario:** album/a.jpg and album/nested/a-link.jpg are one inode. Move album to another filesystem: copy succeeds; remove walk unlinks a.jpg (ctime of the inode moves), then judges nested/a-link.jpg as changed, keeps it and nested/, and reports the item as failed. Retry hits RENAME_NOREPLACE EEXIST at the destination. The user is left with album/nested/a-link.jpg plus an error that blames a concurrent writer.
- **Fix:** Record nlink in Seen and remember the (dev, ino) pairs this walk has already unlinked; for a later path whose snapshot inode is in that set, judge it by (ino, kind, len, mtime) instead of ctime (mtime is not moved by unlink), or refresh the snapshot ctime of every path sharing that inode after each remove_file via an ino->paths index built in snapshot(). Also word kept_error by cause, since 'appeared or changed' is wrong here.

### C10. ODS <text:s> expansion runs before the byte budget: a 107 KB .ods costs flea --sheet 533 MB, ~3 GB at the part cap

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/sheetods.rs:105`

- **What:** The `Tok::Open { name: "s" }` arm (line 105) expands `<text:s text:c="256"/>` (22 bytes) into 256 spaces before the `_ if text.len() >= limits.bytes` guard at line 111 is consulted, so the new 64 MiB budget does not cover the only ODF construct with a >10x in-memory amplification; the same applies to the `tab`, `line-break` and paragraph `\n` pushes above the guard. `flea --sheet` runs unjailed and without rlimits as a Quickshell child on every Space press over a spreadsheet.
- **Scenario:** An .ods whose content.xml holds one row, one cell, one <text:p> and 2,000,000 <text:s text:c="256"/> deflates to 107 KB. Space on it inflates 44 MB through the jailed bsdtar (fine) and then `rows()` grows `text` to 512 MB before `clip()` cuts the cell to 4096 chars. At the 256 MiB MAX_PART_BYTES cap the same shape (about 620 KB packaged) reaches roughly 3 GiB of RSS in the UI's helper process.
- **Fix:** Cap `text` per cell at a small multiple of `limits.cell_chars` bytes (the cell is clipped to 4096 chars anyway) and check it before every push: move the `_ if text.len() >= …` arm above the `s`/`tab`/`line-break`/`p` arms, or write a `push_bounded` helper used by all of them. Optionally add a `<text:s>` fixture to sheet_tests.rs beside `ods_repeats_stop_at_the_byte_budget…`.

### C11. PreviewImage: a size step while a render runs kills it and latches "This image could not be read" with no retry

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `ui/PreviewImage.qml:33`

- **What:** `render()` (line 33) kills an in-flight run and returns; `onExited` (line 52) only re-renders when `forPath !== root.path`. When the trigger was `onRenderSizeChanged` (line 42) for the same path, the killed run exits non-zero or with an empty answer, line 53 sets `renderFailed = true`, and nothing schedules another render, so the Quick Look shows the failure state until the path changes. Since PreviewColumn/PreviewImage no longer fall back to the original file, the image is simply gone. Separately, line 53 reads `renderer.answer` inside `onExited`, which AGENTS.md line 593 documents as racing the StdioCollector on Quickshell 0.3.1; PreviewTable.qml guards that with two flags, PreviewImage.qml does not.
- **Scenario:** Open a large photo in the Quick Look (renders take hundreds of ms for a 20 MP file), then toggle fullscreen or resize the window across a 512 px step before the render lands. render() kills the child; onExited sees forPath === root.path, code 143, answer "" → renderFailed. The preview reads "This image could not be read." for a perfectly good file.
- **Fix:** Record the size a run was started with (`renderer.forSize`); in onExited, `if (renderer.forPath !== root.path || renderer.forSize !== root.renderSize) { Qt.callLater(root.render); return }`. Adopt PreviewTable's streamDone/exitDone pair so `answer` is only read once both have fired.

### C12. --preview-image publishes the jailed child's output bytes unchecked; Qt then sniffs them in-process

**Medium**, PLAUSIBLE (votes: Medium, Medium, Medium). `src/previewimage.rs:74`

- **What:** Lines 74–77 accept any non-empty temp file as the render and rename it to `<md5>.png`; PreviewImage.qml hands that path to `Image`, and Qt chooses a decoder by content when the PNG handler's canRead fails on a `.png` suffix. The shared thumbnail path in thumbs.rs (line 206) at least requires a PNG signature + IHDR via `stamp()`/`insert_after_ihdr` before publishing; the new path has no such check. The jail exists to contain a compromised decoder, and the temp is the one path such a decoder can write to.
- **Scenario:** An image that exploits a decoder inside the jailed thumbnailer makes the child write `%!PS…` (or any other plugin-sniffable header) into %o. flea renames it, prints its path, the UI's Qt/kimageformats content detection routes it to the EPS plugin and Ghostscript runs unjailed in the UI process — the exact chain the fix set out to close, now with a two-stage prerequisite.
- **Fix:** Before the rename, read the temp and require the 8-byte PNG signature and a well-formed IHDR (thumbwrite's `insert_after_ihdr(..).is_some()` or a small dedicated check); reject otherwise. That pins Qt to libpng for this file.

### C17. Previous photo stays on screen under the failure sentence (and under the loading crawl) for the next file

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/PreviewImage.qml:76`

- **What:** On a render failure root.rendered is left holding the previous file's PNG, so picture.status stays Ready and visible (line 76) while the failure Column draws over it; the same stale frame stays up during the whole render of the next file instead of being hidden as HEAD did (HEAD changed source on every path change, hiding the old frame at once).
- **Scenario:** Space on photo A (rendered), press j onto fake.jpg (junk bytes): status becomes "This image could not be read." and the alert/sentence/name column is drawn, but A's picture remains fully visible behind it because rendered/source never changed. Likewise j from A onto a 40 MB JPEG: A remains on screen for the ~1 s the jailed render takes, with only the crawl (after 150 ms) hinting that anything changed, then a one-frame blank (Loading with shownPath !== path) before B appears.
- **Fix:** Clear the frame when it no longer belongs to the shown path: in onExited on failure set root.rendered = "" (or in render() when the path changes), and/or gate `visible` on `root.renderedFor === root.path || root.shownPath === root.path`. Decide deliberately whether keeping the old frame during the next render is wanted; if so, mark the state so the crawl is not the only cue.

### C18. Every killed render leaves an unsweepable temp file in $XDG_RUNTIME_DIR/flea-preview

**Medium**, CONFIRMED (votes: Low, Low, Low). `src/previewimage.rs:70`

- **What:** The QML kills the in-flight `flea --preview-image` on each path change; the SIGTERM'd process cannot remove the exclusive_temp it created, its name is `.flea-<pid>-<rand>.png` so sweep() (line 111, which only counts 32-hex `.png` names) never removes it, and sweep_own_temps is per-pid so no later process can either. Holding j through a folder of large photos leaves one litter file per kill for the login session on tmpfs (0 bytes, or a partial PNG of up to 4096 px if the thumbnailer was mid-write).
- **Scenario:** Hold j across 200 RAW/JPEG photos with the Space overlay open: each render that is still running when the next row arrives is SIGTERM'd; the runtime dir accumulates ~200 hidden temp files that survive until logout, and the 48-render KEEP cap never sees them.
- **Fix:** In sweep(), also remove `.flea-*` temps not belonging to the current pid and older than a short age (the cache's sweep_own_temps pattern generalised), or have the QML let renders finish instead of killing (the result is cached by path+mtime+size, so a finished render is reused when the user comes back).

### C24. Undo-to-Trash ships only in a branch no unit test compiles; ops.sh cannot tell trash from delete

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/undo.rs:251`

- **What:** discard() puts the production body (trash::trash, the failed>0||entries.is_empty() refusal and its message) under #[cfg(not(test))] and swaps in test_trash::take under cargo test, so the safety property the diff introduces ("undo of a copy goes to the Trash, never a delete") is asserted only against a rename helper; the only run of the shipping branch is tests/ops.sh, whose assertions at lines 100 and 144 check just that the copy is gone, which the old remove() satisfied too. #[cfg(not(test))] is also the first in the repo (grep: only undo.rs:251); the accepted pattern is cfg(test)-only helpers/accessors (shelf.rs:41, uistore.rs:42-52, copyfile.rs:46, kind.rs:19, mediaprobe.rs:40, uischema.rs:222-241, undo.rs:108-114), not a production branch replaced under test.
- **Scenario:** Someone later restores the delete (or breaks the failed>0 refusal so a trash-less filesystem falls through to Ok) inside the cfg(not(test)) block: cargo test stays green because that code is not compiled, undo_tests.rs:117 still finds its .flea-test-trash entry, and tests/ops.sh still passes because 'the copy is gone' is true for a delete as well. The regression ships unnoticed.
- **Fix:** Keep one compiled discard path and inject the trash step instead of swapping it: e.g. a `fn(&[PathBuf]) -> (Vec<Entry>, usize)` field on the journal (or a function parameter) that production sets to trash::trash and tests set to a TestDir-local mover, so a unit test can also feed `(vec![], 1)` and assert the 'could not be moved to the Trash, so undo left it in place' refusal. In tests/ops.sh, after each undo of a copy, assert `gio trash --list` (in the same session) names the copy's original path.

### C25. tests/ops.sh now trashes fixture copies into whatever Trash the inherited session has, and never restores them

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `tests/ops.sh:98`

- **What:** ops.sh starts `target/debug/flea --backend` with the caller's environment (line 37; tools/flea-sandbox-guard only owns FIXTURE_ROOT, and tests/run-all.sh invokes ./tests/ops.sh bare). With this diff the 'duplicate, and undo' (91-100) and 'copy transfer, and undo' (131-145) cases route through discard() -> trash::trash -> gio trash, and unlike the existing trash case (104-114) nothing restores them, so each run leaves 'photo copy.jpg', 'dest/c1.txt' and 'dest/c2.txt' in the operator's real ~/.local/share/Trash (FIXTURE_ROOT is deliberately on the home filesystem, AGENTS.md:3904-3906, which is exactly where gio uses the home trash). The new AGENTS.md sentence 'so no test ever writes to the operator's real Trash' is scoped to cargo test and is false for the suite run-all executes.
- **Scenario:** Operator runs ./tests/run-all.sh as the tree instructs; ops.sh passes; three fixture files from /home/flea-sandbox/flea-ops-test-<pid>/ sit in the real Trash, and after a hundred runs three hundred of them, each with a trashinfo pointing at a directory sandbox_remove already deleted.
- **Fix:** Have ops.sh start the backend the way tests/portal.sh re-execs itself: `env -i PATH=$PATH HOME=$D/home XDG_DATA_HOME=$D/home/.local/share XDG_RUNTIME_DIR=$D/run dbus-run-session -- $BIN --backend`, with $D/home created under FIXTURE_ROOT so gio's home trash lands inside the sandbox; then assert `XDG_DATA_HOME=... gio trash --list` names the undone copy, which also gives finding 1 its production-path test.

### C26. A render killed mid-flight leaks its temp file, and sweep() can never reclaim it

**Medium**, CONFIRMED (votes: Medium, Low, Low). `src/previewimage.rs:114`

- **What:** ui/PreviewImage.qml:33 kills the running `flea --preview-image` on every path (and size) change; the launcher dies before line 80's remove_file(&temp), and sweep() (lines 111-124) only counts 36-character `<md5>.png` names, while a temp is `.flea-<pid>-<16 hex>.png` (27 + pid digits, 34 here), so the leftovers are never removed. Reproduced: three SIGTERMs at 40/80/150 ms each left a 0-byte temp in $XDG_RUNTIME_DIR/flea-preview, and a later successful render left all three in place.
- **Scenario:** User opens Quick Look on a folder of large photos and holds the arrow key; each skipped photo kills the in-flight render and leaves one temp (0 bytes to a partial PNG at up to 4096 px) in the session's tmpfs until logout. The cache side already has thumbwrite::sweep_own_temps for this class of leak; the preview side has nothing.
- **Fix:** In sweep() also remove `.flea-<pid>-*.png` entries whose pid is no longer alive (`/proc/<pid>` absent) or whose mtime is older than TIMEOUT; run that sweep before rendering too, so a leak is bounded by one file. Pin it with a unit test that plants a temp under a dead pid in a TestDir (pass the dir into sweep/render rather than reading XDG_RUNTIME_DIR inside, so the test needs no process-global env).

### C27. A window resize during a render marks the image unreadable with no retry

**Medium**, PLAUSIBLE (votes: Medium, Medium, Low). `ui/PreviewImage.qml:52`

- **What:** render() kills an in-flight run and returns (line 33); onExited restarts only when `renderer.forPath !== root.path` (line 52). When the kill came from onRenderSizeChanged (line 42) the path is unchanged, so line 53 sets renderFailed from the killed run (exit 143, empty stdout) and the pane shows 'This image could not be read.' for a file the jail can render, until the cursor moves to another file.
- **Scenario:** Quick Look opens on a 20 MP photo (render takes a few hundred ms); the user maximises the window or toggles the sidebar, decodeWidth crosses a 512 px step, render() kills the run, onExited sees the same path and reports failure. Preview.qml binds decodeWidth to the whole window (Preview.qml:309-310), so any window resize while a render runs triggers it. The initial open is safe because the size bindings land before the path binding (Preview.qml:311).
- **Fix:** Record why a run was killed: set `renderer.restart = true` in render() before `running = false`, and in onExited `if (renderer.restart || renderer.forPath !== root.path || renderer.forSize !== root.renderSize) { renderer.restart = false; Qt.callLater(root.render); return }`. A tests/ui.sh case that resizes the window while a large fixture renders would pin it.

### C28. The sheet end-to-end test now needs a working bwrap namespace but does not skip like every other jailed test

**Medium**, CONFIRMED (votes: Low, Medium, Low). `src/sheet_tests.rs:128`

- **What:** sheet::read() refuses without bwrap+prlimit and member() runs bsdtar under sandbox::wrap_readonly (sheet.rs:98-100, 133-138), so a_real_xlsx_and_ods_package_read_end_to_end_through_bsdtar is now a jailed test, yet it carries no `sandboxprobe::skipped()` guard, unlike the nine tests that guard exists for (thumbs.rs:326-402, archiveops.rs:144-308, sandbox.rs:245, tui/job.rs:107-121).
- **Scenario:** On a box where bwrap is on PATH but cannot create a user namespace (the container case sandboxprobe.rs was written for), member() returns None, read() answers 'no first sheet in the package', and `.expect("the package reads")` panics: the suite goes red for the box, not for flea.
- **Fix:** Add `if crate::backend::sandboxprobe::skipped() { return; }` before the two package reads; the `.fods` read and the `broken.xlsx` refusal need no jail and can be asserted before the guard.

### C34. A render killed on j/k leaks its temp file in $XDG_RUNTIME_DIR/flea-preview and nothing ever sweeps it

**Medium**, CONFIRMED (votes: Low, Low, Low). `src/previewimage.rs:111`

- **What:** ui/PreviewImage.qml:33 sets renderer.running=false on every path or size change while a render is in flight, which SIGTERMs flea --preview-image before its cleanup (previewimage.rs:79-81) runs; the leftover .flea-<pid>-<hex>.png temp is never removed because sweep() only matches 36-character md5 names and sweep_own_temps runs only for the cache's large dir at backend start (src/backend/run.rs:446).
- **Scenario:** A user browsing a photo folder with Space open steps j/k faster than renders complete (any step within ~1-5 s of the last on a big photo): each step leaves one temp file (0 bytes if glycin had not written yet, a partial PNG of several MB if it was mid-write) in tmpfs for the rest of the login session; over a long session the count is unbounded.
- **Fix:** In sweep(), also remove .flea-*.png temps whose owning pid is gone or whose mtime is older than TIMEOUT (20 s); or have the UI kill with the process's own cleanup in mind by removing temps of dead pids at the next render (thumbwrite::sweep_own_temps only handles the current pid's prefix, so a pid-liveness sweep is needed here).

### C35. Undo of a copy/paste is now three gio spawns per item, synchronous on the backend request thread

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/undo.rs:339`

- **What:** discard() calls trash::trash(&[path]) once per Created/Copied step (undo.rs:125 iterates steps), and each call runs gio trash --list, gio trash -- <path>, gio trash --list (trash.rs:77-86); do_undo (opsdispatch.rs:215-221) runs inline in the run.rs:335 request loop, so no listing, peek or other request is answered until every step is done.
- **Scenario:** User pastes 500 files locally, presses Ctrl+Z: 1,500 gio processes run one after another; at the measured ~41 ms per step with a trash holding 1,500 entries that is ~20 s during which the pane does not navigate or refresh, versus ~500 unlinks (milliseconds) before. The per-step cost grows with the size of the user's Trash because --list prints every entry twice per step.
- **Fix:** Collect the paths of all Created/Copied steps in the entry and make one trash::trash(&paths) call (the function already takes a slice and passes them to a single gio trash -- ...), so an undo costs three spawns total; and/or run undo on the ops thread with the existing busy/Live mechanism so the request loop keeps answering.

### C36. Cross-device move now walks the whole source tree (lstat per entry) before the first byte copies, then again after, holding a per-entry map in memory

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/movesource.rs:40`

- **What:** copyfile.rs:294 and renamecompat.rs:99 call snapshot() before copy_any: walk() does one symlink_metadata plus read_dir per entry into a HashMap<PathBuf, Seen>, and remove_copied() (movesource.rs:59-87) then lstats both the source and the destination for every entry; the transfer's progress sink only fires from copy_file_at, and spawn_total (opsreq.rs:156) is walking the same tree concurrently, so the card shows nothing while two walks run.
- **Scenario:** Moving a 1M-entry photo archive to another disk: ~1.9 s and ~140 MB of backend RSS before copying starts and ~2.2 s of verification afterwards on the fastest local case; on an SMB/NFS/sshfs source at 1-5 ms per stat a 100k-entry folder spends 2-8 minutes silent before the copy begins, with a second full walk (the sweep) competing on the same mount.
- **Fix:** Record Seen for each entry as copy_dir_entries visits it (it already stats every entry it copies) instead of a separate pre-walk, so the snapshot costs no extra syscalls and no time before the first byte; skip the snapshot entirely for a single regular file; and stat the destination only when the source entry is unchanged (it is the rarer check that matters).

### C37. A move that had to keep items returns Err with a complete copy that is neither journaled nor undoable, and the item is offered for a retry that cannot succeed

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/copyfile.rs:296`

- **What:** When remove_copied() reports kept > 0, move_any returns Err(kept_error) even though copy_any completed and p.partial is None; one_item (opsreq.rs:321-330) therefore pushes no Step for the destination, counts the item as failed and adds it to the retry list (opsreq.rs:273-279), which run.rs:368-371 later re-selects for a retry that will rename onto the existing destination.
- **Scenario:** User moves album/ (1,000 photos) to an external drive while a sync client drops one new file into it: the drive gets the full copy, album/ is left as a skeleton holding only the new file, the card reports 1 failed with 'the original was kept beside the copy', Ctrl+Z cannot remove the copy (no journal step), and 'select for retry' then fails with a destination-exists error.
- **Fix:** Treat kept > 0 as a partial success: journal a Copied step for the destination (and Moved steps for what was removed) before returning, report the kept count as a warning on the item rather than a failure, and do not add it to the retry list.

### C38. A legitimate large sheet is cut at the 64 MiB output budget with no 'First N rows' line or any other sign

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/sheet.rs:37`

- **What:** MAX_OUTPUT_BYTES stops sheetxlsx::rows/sheetods::rows (sheetxlsx.rs:128, sheetods.rs:25) and command() writes the truncated CSV with exit 0 and nothing on stderr; ui/PreviewTable.qml:38 shows its footer only when the record walk itself hit maxRecords (1,000,000), so a byte-budget cut is invisible.
- **Scenario:** A 200,000-row x 12-column export (33-char cells) opened in the preview shows rows 1-164,482 and simply ends; the user has no indication that 35,000 rows exist below, and a 20-column sheet is cut sooner still.
- **Fix:** Make the readers return a 'cut' flag when the byte budget (not the row count) stopped them and have command() print a trailing sentinel line or a distinct exit code that PreviewTable turns into the same 'First N rows' footer; clipped cells already carry an ellipsis, rows should be as honest.

### C3. Undo of a multi-file copy now spawns three gio processes per file and lists the whole Trash twice each time

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/undo.rs:253`

- **What:** discard() calls trash::trash(slice of one path) per Copied/Created step, and trash_checked runs `gio trash --list` before (trash.rs:77) and after (trash.rs:86) every call to identify the new entry, so undoing an N-file copy costs 3N gio invocations and 2N full Trash listings, where it was N unlinks before.
- **Scenario:** Copy 2,000 photos, press Ctrl+Z with a Trash holding 10,000 items: ~6,000 gio processes and 4,000 listings parsed, minutes on the ops thread, during which the operations card shows no progress; a failure part-way (e.g. Trash refused on that mount) drops the journal entry with the earlier files already in the Trash.
- **Fix:** Batch: collect every Created/Copied path of the entry that passed its identity checks and call trash::trash once with all of them (it already handles a batch and per-path success), then map entries back to steps; or pass the pre-listing once and only re-list after the whole batch.

### C5. rename-kept sentence and comment now claim the copy is whole in a case where the copy is the stale one

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/js/Errors.js:31`

- **What:** copy_then_remove (renamecompat.rs:113-116) now returns KEPT when remove_copied kept an entry because the source was modified during the fallback copy. In that case the OLD name holds the newer bytes and the NEW name holds a stale copy, but Errors.js replaces the backend message with "Copied, but the old name was only partly removed. Check it." and PaneWire.qml:463 states "The copy is whole and only the name it came from is unknown". The invariant that KEPT means 'destination is authoritative' no longer holds for this path (rclone/megafs directories, GVFS WebDAV, cross-device rename via undo's move_back).
- **Scenario:** A rename on an rclone mount falls back to copy_then_remove; a sync writer appends to a file inside during the copy. remove_copied keeps that file, the UI says the copy is complete and the old name was only partly removed. The user 'checks it' by deleting the old name, losing the appended data that only the old name held.
- **Fix:** Give the changed-during-copy outcome its own kind (e.g. "rename-changed") or pass movesource's message through Errors.js for KEPT, and reword PaneWire's comment so the refresh does not assume the destination is authoritative.

### C6. An entry landing in a directory between its read_dir and remove_dir turns the whole walk into an I/O error instead of a kept count

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/movesource.rs:78`

- **What:** remove_at empties a directory child by child and then calls remove_dir when kept == 0. If a new entry lands after the read_dir iteration finished, remove_dir answers ENOTEMPTY and the error propagates with `?`, aborting the rest of the walk. Nothing is lost (the new entry stays), but move_any then reports an io error ('Directory not empty') for a move whose copy is complete, and directories later in the walk that could have been removed are left behind.
- **Scenario:** Move a folder that a download client is writing into: the last file of a subfolder is removed, a new .part file appears, remove_dir fails ENOTEMPTY, move_any returns Err with no kept count, and other already-copied sibling folders are left in the source too.
- **Fix:** Match remove_dir's error: on ErrorKind::DirectoryNotEmpty return Ok(kept + 1) so the folder is reported as kept like any other late arrival; keep `?` for other errors.

### C7. A window resize mid-render kills the jailed preview and shows the killed run as 'This image could not be read'

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/PreviewImage.qml:33`

- **What:** render() stops a running renderer and returns without restarting it. onExited (lines 52-54) only reschedules a render when `renderer.forPath !== root.path`; when the path is unchanged (the case for onRenderSizeChanged) it sets renderFailed = code !== 0, and a killed process exits non-zero, so a valid image is declared unreadable and nothing retries until the path or the size changes again.
- **Scenario:** Open Quick Look on a large photo (render running), resize the window across a 512 px step: renderSize changes, render() kills the run, exit code is non-zero, the pane shows 'This image could not be read.'
- **Fix:** Record `forSize` beside `forPath` when the process starts, and in onExited call Qt.callLater(root.render) whenever forPath !== root.path OR forSize !== root.renderSize, setting renderFailed only for a run that was not superseded.

### C8. Files a paired peer sends during a Flea-started LocalSend run are accepted and then deleted with the private inbox

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/localsend.rs:98`

- **What:** Every peers()/send() run now receives into a 0700 temp dir that Drop removes with remove_dir_all. localsend-cli auto-accepts from paired devices, so a transfer a peer starts while Flea is discovering succeeds on their side and the payload is destroyed seconds later without any notice on this box. Before the diff it landed in the CLI's default folder (the exposure the diff closes). This is a deliberate trade, but it is the one place in the diff that discards bytes somebody meant to deliver.
- **Scenario:** User opens the context menu (discovery run), a paired phone sends a photo at the same moment; the CLI writes it to /tmp/flea-localsend-<pid>-<n>/, the run ends, the folder is removed; the phone shows 'sent'.
- **Fix:** Before dropping, if the inbox is non-empty move its contents to a stable place (e.g. the pane's current folder with a message) or at least notify 'N files were received during this run and discarded'.

### C13. Trash list validation drops only the second half of a newline-split line; the first half is a well-formed entry with a forged original

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/trash.rs:32`

- **What:** `gio trash --list` prints the original path raw, so a trashed name of the form `<victim>\n…` yields two lines; the fix rejects the second (`--empty\t…`) but the first, `trash:///<forged>\t/dir/<victim>`, passes `is_trash_uri` and `starts_with('/')`. `newest_entry_for` (line 107) takes the first `after` entry whose `original` equals the path, so a batch containing both the victim and the forged name can journal the forged URI against the victim; the victim's own step gets no URI.
- **Scenario:** An extracted archive leaves a directory named `a\n` beside the user's file `a`. The user selects both and presses Delete. gio lists `trash:///a%0A\t/dir/a` and `trash:///a\t/dir/a`; whichever comes first is journaled for `/dir/a`. Ctrl+Z restores the attacker's directory, `/dir/a` stays in the Trash, and undo reports the other step as not restorable.
- **Fix:** Cross-check each candidate: the percent-decoded basename of `uri` must equal the path's basename or its gio `.N` variant (`name.2`, `stem.2.ext`). Additionally treat a line that fails `is_trash_uri` as invalidating the line before it, since that is the signature of a raw newline in an original path.

### C14. A killed --preview-image leaves its exclusive temp in $XDG_RUNTIME_DIR/flea-preview forever; sweep never removes them

**Low**, CONFIRMED (votes: Low, Low, Low). `src/previewimage.rs:70`

- **What:** `exclusive_temp` (line 70) creates `.flea-<pid>-<rand>.png`; nothing removes it if the process dies before its own cleanup, and `sweep()` (lines 111–114) only considers 36-character names, so temps are never swept. PreviewImage.qml kills a running render on every path change or size step while one is in flight, which is the common case when arrowing through photos.
- **Scenario:** Arrow quickly through a folder of large photos in the Quick Look: each superseded render is SIGTERMed mid-decode and leaves a 0-byte (or partially written) temp in the tmpfs runtime dir until logout.
- **Fix:** At the start of `render()` (or in `sweep`), remove `.flea-<pid>-*.png` temps whose pid no longer exists (`/proc/<pid>` absent) or whose mtime is older than a few minutes; `thumbwrite::sweep_own_temps` cannot do this because it keys on the current pid.

### C15. LocalSend inbox lives in the shared temp dir under a guessable name, so a co-located user can make every discovery and send fail

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/localsend.rs:88`

- **What:** `Inbox::new` builds `/tmp/flea-localsend-<pid>-<n>` (line 88) and refuses an existing name via `DirBuilder::create` with 0700, which correctly prevents a symlink or foreign directory from becoming the destination. But the backend pid is visible in `ps` and `n` counts from 0, so another local user can pre-create the next names and turn every LocalSend leg into "could not make its private folder". previewimage.rs already solved the same problem by using $XDG_RUNTIME_DIR.
- **Scenario:** On a shared machine, another account runs `mkdir /tmp/flea-localsend-<backend pid>-{0..50}`; the LocalSend row then never lists a device and every send fails until Flea restarts.
- **Fix:** Create the inbox under `$XDG_RUNTIME_DIR` (fall back to temp_dir only when unset) and add a random suffix from /dev/urandom, as `thumbwrite::exclusive_temp` does.

### C19. LocalSend discovery still runs for folder rows whose LocalSend row can never be used

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/Pane.qml:626`

- **What:** The new hidden-in-Settings gate lives inside LocalSend.refresh(), but refresh is called from onSnapshotRequested for every row menu (ContextMenu.qml:248 fires for any openAt row, directories included) while Pane.qml:632 hands the menu `peers: []` when cursorRow.d, so a right click on a folder starts a localsend-cli run that announces this box and receives for its length, and the row is drawn disabled/errored regardless.
- **Scenario:** Right-click any folder in a listing with localsend-cli installed and the row not hidden: backend.localSend("peers") runs (LAN announcement, private inbox for a second or more), then the menu draws Send with LocalSend red and disabled because peers is forced to [] for a directory.
- **Fix:** Pass the row's eligibility into the gate, e.g. `menuActions.localSend.refresh(menu.localSend.installed && root.cursorRow && !root.cursorRow.d)`, or move the hidden/eligible test into Pane.qml next to the peers expression so both read one rule.

### C20. Un-hiding the LocalSend row within the 15 s warm window leaves it red with no peers

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/LocalSend.qml:34`

- **What:** When hidden, refresh() clears root.peers but keeps _askedAt, so a refresh after the row is re-enabled within warmMs returns at line 39 without asking again; the menu then draws the row disabled and errored (peers empty, checking false) although devices were just found.
- **Scenario:** Open a row menu (devices listed, _askedAt = now), open Settings > Menus and hide Send with LocalSend, open a row menu (peers cleared), un-hide within 15 s, open a row menu: the row is red and disabled until the warm window expires and a further menu open asks again.
- **Fix:** Reset `root._askedAt = 0` in the hidden branch (or do not clear peers there; the row is filtered by applyHidden anyway).

### C21. A type with no thumbnailer (or no sandbox) is blamed as an unreadable image

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/PreviewImage.qml:53`

- **What:** Every non-zero exit of `flea --preview-image` becomes "This image could not be read.", including the refusals 'no thumbnailer renders this type' and 'the sandbox is unavailable'. Files Qt used to decode in-process (xcf, psd, xpm, pcx, icns here; every image on a box without the glycin/evince thumbnailers) now show the alert sentence that says the file is broken; the column says 'no preview could be made'.
- **Scenario:** Space on design.xcf or layers.psd: 'This image could not be read.' although the file is fine and opens in GIMP. On a machine without gnome's glycin thumbnailers even photo.jpg reads that way.
- **Fix:** Collect stderr (StdioCollector on stderr, waitForEnd) and show a distinct sentence for a refusal by type or sandbox ('No jailed renderer for this type.'), keeping 'This image could not be read.' for a decoder failure so tests/ui.sh previewviews keeps its string.

### C22. A renderer that cannot start leaves the overlay in "loading" forever

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/PreviewImage.qml:47`

- **What:** The Process has no onRunningChanged guard; when the program cannot be executed Quickshell emits neither exited nor streamFinished (measured), so renderFailed stays false, rendered stays "", status stays "loading" and the crawl never ends. PreviewTable.qml has the same shape.
- **Scenario:** FLEA_BIN unset/empty and no `flea` on PATH (a dev launch that bypasses src/gui.rs, or the binary removed mid-session): Space on any image shows the crawl indefinitely with no sentence.
- **Fix:** Add `onRunningChanged: if (!running && !renderer.exitedSeen && renderer.forPath === root.path) root.renderFailed = true` (clearing exitedSeen in render()), the pattern ui/ViewState.qml uses for its writer.

### C29. previewimage.rs has no tests and no suite names --preview-image

**Low**, CONFIRMED (votes: Low, Low, Low). `src/previewimage.rs:89`

- **What:** The module has no #[cfg(test)] block, and grep over tests/ finds no 'preview-image': private_dir()'s three refusals (symlink, wrong owner, group/other bits), sweep()'s KEEP=48 and 36-character filter, command()'s argument parsing and the MAX_SIZE clamp are all pure and untested, in a tree whose Testing section makes cargo test the module gate and whose tests/thumbs.sh pins the equivalent fail-closed behaviour for the cache with PATH pointed at an empty directory.
- **Scenario:** A later edit to the 36-character filter or the 0o077 mask silently widens what sweep deletes or what private_dir accepts; nothing red appears. I verified the current behaviours by hand only.
- **Fix:** Split render() so private_dir(runtime: &Path) and sweep(dir) take their directory, then test them with TestDir; add a `tests/sandbox.sh`-style case that runs `flea --preview-image` with PATH empty and asserts exit 1 and no file written.

### C30. Display suites describe the old image path and reject the new localsend-cli argv

**Low**, CONFIRMED (votes: Low, Low, Low). `tests/ui.sh:9045`

- **What:** tests/ui.sh:9045 still says 'PreviewImage reads the file itself' and its three image cases now silently depend on `flea --preview-image` succeeding in the harness (FLEA_BIN, XDG_RUNTIME_DIR, a working jail, a thumbnailer for p.webp and p.heic); tests/ui-providers.sh:200 makes the localsend-cli stub exit 95 unless argv starts with `--port <n>`, while the backend now starts every run with `--destination <dir>` (spawn, localsend.rs:110) and already sent no --port at HEAD, so the one display case for LocalSend cannot exercise the inbox argv and would fail if it ran.
- **Scenario:** tests/ui.sh preview case passes or fails for reasons its comment does not describe; providers_localsend_checks (ui-providers.sh:420-445) gets exit 95 from the stub on the peers run, finds no device row, and fails 'the row is a brand row carrying the device the CLI discovered'.
- **Fix:** Update the ui.sh comment to name the jailed render, and teach the stub to accept `--destination <dir>` first, assert the dir exists, is 0700 and is not under $HOME/Downloads, then the `-f` pairs; that makes the private-inbox fix observable by the one suite that drives the real QML.

### C31. LocalSend Settings gate and duplicate-name labels live in QML, where no test reaches them

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/LocalSend.qml:33`

- **What:** The new gate (lines 31-37, Menu.isHidden(ViewState.menuHidden, 'localsend') stops the discovery run that announces the box) and the '(address)' disambiguation of rows (55-60) are written in the component; only peerId/peerName went into ui/js/LocalSend.js and tests/js/localsend.js. grep over tests/ finds no case naming 'hidden in Settings' or the localsend hidden id, so the one change that stops a network announcement has no test in either tests/js.sh or the display suites.
- **Scenario:** A later refactor of ask() drops the isHidden check or reads the wrong id ('localSend' vs 'localsend'); tests/js.sh stays at 19/0 and the CLI is spawned again from a menu whose row is hidden.
- **Fix:** Move the pure parts into ui/js/LocalSend.js as `rows(list)` -> [{id,label}] and `blocked(installed, hiddenActions)` -> reason string, call them from the component, and add checks to tests/js/localsend.js (two devices named alike get their addresses; a hidden row answers 'hidden in Settings' before any run).

### C39. A render interrupted for the same path (resize across a 512 px step, or j then k back within the kill window) is reported as 'This image could not be read'

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/PreviewImage.qml:51`

- **What:** render() kills an in-flight run and returns (line 33); onExited only re-renders when forPath differs from root.path, otherwise it judges the killed run (nonzero code / empty stdout) as a failure (lines 52-54) and nothing restarts it.
- **Scenario:** With Space open on a 24 MP photo on a 4K display (render ~4.7 s), the tiling WM resizes Flea's window across a 512 px boundary (a new window opens beside it, or the window moves to a 1080p monitor): onRenderSizeChanged -> render() kills the run, the exit is judged failed, and the pane shows the alert glyph and 'This image could not be read.' for a perfectly good file until the user steps away and back.
- **Fix:** Set a 'cancelled' flag in render() when it kills a run and, in onExited, re-run instead of judging when that flag is set (or compare a generation counter), mirroring the forPath !== path branch.

### C40. 48 kept renders can hold 0.3-0.8 GB of RAM-backed tmpfs in $XDG_RUNTIME_DIR until logout

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/previewimage.rs:24`

- **What:** KEEP=48 bounds the count, not the bytes: at 4096 px a smooth 24 MP photo renders to 6.1 MB and a noisy one to 15-17 MB, so a 4K user who has stepped through 48 photos holds up to ~800 MB of shmem in /run/user/<uid>, which logind caps at 10% of RAM by default and which other applications' sockets and files share; the folder is not cleared when Flea exits.
- **Scenario:** On an 8 GB laptop with a 4K display (runtime dir cap ~820 MB), browsing a folder of high-detail photos fills the runtime dir; any other app writing there then fails with ENOSPC, and the memory stays allocated until logout even after Flea closes.
- **Fix:** Budget the folder by bytes (e.g. 256 MB) in sweep(), clear it when the backend exits, and prefer 2048 renders unless the pane is expanded; alternatively write renders into the on-disk thumbnail cache tree (~/.cache/thumbnails/x-large) where the desktop's own 1024 px size already lives.

### C9. Undo of a copy is now refused on any filesystem gio cannot trash on, and on removable media the undone copy stays on the device in .Trash-1000

**Info**, CONFIRMED (votes: Info, Low, Info). `src/backend/undo.rs:253`

- **What:** discard() routes Created/Copied undo through gio trash and refuses on failure. That is the safe direction, but it is a functional regression on FUSE mounts (rclone, sshfs, gvfs, most MTP) where g_file_trash answers NOT_SUPPORTED, and on USB sticks the copy is moved into <mount>/.Trash-1000 so the space the user expected back is not returned. The trash entry is not journaled, so it can only be recovered from the Trash browser.
- **Scenario:** Copy a 20 GB folder onto a nearly full stick, press Ctrl+Z: the folder is moved into /media/stick/.Trash-1000/files, the stick stays full, and the listing no longer shows why.
- **Fix:** Acceptable as designed; consider telling the user where the undone copy went ('moved to the Trash on <device>') and, for mounts where trash is refused, offering the old delete behind an explicit confirmation.

### C23. Record cap silently drops the trailing columns of a legitimate long record

**Info**, CONFIRMED (votes: Info, Low, Low). `ui/js/Delimited.js:80`

- **What:** fields() now stops 65536 chars into a record; a real CSV row with one large cell (embedded base64, JSON, a long description) loses every field after that point for that row, with no mark, while other rows keep their columns.
- **Scenario:** export.csv with a 100 KB JSON blob in column 2 and ids in columns 3-5: that row draws column 2 cut and columns 3-5 empty; sorting/aligning by eye misreads the sheet.
- **Fix:** Append an ellipsis to the last parsed field when the cap cut the record (mirroring sheet.rs clip's '…'), or cap per cell (stop appending to `cell` past N chars but keep scanning delimiters) so column positions survive.

### C32. AGENTS.md prose matches the five documented fixes but omits the LocalSend and Delimited ones, and one user-facing sentence overstates

**Info**, CONFIRMED (votes: Info, Info, Info). `AGENTS.md:3870`

- **What:** Checked against the code: --preview-image line, previewimage.rs entry (0700, 48 kept, no fallback in the columns view), sheet.rs jail and 4096/64 MiB budgets, undo-to-Trash and the cargo-test folder, movesource's (inode, size, ctime) photograph and kept count, keep_times, and the restore check all match. Not recorded anywhere: the private `--destination` inbox, the '<address> <name>' send pin, the Settings gate (AGENTS.md has no LocalSend section at all; 'Directive 71' lives only in code comments), Delimited's MAX_RECORD_CHARS, and closes()'s 110 s linear fix. Separately, movesource.rs:95 tells the user 'the original was kept beside the copy' when only the changed/new entries remain at the source; the AGENTS.md sentence ('Anything new or changed stays') is the accurate one.
- **Scenario:** The next reader of AGENTS.md's LocalSend-free design log does not learn why every CLI run carries --destination and re-measures it; a user reading the move error looks for a whole original that is mostly gone.
- **Fix:** Add a short LocalSend paragraph (inbox, pin, gate, and the ui-providers stub contract) and a Delimited/closes() sentence to the design log; reword kept_error to 'so those stayed at the source beside the complete copy'.


## The rest of Flea

### S0. A FIFO named *.pdf freezes the GUI thread for good the moment the cursor rests on it

**High**, CONFIRMED (votes: High, High, Medium). `ui/js/Facts.js:53`

- **What:** Facts.state routes any row whose name ends in .pdf to the PDF state (only symlinks are checked, line 45; the backend gives a fifo/socket/device the icon of its name, src/backend/rows.rs:280-291), so ui/PreviewColumn.qml:216-218 activates PreviewPdf, ui/PreviewPdf.qml:51-54 opens the first document at once, and ui/PreviewPdf.qml:65 assigns PdfDocument.source, which QtPdf loads synchronously on the GUI thread: QFile::open on a fifo with no writer blocks forever. The Space overlay takes the same route via ui/js/Kinds.js:123 quickLookKind (name only, so a symlink to a fifo also passes) -> ui/PdfViewer.qml:251-256.
- **Scenario:** A tarball holds a fifo member called report.pdf; bsdtar -xf restores it (verified; Flea's extract is plain `bsdtar -x -f … -C …`, src/backend/archive.rs:88-118, and bsdtar has no option to skip fifos). The user extracts it in Flea and arrows onto the row (or enters the folder, where the cursor starts at index 0, ui/js/Nav.js:99; the folder is also restored on the next launch via lastPath). After the 120 ms settle the GUI thread enters open(2) and never returns: no key, click or Escape works until Flea is killed or a writer opens the fifo from a terminal. Repeats on every visit. A stalled network/FUSE mount produces the same block on QFile::open/read for any .pdf on it.
- **Fix:** Gate every reader on the file type before the kind arms: in Facts.state and Kinds.quickLookKind return UNSUPPORTED unless (row.p & S_IFMT) === S_IFREG (the wire already carries mode `p`), and for symlink rows use the target's mode (add a `regular` flag to the meta reply from the stat metareq.rs already does). Mirror the check in ui/Preview.qml open()/follow() so Space refuses too.

### S87. Media preview decodes embedded cover art in-process, unjailed, and can spawn ghostscript on attacker PostScript

**High**, CONFIRMED (votes: High, High, Medium). `ui/PreviewMedia.qml:190`

- **What:** When PreviewMedia.qml's MediaPlayer opens an audio/video file, Qt Multimedia's ffmpeg backend automatically extracts the file's attached cover-art stream and decodes it in the main Flea GUI process through Qt's unjailed image-format plugins; an EPS cover spawns ghostscript on the file's PostScript, and a slow decode wedges the GUI event loop at player teardown.
- **Scenario:** An attacker sends the user an ordinary-looking .mp3/.mp4 (reads fine, ffprobe reports it fine, shows as a normal Audio/Video row). The user presses Space on it (Preview.qml load() sets mediaLoader.source=PreviewMedia.qml, PreviewMedia sets MediaPlayer.source and autoStart=true). Qt's ffmpeg backend reads the attached_pic stream and, WITHOUT any code querying CoverArtImage and without the user pressing play, hands the embedded image bytes to QImage in the main process. Qt loads the whole imageformats plugin set (KImageFormats PSD/XCF/RAW/QOI/EPS, etc.). An APIC frame declared image/jpeg but containing EPS causes kimg_eps.so to launch /usr/bin/gs on the attacker's PostScript, in the Flea GUI process, OUTSIDE the bwrap+prlimit jail that every other untrusted-file tool (thumbnailer, ffprobe, bsdtar) is forced through. Ghostscript has a long CVE history of -dSAFER/-dPARANOIDSAFER sandbox escapes, so this is an attacker-code surface reachable by one keypress on a file with no visible danger. Separately, when the player is torn down (cursor moves to the next row via follow()/load(), or the overlay is closed via close() which sets mediaLoader.source="") while the cover decode is still in flight, the MediaPlayer destructor blocks the GUI event loop until the decode finishes.
- **Fix:** Do not let the live in-process MediaPlayer decode untrusted embedded art. Either disable cover-art decoding for the preview player (e.g. clear/ignore the CoverArtImage metadata path, or set a QtMultimedia option to skip attached_pic), or route media preview art through the same jailed thumbnailer that ordinary image files already use rather than QtMultimedia's in-process decode. At minimum set QT_FFMPEG_PROTOCOL_WHITELIST and cap QImageReader allocation, and disable the KImageFormats EPS/ghostscript handler for in-process decodes (QT_IMAGEIO / removing kimg_eps from the preview process). Also bound the teardown: destroy the player asynchronously so a slow decode cannot freeze the event loop.

### S103. Copying, duplicating or moving a folder about 1,200 levels deep crashes the whole backend (stack overflow) and leaves an unrecorded partial copy

**High**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/copyfile.rs:236`

- **What:** The copy calls copy_at -> copy_dir_at -> copy_dir_entries -> copy_at once per directory level. It runs on a std::thread::spawn thread, which has a 2 MiB stack (opsreq.rs run_transfer_checked via opsdispatch.rs:129, duplicate via opsdispatch.rs:169). Since Issue 110, every child is reached through the short path /proc/self/fd/N/<name>, so PATH_MAX no longer limits the depth. At about 1,175 levels the thread runs out of stack and Rust aborts the whole flea --backend process. It is not a clean error for that one item.
- **Scenario:** Someone sends a small tar or zip holding a/a/a/... about 1,300 levels deep. The full path is only about 2,600 bytes, well under PATH_MAX, so any tool can make it and Flea's own jailed Extract unpacks it cleanly ('archivedone ok:true verified:true'). The user then duplicates the folder, copies it with y/p, or ctrl-drags it. The backend aborts with 'thread has overflowed its stack'. Everything else in flight dies with it: undo history, other operations, and running extract/compress jobs, which leave .flea-work dirs behind. A roughly 1,175-level 'X copy' tree stays on disk with no undo entry, and the window only says 'the backend exited with code …'. Cross-device moves, redo and shelf copies all go through the same copy_any code.
- **Fix:** Make copy_dir_at/copy_dir_entries iterative, keeping an explicit stack of (from fd, into fd, ReadDir) frames, the way trashdelete's review already works. Or cap the depth, say at 1024, and fail that item with a named error that records the partial. At minimum, run transfer and duplicate threads through thread::Builder with a larger stack_size. That only moves the limit.

### S120. Renaming a folder on a case-insensitive rclone remote (Dropbox, OneDrive, Box, SMB) can delete the folder's contents

**High**, PLAUSIBLE (votes: High, High, High). `src/backend/renamecompat.rs:102`

- **What:** The rclone directory-rename fallback relies on create_dir/create_new being exclusive. rclone's VFS looks names up case-sensitively by default on Linux, but backends like Dropbox and OneDrive treat Mkdir as idempotent and case-insensitive. So `mkdir("photos")` next to an existing "Photos" succeeds and returns the SAME remote folder. The fallback then either (a) hits EEXIST on the first child and its cleanup runs remove_any(to), which is remove_dir_all on the target and deletes the original files through the alias, or (b) "copies" every file onto itself, and movesource::remove_copied then deletes the source entries, which are the only copies.
- **Scenario:** rclone mount of Dropbox or OneDrive with default Linux options (--vfs-case-insensitive=false). In Flea the user renames folder "Photos" to "photos", or renames "Old" to "Backup" while a sibling "backup" exists. (1) Kernel LOOKUP("photos") returns ENOENT because the VFS is case-sensitive. (2) renameat2 with RENAME_NOREPLACE on a directory returns EINVAL, as measured in AGENTS.md ~3814. needs_fuse_fallback_in then picks copy_then_remove. (3) copy_dir_at create_dir("photos"): rclone Dir.Mkdir calls f.Mkdir, and the backend (dropbox getDirMetadata, onedrive FindLeaf) finds "Photos" case-insensitively and returns success without creating anything. (4a) create_new("photos/a.jpg") gets EEXIST because the listing of "photos" is the remote "Photos". progress.partial == to, so line 102 runs remove_any(to), a remove_dir_all that unlinks every child and then rmdirs the remote folder. The user sees only "already exists". (4b) If the new VFS dir reads as empty instead, each upload overwrites the same object with itself. remove_copied then finds every source entry unchanged and carried (lstat of photos/x succeeds, same size) and remove_file deletes the only copy. The same root cause affects copyfile.rs:191: cancelling a copy of "Photos" into a remote folder that already holds "photos" runs remove_tree(dst.at) over the pre-existing remote folder. Undo of that journaled partial now trashes the whole pre-existing folder.
- **Fix:** In copy_then_remove, refuse the fallback when the source and target names are equal under Unicode case folding, and when the target's parent listing already holds a case-variant of the target name. After the exclusive create_dir of the top target, require read_dir(into) to be empty before copying anything; otherwise abort WITHOUT cleanup. Replace the by-path remove_dir_all cleanup (renamecompat.rs:102, copyfile.rs:191) with removal of only the paths this operation recorded as created.

### S125. The CSV/TSV table walk is quadratic in quotes that do not open a field, so a 1 MiB .csv or .tsv freezes the GUI for about 14 s when the cursor lands on it, and Space on a larger file freezes it far longer

**High**, CONFIRMED (votes: Medium, High, Medium). `ui/js/Delimited.js:55`

- **What:** recordStarts() calls text.indexOf("\n", from) again on every pass of its inner loop. A quote that does not open a field (a mid-field quote such as 5") only moves `from` one character past that quote, so the next pass scans for the newline to the end of the line again. On a body with K such quotes before the next newline the cost is K × (line length), which is quadratic. PreviewTable binds `starts` to this walk on the GUI thread (PreviewTable.qml:35) and does not stop it at maxRecords, because the whole line is one record. MAX_RECORD_CHARS only bounds fields(), not this walk. sniff() runs the same walk on a 64K sample once per candidate separator.
- **Scenario:** A downloaded report.tsv (or .csv/.psv/.tab) holds one line, `a"` repeated to 1 MiB. With the default settings (preview column on, loadOn automatic), the cursor rests on the row for 120 ms, and PreviewColumn → PreviewLines (1 MiB gate, compact, maxRecords 14) → PreviewTable.starts → Delimited.recordStarts runs on the GUI thread for about 14 s. The window does not paint or take input during that time, and moving the cursor away cannot interrupt it. Space opens PreviewText, whose gate is 500 MiB: a 10 MB file then takes about 14 s × 100, roughly 25 minutes, and larger files effectively hang the window for good. For a .csv, sniff() adds about 0.3 s on a 64K sample.
- **Fix:** Compute the next newline once and reuse it while it is still ahead of `from` (recompute only after passing it), or walk with a single indexOf of either character, so the walk is linear. Also stop a record walk after MAX_RECORD_CHARS, and consider running the walk off the GUI thread (WorkerScript).

### S1. A FIFO (or stalled file) under a media name wedges the GUI thread at teardown after Space

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/Preview.qml:152`

- **What:** Space on a row whose icon starts with audio-/video- (a fifo named clip.mp4 qualifies, rows.rs:280-291 + Kinds.js:106-107) loads PreviewMedia.qml, whose MediaPlayer (ui/PreviewMedia.qml:69-76) autoplays the path; Qt's FFmpeg backend opens the file on a pooled thread, which blocks in open(2) on the fifo. The overlay stays responsive until close() (line 132: mediaLoader.source = "") or a cursor move (line 152) destroys the player: the GUI thread then blocks on a futex waiting for the stuck loader thread and never comes back. The column view reaches the same teardown after the user presses play (ui/PreviewColumn.qml:137) and then moves the cursor (line 44).
- **Scenario:** User presses Space on clip.mp4 that is a fifo (from an extracted tarball, or created by another local process), sees the loading crawl, presses Escape or j/k. A second or two later Flea stops answering all input permanently; only kill -9 recovers it. A hung mount holding a real video gives the same sequence.
- **Fix:** Same regular-file gate as the first finding (Facts.state / quickLookKind / Preview.open). Nothing in QML can bound a loader thread stuck in open(2), so refusing non-regular files is the only complete fix; for stalled mounts, consider having the backend `open(O_NONBLOCK)`+read a header before the overlay hands the path to MediaPlayer.

### S2. A large PDF with a broken cross-reference table blocks the GUI thread for seconds on cursor arrival (synchronous QtPdf load)

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/PreviewPdf.qml:65`

- **What:** PdfDocument.source is loaded synchronously on the GUI thread (QtPdf private header qpdfdocument_p.h: load(QIODevice*), initiateAsyncLoadWithTotalSizeKnown, checkComplete under QPdfMutexLocker). When the xref is invalid PDFium rebuilds it by scanning the entire file through the QFile callbacks, so the block is linear in file size. The column opens any *.pdf row after the 120 ms settle without user action (ui/PreviewColumn.qml:216-218 -> PreviewPdf.qml:51-54).
- **Scenario:** A 2 GiB file named anything.pdf (garbage after a %PDF header, or a genuinely corrupted scan) in a browsed folder: the cursor rests on it and Flea freezes about 14 s on NVMe, proportionally longer on a USB stick or network mount, then shows "This file could not be read." Each visit repeats it.
- **Fix:** Do not open a PdfDocument in-process until a jailed check passed: let the backend meta reply (metareq.rs, already jailed with a 5 s watchdog for media) validate the PDF (size ceiling, `%PDF` header, xref sanity via `pdfinfo` or the first-page thumbnail from evince-thumbnailer that thumbs.rs already runs under a 20 s timeout) and have PreviewColumn/Preview open only files that passed; at minimum apply a size ceiling for the automatic column the way PreviewLines says "too large to preview".

### S9. App-supplied save/open folder is never canonicalised: '/.' padding hides the real folder from both displays, so a sandboxed app can steer a Save into ~/.config/autostart with one Enter

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `tools/flea-portal:143`

- **What:** request_for() passes current_folder / the directory half of current_file through verbatim (no normpath/realpath); ui/picker.qml:567 opens on that raw string, ui/PickerChrome.qml:242-247 draws it with elide: Text.ElideLeft (only the tail is visible) and ui/PickerSave.qml:111-115 draws the Output URI from contentX=0 (only the head is visible). A folder like /home/u/./(x60)/.config/autostart/./(x60) shows as '.../././.' in the breadcrumb and 'file:///home/u/./././...' in the URI strip; the true directory is visible in neither, Parent walks one '.' at a time, and the listing of an empty autostart dir looks like any empty folder. The earlier name-traversal fix (tests/js/picker.js:84-92) covered current_name only, not the folder.
- **Scenario:** A sandboxed (Flatpak) app whose user expects a normal 'Save document' dialog calls SaveFile with current_file = /home/u/./.../.config/autostart/./.../evil.desktop (or any writable dotdir). xdg-desktop-portal only type-checks the ay option and forwards it. The picker opens titled by the app, filename pre-filled, no collision, canAccept true; the user presses Enter in the filename field (PickerSave.qml:71 onAccepted -> accept) and the reply URI passes valid_uri() (dots are legal). The frontend registers that path writable for the app, which writes a .desktop that runs at next login: sandbox escape and code execution as the user. Same trick with '..' segments (/home/u/Documents/../../../etc is shown raw) and with existing targets (~/.bashrc) where the only extra step is the 'Use this location' button, whose text also never names the folder.
- **Fix:** In request_for(): after os.path.isabs(), apply os.path.normpath() (or os.path.realpath()) to folder and file before splitting, so '.' and '..' never reach the window; mirror it in Picker.request() so flea shelf choose and any other caller get the same rule. Draw the resolved folder next to the Filename field in save mode (e.g. 'into ~/.config/autostart') rather than only in the elided breadcrumb, and add a picker.js test that a padded/dot-dot folder is normalised.

### S10. No cap on concurrent picker requests: every OpenFile spawns a full qs + Vulkan + flea --backend process, so a sandboxed app can exhaust memory

**Medium**, PLAUSIBLE (votes: Low, Low, Low). `tools/flea-portal:326`

- **What:** on_call() refuses only a duplicate handle; every distinct handle gets its own Pick, mkdtemp and 'flea --pick' child, which exec's a Quickshell instance (qs is launched without --no-duplicate, so instances multiply) that in turn starts flea --backend. The handle is chosen by the frontend per call, so a caller gets one window per call. The earlier review recorded the same gap for FileManager1, but that service is reachable only by unsandboxed peers; the FileChooser impl is reachable by every sandboxed app through org.freedesktop.portal.Desktop.
- **Scenario:** A Flatpak app (or a web page driving a browser that uses the portal) issues OpenFile in a loop; xdg-desktop-portal creates a Request per call and forwards each. Fifty calls become fifty qs processes with Vulkan contexts plus fifty backends, each holding a listing of the start folder; the compositor and RAM are saturated and the user has to hunt down the offending app while windows keep appearing. GTK's backend pays one in-process dialog per call; Flea pays a process tree.
- **Fix:** Keep a per-app_id (and global) ceiling on open picks, e.g. one live chooser per app_id and a small global cap; answer RESPONSE_OTHER (or queue behind the live one) for anything above it. Optionally reuse a single qs process with multiple windows later.

### S15. Interrupted permanent delete is never recovered unless the user opens the Trash view

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/trashbrowse.rs:268`

- **What:** trashdelete::recover() has exactly one caller: the trashbrowse "list" op when the request carries recover:true (src/backend/trashbrowse.rs:267-270). The only sender of recover:true is ui/TrashView.qml:86 (Trash view open); the 30-day sweep sends recover:false (ui/TrashHost.qml:37) and there is no startup hook (grep of src/ for recover( / recovery_root). So after a backend death mid-delete (OOM kill, crash, power loss), the item vanishes from the folder listing while its surviving contents stay in a hidden 0700 quarantine dir beside it (<parent>/.flea-delete-<pid>-<n>/payload) and the journal waits in $XDG_STATE_HOME/flea/recovery; disk space is not freed and nothing tells the user. A user who never opens Flea's Trash view keeps that state indefinitely; if they meanwhile create a folder of the same name, the eventual replay refuses (RENAME_NOREPLACE, recovery.rs:171) and the data stays hidden.
- **Scenario:** User permanently deletes ~/Projects/old (large tree). The backend is killed 20% through. Next launch: ~/Projects lists no 'old' (the quarantine is a dotdir, invisible unless hidden files are shown), du shows the space still used. Nothing is reported. Only when the user happens to open the Trash view does 'old' reappear with its remaining files.
- **Fix:** Run recover(recovery_root()) once at backend start (and/or before the first "list" of a folder that contains a .flea-delete-* entry), and have the sweep's list pass recover:true. Report the RecoveryReport through the normal status channel (operationResult) rather than only inside the Trash view. Optionally render a .flea-delete-* directory in a listing as 'interrupted deletion (N items preserved)' instead of an ordinary hidden folder.

### S18. Deep search walks the request loop into hung FUSE/network mounts with no skip list or thread

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/search.rs:53`

- **What:** Ctrl+Shift+F (search with shallow:false) is ticked on the one thread that answers every request (run.rs:107-121 try_recv -> tick_walkers run.rs:413-418 -> searchreq.rs:262-266 -> search.rs:40-56), and read_one() calls std::fs::read_dir on every directory it reaches with no mount check. The unbounded-mount skip that dirsize built for exactly this (dirsize.rs:25-46, 51-67, 157-160) has no other user (grep confirmed), and unlike dirsize the search has no deadline, no generation flag and no thread.
- **Scenario:** On this box Ctrl+Shift+F from ~ descends into ~/TresoritDrive (fuse.tresoritfs, which dirsize.rs:16-19 documents as 'du does not finish in 25 s' and hanging the walker), and from / or /run into /run/user/1000/gvfs (remote shares: network traffic the user did not ask for) plus the whole of /proc and /sys. One getdents that never returns holds the loop forever: no list, window, searchcancel (Esc), dirsize or quit is processed (they all queue behind it in the channel; only transfercancel is acted on by the reader thread, events.rs:411-416), and ui/Backend.qml has no request timeout (only onExited at Backend.qml:363-370 reports anything), so the window is dead until the user kills Flea. No data is lost.
- **Fix:** In Search::new read /proc/self/mountinfo once and keep dirsize::unbounded_mounts(); in read_one skip a directory equal to one of them (and by fs type, proc/sysfs/devtmpfs). Better: run the walk on its own thread with the dirsizereq.rs generation/stop pattern, streaming batches back as an Event, so the loop can never wait inside a getdents and Esc always works.

### S27. Held Delete/Enter/Ctrl+Z/Ctrl+V auto-repeat re-dispatches the action on every repeat event

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/js/Focus.js:333`

- **What:** Focus.handleKey swallows auto-repeat only for toggleSelect (line 283); every other action, including trash (Delete), open (Enter), undo (Ctrl+Z) and paste (Ctrl+V), is dispatched once per KeyPress, and Qt delivers a held key as a stream of KeyPress events with isAutoRepeat=true. The backend's busy guard (opsdispatch.rs start_trash / start_transfer_checked) only refuses while a thread is in flight; do_undo (opsdispatch.rs:214) is synchronous so it has no busy window at all.
- **Scenario:** Operator leans on Delete for about a second (or a key sticks). Press 1 trashes the cursor row; PaneWire.onTrashed -> Anchor.afterDelete moves the cursor onto the row that took its place and re-lists; each auto-repeat that lands while the backend is idle sends `{c:"trash", rows:[cursor]}` for the next row. Ten or more rows go to Trash, each as its own journal entry, with no dialog; the status line says '^z undoes' but each ^z undoes one. Held Ctrl+Z unwinds many operations (a copy's undo now sends the copies to Trash) and there is no redo key or menu row to walk back. Held Enter on a file relaunches `flea --open` as soon as the previous one exits (11-15 ms for Exec= handlers), spawning the handler repeatedly. Held Ctrl+V with a copy clipboard queues transfer after transfer.
- **Fix:** In Focus.handleKey, right after `action = lookup(event, root)`, return true when `event.isAutoRepeat` and the action is anything but a cursor/page/extend movement (or whitelist: cursorDown/Up/Left/Right, pageDown/Up, cursorFirst/Last, extendDown/Up). Apply the same guard in ui/SettingsPanel.qml's favourite/openrule x/Delete branch and in TrashView.qml for `trash`/`deletePermanently`. Consider exposing redo (Ctrl+Shift+Z / Ctrl+Y) so an over-undo can be walked back.

### S32. Space preview keeps up to 48 full-size lossless renders in the runtime tmpfs, never cleaned at exit

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/previewimage.rs:24`

- **What:** Every Space image preview is rendered by the jailed thumbnailer into $XDG_RUNTIME_DIR/flea-preview at the window's size (renderSize rounds the viewport up to a 512 step, max 4096, ui/PreviewImage.qml:30) and the folder is only trimmed to the newest KEEP=48 files (src/previewimage.rs:83,111-124) after a further successful render; nothing removes them when the preview closes, when Flea exits, or when the source volume is unmounted. Measured under /tmp with target/release/flea --preview-image: a 4000x3000 JPEG renders to a 15.4 MiB PNG at size 4096 (5.9 MiB at 2560). 48 such renders are ~720 MB of tmpfs, and systemd sizes /run/user/<uid> at 10% of RAM by default (~800 MB on an 8 GB laptop). Two consequences: (1) a photo-browsing session on a 4K screen can fill the runtime dir that also holds the Wayland, PipeWire, D-Bus and keyring sockets, so other apps start failing until logout; (2) exact-resolution copies of the last 48 private images the user looked at (including ones on a removable, encrypted or remote volume that is now gone) sit in RAM-backed tmpfs until logout, in a 0700 folder any same-user process can read.
- **Scenario:** User with a 4K window steps through 50 phone photos with Space on an 8 GB machine: ~48 x 15 MB PNGs remain in /run/user/1000/flea-preview after Flea is closed; the runtime tmpfs hits its limit and new sockets/files by other session processes fail. Separately, the user previews photos from an encrypted USB stick, unplugs it, and the full-size renders remain readable in the runtime dir for the rest of the session.
- **Fix:** Cap the folder by bytes (e.g. 128-256 MB) rather than by count and sweep on every render; delete renders that no open preview references (e.g. remove the folder from ui/shell.qml on window destruction via a flea --preview-image --sweep, or unlink the PNG once Image reports Ready and the next render is complete); render at the visible surface size rather than the 512-step ceiling; optionally skip caching for files on removable or remote mounts.

### S36. Shelf zips land in the state directory the README tells you to rm -rf, and nothing ever prunes them

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/shelfzip.rs:189`

- **What:** `flea shelf zip` writes every archive to `$XDG_STATE_HOME/omarchy/flea-shelf/archives/shelf-<date>[-n].zip` (archives_dir, shelfzip.rs:188-190), a 0700 hidden directory the card presents only as a pile row. `x` on that row (Panel.qml:385-392 -> shelf.rs:66-79 settle) forgets the reference and leaves the file; a drag-out is a copy for any receiver but Flea. README.md:149-153 then instructs `rm -rf .../omarchy/flea-shelf` on uninstall and states "the files the shelf was holding are untouched", which is false for archives.
- **Scenario:** User zips a pile with `a`, drags the archive to a mail client or copies it out (a copy), presses `x` on the row, later follows the README uninstall. Every archive ever made with `a` is deleted with the state dir; the same happens with any `~/.local/state` cleanup. Reproduced under /tmp: after zip + forget, the pile is `[]` and `archives/shelf-2026-09-23.zip` is still on disk, invisible from the card.
- **Fix:** Write archives somewhere user-visible and durable (the pile's common ancestor, `$XDG_DOWNLOAD_DIR`, or a Settings-chosen folder) instead of the state dir; or keep the location but make the archive row's drag-out a move by default, add a "Reveal" action, and correct the README so uninstall does not delete archives (or have `flea shelf clear`/uninstall move them out).

### S37. Shelf `z` can reverse an older, unrelated move: drag-out moves are never journaled and undo.json has no age or session bound

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/shelfundo.rs:150`

- **What:** Only the CLI `flea shelf move` writes undo.json (shelfops.rs:99-103). A drag out of the card into a Flea pane is redeemed by src/backend/shelfdrop.rs:44-90, which settles the pile but never records a Move for the shelf's undo (it lands only in that pane's in-memory journal). `flea shelf undo` (shelfundo.rs:150-171) then compares the undo.json timestamp only against the last cleared pile, with no session or age limit, and `reverse` walks whatever record is there while the card says "Undid the move" (Model.js:254-263).
- **Scenario:** Day 1: `m` moves ~/Downloads/a.pdf to ~/Work. Day 3: user drags b.txt from the card into a pane folder (a move), regrets it and presses `z` in the card as README:111 says. a.pdf is silently moved from ~/Work back to ~/Downloads, b.txt stays where it was, and the card reports "Undid the move". Reproduced under /tmp: after `shelf move` of a.txt then an unrelated later move of b.txt, `shelf undo` printed `move 1` and put a.txt back.
- **Fix:** Record the drop's `entry.steps` (Step::Moved) into shelfundo::Moves from shelfdrop::run_and_settle, so `z` in the card covers the gesture the card started; and bound the record (refuse or confirm when older than a few minutes / a different card session, or name what will be undone: "Put a.pdf back in Downloads?").

### S41. Copy discards close(2)/flush errors, so a cross-device move can delete its source after a write the mount reported as failed

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/copyfile.rs:93`

- **What:** copy_file_at calls w.flush() (a documented no-op for std::fs::File) and then lets w drop, which ignores the close(2) return value; nothing in the copy path calls sync_data/sync_all or checks close (grep: only trashmanifest.rs syncs). move_any (copyfile.rs:290-303) then runs movesource::remove_copied, whose only guard against a bad copy is that symlink_metadata(dst).len() equals the source length. On mounts that deliver write failures at close (kernel NFS/CIFS via fstab, FUSE filesystems that implement flush such as sshfs, gvfs MTP which pushes the whole file at close) the error is dropped, the client's cached attributes still answer the full size, the source is removed and the destination is short or absent. This extends the already-known 'no fsync before removing source' item: it is not only durability on power loss, synchronously reported errors are lost too.
- **Scenario:** User drags a folder of photos from the local disk onto a pane listing an NFS or CIFS share mounted via fstab (or an sshfs mount) with Move. The server hits its quota / ENOSPC / drops the connection during writeback; write(2) succeeded into the page cache, close(2) returns EDQUOT/ENOSPC/EIO, Flea ignores it, remove_copied stats the destination (cached size == source size), unlinks the local original, and reports the move as ok. The share holds a truncated file; the original is gone.
- **Fix:** After the write loop call w.sync_data() (or sync_all) and check it, then close explicitly (e.g. libc::close(w.into_raw_fd()) or nix) and treat a non-zero return as left_partial(); only then run remove_copied. Optionally fstat the destination through the open fd rather than a fresh path stat so the size check is not answered from cached attributes.

### S42. A mount that stops answering blocks the backend's only request thread (and its thumbnail workers) with no timeout; quit then leaves an orphaned backend with its transfer still running

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/run.rs:211`

- **What:** Every listing-side syscall runs on the loop thread with no bound: scan() read_dir/stat (run.rs:211), watch.begin -> inotify_add_watch path lookup (run.rs:210), stat_range (run.rs:456), statfs for FsInfo (run.rs:351), dev_of (run.rs:231), the search walker's read_dir ticking on the loop with no mount-boundary rule (search.rs:53, unlike dirsize.rs unbounded_mounts), thumb_rows' metadata (thumbreq.rs:36), and copy_then_remove for a WebDAV/rclone directory rename (ops.rs:44 via opsdispatch.rs:190). The four pool workers canonicalize the input before the 20 s child timeout applies (thumbargv.rs:7), and gio trash has no bound (trash.rs:16-19). A FUSE request already handed to gvfsd-fuse is not interruptible by signals, so the thread sits in D state until the SMB/SFTP layer times out. Backend.qml has no per-request watchdog, only the 30 s quitDeadline (Backend.qml:325-329): closing the window queues 'quit' behind the stuck syscall, the shell kills itself at 30 s (shell.qml:143) and the backend lives on with stdin closed; Event::Closed is never processed, so drain()'s cancel (run.rs:432-434) never runs and a transfer thread keeps writing with no window, then leaves its partial under the final name when the mount errors (the known truncated-file item) with the in-memory journal gone. AGENTS.md:4446-4460 acknowledges only the symlink-stat variant as 'not defended against on purpose'; opscancel.rs:1-4 records the loop being held inside read_dir on an MTP phone (issue 144). It is not a machine wedge: the wait ends when the gvfs backend times out or the session tears gvfsd-fuse down.
- **Scenario:** Pane lists /run/user/1000/gvfs/smb-share:server=nas,share=photos and a copy into it is running. The NAS goes away. The next window/fsinfo/changed request hangs the loop, the copy thread hangs in write(); Cancel does nothing (flag is polled between chunks), closing the window shows nothing for 30 s, then the shell exits and `flea --backend` remains, in D state, until the SMB timeout; the half-written file then sits on the share under its real name and nothing ever reports it. With Ctrl+Shift+F from / or /run, the same walk enters every gvfs mount and a dead phone or share freezes the pane the same way.
- **Fix:** Move scan/stat_range/statfs/search steps off the loop onto a worker with a generation counter (the pattern cancel_dirsizes already uses) so a stuck one is abandoned; apply dirsize.rs's unbounded_mounts() refusal to search.rs; do canonicalize inside the child's timeout or via an O_PATH|O_NONBLOCK open + /proc/self/fd; run gio trash under run_with_timeout; in Backend.qml, SIGTERM the child when quitDeadline fires so no orphan survives the window.

### S44. PDF teardown mid-render is unguarded: the #117 abort path survives the settle fix

**Medium**, PLAUSIBLE (votes: Medium, Medium, Medium). `ui/PreviewColumn.qml:216`

- **What:** The Issue 117 mitigation in ui/PreviewPdf.qml only delays PDF-to-PDF source switches; the two teardown paths that destroy the PdfDocument (and its carrier QFile) while an async PdfPageImage render is still on Qt's pixmap reader thread run immediately: the column's pdfLoader goes inactive the instant the cursor leaves a PDF row for any other kind (PreviewColumn.qml:216), and the overlay's close()/follow() drop pdfLoader.source = "" at once (Preview.qml:152). A freshly created reader also opens at once with no settle (PreviewPdf.qml:52), so a held Down through a mixed folder opens every PDF immediately and destroys it on the next step. The maintainers' own diagnosis (commit a3a027a) is that destroying the carrier device under the reader thread aborts the process, so a slow-rendering page plus one more keypress crashes the whole GUI.
- **Scenario:** Cursor lands on a heavy PDF (a scanned page that takes ~2 s to rasterise); within that window the user presses Down onto a text file, or Space then Escape in the overlay. The Loader destroys PreviewPdf and its PdfDocument while QQuickPixmapReader is still inside QPdfIOHandler::read on the carrier QFile: the same use-after-free that #117 reported, now via teardown instead of a source change. The window aborts; any dialog, rename or selection in progress is lost.
- **Fix:** Keep one PreviewPdf alive across kinds and clear its document only after the in-flight render completes: e.g. keep pdfLoader active while `pdfLoader.item && pdfLoader.item.rendering`, gate the overlay's close() on PdfPageImage.status !== Image.Loading (or hide and defer the source clear via the existing settle timer), and apply the 120 ms settle to a freshly created reader too.

### S45. Ejecting a stick fails while the cursor rests on a PDF: the preview holds the file open and the message blames someone else

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/DeviceMounts.qml:191`

- **What:** The column keeps a QtPdf PdfDocument loaded for as long as the cursor rests on a PDF (PreviewColumn.qml:216 -> PreviewPdf.qml:65), and QtPdf keeps the QFile open for the document's lifetime (confirmed with qml6: one fd on the PDF still open 2.5 s after status Ready). Eject and Unmount (DeviceMounts.qml:172,191) run `gio mount -e/-u` without releasing any preview first, and Ctrl+E from a listing targets exactly the removable volume the cursor is in (Eject.js release). umount returns EBUSY, udisks refuses, and Eject.sentence (Eject.js:122) tells the user 'is still mounted; close what is using it' when the user is Flea's own preview. The same holds for a paused Quick Look video/audio (MediaPlayer keeps the file open) and the overlay PdfViewer.
- **Scenario:** User copies photos onto a USB stick, browses it, the cursor lands on manual.pdf (automatic preview is the default: uischema.rs:36 loadOn=automatic), presses Ctrl+E. gio mount -e fails with 'target is busy'; Flea reports 'is still mounted; close what is using it'. The user closes other apps, retries, fails again, and is left with a mounted filesystem holding dirty pages from the copy; unplugging anyway loses the freshly written files. Nothing in the UI says the preview column is the culprit or offers to release it.
- **Fix:** Before running gio mount -e/-u, release every surface that holds a file on that mount: set a pane-level `releasing` flag that forces pdfLoader/mediaLoader/PreviewText inactive (and closes the Quick Look), wait for the loaders to report null items, then spawn gio. When the verdict is 'mounted' and the path is inside the volume, say so ('Flea's preview was using it; moved off the file, try again') instead of 'close what is using it'.

### S48. TUI rename editor writes raw control characters from file names to the terminal

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/tui/editor.rs:185`

- **What:** Editor::line() pushes every char of self.value verbatim (`out.push(c)`); the only filtering is render::clean() on newly typed text in update(). Editor::rename() seeds value with the raw row name and the path bar's Tab completion seeds it via replace() with a raw directory name, so a file or folder whose name embeds ESC/BEL/OSC bytes is emitted unfiltered into the terminal stream by render.rs:344 (row line) and render.rs:441 (header line). Every other draw path (rows, tabs, preview, properties, menu, footer) goes through fit()/clean(), which strips controls and bidi overrides; this one does not.
- **Scenario:** Attacker plants a file named e.g. `a\x1b]52;c;<base64>\x07b.txt` (or a kitty graphics `\x1b_G...` command, or an OSC title/hyperlink) in a folder the user browses in `flea --tui`. The user presses `r` on it (or Tab-completes into a directory with such a name in the path bar). The name is printed raw, so the terminal executes the embedded sequence: clipboard write via OSC 52 on terminals that allow it, arbitrary readable file displayed via kitty graphics t=f, title spoofing, etc. text_width() reports 0 for the control chars so the width budget never truncates them. Verified end-to-end: the backend delivers such names as `\u001b` in the rows line (see evidence) and nothing between Row::parse, Editor::rename and print! filters them.
- **Fix:** Run the value through render::clean() (or filter with render::safe) in Editor::new/rename/replace, or in Editor::line() skip any char for which !render::safe(c). Keep the raw name in Editor.path for the identity check; only the displayed/edited string needs cleaning (and valid_name should probably also reject control characters so a cleaned name cannot silently rename the file).

### S53. Folder request: Enter or Right on a file row hands the app the whole current directory

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `ui/picker.qml:229`

- **What:** In a directory chooser (OpenFile directory=true, or SaveFiles) the picker lists files as unmarkable rows, but activate() on a file row falls through to accept(), and accept() with no marks in folderMode marks and returns win.path, the directory currently on screen. The list starts at $HOME when the app names no folder (picker.qml:567) or at whatever the app put in current_folder. The status hint says 'Enter open · Space mark folder', and Right (Keymap.js:47, pageForward in the listing context) means 'open / preview' in the browser window, so a single habitual Enter or Right on a file row grants a sandboxed app the entire directory without any Space mark.
- **Scenario:** A Flatpak app calls OpenFile with directory=true and current_folder=/home/u (or ~/.ssh, ~/.config). The picker opens with the cursor on the first row, a file. The user presses Right to peek at it, or Enter expecting it to open. PickerList.qml:166-167 calls picker.activate(cursor); picker.qml:198-207 sees a non-directory and calls accept(); accept() at 229-236 has no marks, sends {op:mark, path:win.path, directory:true}, the backend pins the directory, onPickerResult finishes with RESPONSE_OK and the app receives file:///home/u (document portal grants the whole tree). GTK's folder chooser ignores activation of a file row; Flea grants.
- **Fix:** In folderMode make activate() on a non-directory row a no-op (or say 'Press Space to select a folder first', the same rule the file mode applies), and let only the explicit Choose folder control, or Enter with a marked directory, return the directory the window is standing in. Do not let pageForward reach accept() at all in the picker.

### S54. SaveFile collision review approves a symlink under the chosen name without saying where it points

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/picker.rs:143`

- **What:** The save probe opens an existing entry at folder/name with O_NOFOLLOW, notices it is a symlink, pins the target too, but then reports only collision:true with the link path; review passes and the reply URI is the link. The prompt (PickerSave.qml:138 'This confirms the shown name and folder. Cancel leaves the existing file untouched.') and the Output URI show the link path only. The application then writes through the link: GLib's g_file_replace follows symlinks unless G_FILE_CREATE_REPLACE_DESTINATION is set, so the file that is overwritten lives outside the folder the user was shown. The picker is the only party that knows it is a link and it stays silent.
- **Scenario:** A downloaded archive extracted into ~/Downloads contains notes.txt -> ../../.bashrc (bsdtar creates symlinks with arbitrary targets). Later an app's Save dialog is opened in ~/Downloads with current_name=notes.txt (or the user types it). The picker says the name collides, the user presses 'Use this location' believing they replace a text file in Downloads, the app truncates and rewrites ~/.bashrc. Reproduced the Flea half under /tmp: with notes.txt -> ../elsewhere/bashrc the backend answered {"op":"save","ok":true,"collision":true,"path":".../Downloads/notes.txt"} and {"op":"review","ok":true,...} with no mention of the link or its target.
- **Fix:** When the colliding entry is a symlink, either refuse the name ('notes.txt is a link to /home/u/.bashrc; choose another name') or resolve it and make the review name the resolved target explicitly (collision text and Output URI showing the target path, danger button labelled with it). Add 'link' to the save reply so the UI can tell the two apart; add a unit test beside save_refuses_directories_and_links_to_directories.

### S57. Refreshed Trash confirmation re-targets a reused trash name with no identity check; the dialog looks identical, so a different file is permanently deleted

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/trashbrowse.rs:464`

- **What:** When the 2.5 s 'check' (or the GIO monitor) reports the trash changed while the permanent-delete dialog is open, TrashView.sourceChanged() re-prepares with refreshToken. refreshed_targets() rebuilds the target set from the URIs of the previous confirmation with identity:"" (trashbrowse.rs:456-470), and prepare() skips require_selected_identity when refresh_token > 0 (trashbrowse.rs:490-492). Whatever now lives at trash:///<same name> is snapshotted and offered for deletion. TrashConfirm's text for a selection is only 'Delete N items permanently?' / 'These Trash items are deleted from disk.' (ui/TrashConfirm.qml:90-92), with no name, original path or byte count, so the reopened dialog is indistinguishable from the one the user was answering.
- **Scenario:** User selects trash:///notes.txt (original ~/Documents/notes.txt) and presses Delete; the dialog opens. Meanwhile another client (second Flea window/pane, Nautilus, a sync tool, a script) restores that item and trashes a different ~/Desktop/notes.txt, which GIO files under the now-free name notes.txt. Within ~120 ms the GIO monitor fires sourceChanged(); the dialog blinks and reopens with the same wording; the user finishes the Right+Enter they were already making and ~/Desktop/notes.txt, never selected, is unlinked with no undo.
- **Fix:** In refreshed_targets, carry the previous items' provider identity (or the backing Reviewed dev/ino) and drop/refuse any URI whose current detail() no longer same_item()s the reviewed one, returning 'Trash changed; select the items again' instead of silently re-targeting. Independently, make the non-all confirmation show the item name(s)/original path and bytes so a substitution is visible.

### S58. Flea's quarantine lives inside Trash/files, so GIO lists it as a trash item; any surviving quarantine kills the Trash view and the auto-sweep, and hides the recovery report

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/trashdelete.rs:477`

- **What:** with_claimed() creates the .flea-delete-<pid>-<n> quarantine inside <Trash>/files (trashdelete.rs:472-482). gvfs lists every entry of files/, and one without a .trashinfo prints as 'trash:///.flea-delete-...\t(null)'; trashbrowse::parse_list then fails closed (trashbrowse.rs:111-116). The failure branches that deliberately leave the quarantine behind ('Recover preserved items from …', trashdelete.rs:506-511 and 519-526, or a crash/SIGKILL/shell restart mid-delete) therefore make every subsequent list, check, prepare and sweep fail with 'GIO returned an invalid Trash identity.' In the list op recover() runs before list()? (trashbrowse.rs:268-271), so when the journal cannot be replayed (name collision, which is exactly the case that leaves a quarantine behind) its failure text is discarded and the user only sees the GIO error. TrashHost.sweepReceive() ends silently on any !ok (ui/TrashHost.qml:62) and never tells the user the 30-day sweep has stopped. The 'preserved' payload also sits where every other trash client's Empty Trash (Nautilus, gio trash --empty) deletes it unreviewed, and the sidebar count includes it.
- **Scenario:** Empty Trash; while report.txt is claimed, another app trashes a new report.txt (or the deletion fails and the rename-back hits RENAME_NOREPLACE). Flea reports 'Recover preserved items from …/files/.flea-delete-…'. From then on the Trash view shows only 'GIO returned an invalid Trash identity.', Empty Trash and Restore are unavailable, the opt-in auto-empty never runs again, and the journal's 'Could not return interrupted item without overwriting' never reaches the UI. A shell restart during Empty Trash gives the same state until a replay succeeds.
- **Fix:** Create the quarantine as a sibling of files/ (e.g. <Trash>/.flea-quarantine/<name>, same filesystem so renameat2 still works, invisible to GIO) and record that path in the journal; and in parse_list tolerate a '(null)' original for names starting with .flea-delete- so an in-flight or leftover quarantine never blanks the view. Report recovery failures even when list() fails, and surface a sweep failure at least once.

### S62. Folder-size mount refusal is lexical on the un-canonicalised base, so a directory reached through a symlink into a cloud/NFS mount is walked instead of refused

**Medium**, CONFIRMED (votes: Medium, Low, Low). `src/backend/dirsize.rs:117`

- **What:** walk_cancellable() decides refusal with mount_type_in(path, mountinfo), which compares the raw path string with mount points by Path::starts_with (mountinfo.rs:155), and skips descendants only when `mount == path` exactly (dirsize.rs:157); the walk root is st.base.join(name) where st.base is whatever string the client navigated with, and the UI does navigate through symlinks unresolved (Opener.qml:45 isDirectory -> PaneWire.qml:70 pane.open(path) with the symlink's own path).
- **Scenario:** ~/nas is a symlink to /mnt/nas (nfs4, or ~/cloud -> /run/user/1000/gvfs/sftp:...). User presses Enter on the symlink row: open.rs returns the is-directory status and the pane opens '/home/u/nas'. Folder sizes are on, so every visible directory row queues a dirsize for '/home/u/nas/<dir>'. refuses() finds no mount point that is a lexical prefix of that path and unbounded_mounts() never matches on the way down, so the walker thread runs read_dir/lstat over the network share; if the server has stopped answering the thread blocks in getdents forever, cancel_dirsizes() abandons it by generation, and every scroll/visit leaks one more D-state thread plus an open directory handle. children() (dirsize.rs:105) also read_dir's the root on the same thread.
- **Fix:** Canonicalise the walk root once (std::fs::canonicalize, falling back to the raw path) before refuses()/unbounded_mounts(), or better compare devices: read the root's st_dev and stop descending when entry.metadata().dev() differs (du -x semantics), which also covers bind mounts and autofs without any name matching.

### S64. run_boxed buffers the jailed tool's stderr with no budget: an 80 KB archive drives the backend to hundreds of MB

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/archivework.rs:71`

- **What:** run_boxed runs bsdtar/7z/magick with Command::output(), which accumulates the whole stderr in memory. bsdtar prints one diagnostic line per refused member and echoes the member name, so an archive of pax members with long '../' names produces stderr proportional to its decompressed size while its compressed size stays tiny. The prlimit caps (30 CPU-s, 2 GiB address space) apply to the child, not to the parent's buffer, and archive jobs take no slot so several can run at once.
- **Scenario:** User presses Extract on a downloaded 80 KB .tar.zst holding 12,000 members named '../' + 30,000 'a's. Measured through the real backend under /tmp: the extract runs ~11 s and the backend's VmRSS peaks at 357,412 kB (from ~10 MB) before answering 'bsdtar: Error exit delayed from previous errors'. With the 30 CPU-second cap the same shape reaches ~1 GB per job; two or three such extracts started together (allowed by design) push a laptop into swap. The --sheet path was given a 64 MiB budget in this very diff (src/sheet.rs MAX_OUTPUT_BYTES); this jail was not.
- **Fix:** Pipe stderr and read it through a bounded reader that keeps only the last line (or last 64 KiB) and discards the rest, the way sheet.rs budgets its reader; optionally pass '--ignore-zeros'-free quiet options where the tool has them. Consider a wall-clock deadline per job as well.

### S67. Cross-device move: a child that cannot be removed aborts remove_copied and leaves the complete copy unjournaled with a hollowed source (Err arm of the known kept-entries finding)

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/copyfile.rs:296`

- **What:** The known finding covers Ok(kept>0). The Err arm behaves the same but worse: movesource::remove_at propagates the first remove_file/remove_dir error with '?', so siblings already removed are gone from the source, the rest stays, the destination holds a complete copy, one_item's Err arm journals only p.partial (None after a successful copy) so nothing is undoable, the item is reported as failed with 'permission denied', and it is pushed to retry; the retry then fails with 'already exists' because the destination tree is there.
- **Scenario:** Move (drag, or cut+paste) a folder from an ext4 home to a USB stick or tmpfs where the folder holds a.txt, b.txt and a 0555 subfolder locked/ with files. copy_any succeeds (locked is recreated 0555). remove_at removes a.txt and b.txt, then fails with EACCES on locked/inside.txt. Result: source keeps only locked/, destination has everything, the status bar says 'Move failed: <folder> · permission denied', undo has no step for the copy, retry answers 'already exists'.
- **Fix:** In remove_at, treat a failed removal like a changed entry (count it as kept and continue) instead of aborting the walk; and on any error after a complete copy, journal Step::Copied{src,dst} and answer under the KEPT kind so the UI says the copy is whole and undo can take it back.

### S69. Held x/Delete in Settings strips favourites and open rules one row per key repeat, with no undo

**Medium**, CONFIRMED (votes: Low, Low, Low). `ui/SettingsPanel.qml:543`

- **What:** The known held-key finding (ui/js/Focus.js) is incomplete: the Settings panel's own key handler removes the favourite or open rule under the cursor on every `x`/Delete event with no `isAutoRepeat` gate, and after each removal the next row slides under the cursor (favourites: onWrote keeps `cursor`; open rules: removeOpenRule steps to the previous rule), so a key held past the repeat delay deletes row after row. Favourites and open rules have no undo journal and ui.json is rewritten atomically, so the list is simply gone.
- **Scenario:** Open Settings > Places (or File types), put the cursor on a favourite (or rule) row, hold Delete or `x` for ~1.5 s (Hyprland default repeat 600 ms delay, 25/s). Favourites: each `flea --favourites {"op":"remove","index":i,"expected":[...]}` write (lock+fsync+rename, measured 7-19 ms in uistore.rs) finishes before the next 40 ms repeat, Favourites.apply's `busy` gate is already clear, `records` are refreshed on exit so `expected` matches, and the next favourite under the same cursor index is removed on the next repeat. Open rules: ViewState.changeLeaf applies locally at once, rows rebuild, and the cursor is stepped to the previous rule, which the next repeat removes. A 2 s hold empties a 20-row list.
- **Fix:** In SettingsPanel's Keys.onPressed, `if (event.isAutoRepeat) return` before the destructive branches (or drop only the removal branches on repeat, as Marks.paintPress does for Space); additionally require a second confirming keypress or a status-centre undo (re-add the removed record) for favourites.

### S76. A failed (non-cancelled) shelf move or copy leaves a truncated file under the real name at the destination with no undo route, and the retry is refused as "already exists"

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/shelfops.rs:119`

- **What:** When a copy fails after the destination was created, the engine keeps the partial and reports it as a `Step::Copied` for the journal (copyfile.rs:127, opsreq.rs:328); the pane's ctrl-z removes it. The shelf's `record_undo` keeps only `Step::Moved` rows (shelfundo.rs:129) and never journals copies at all, so a shelf move that failed mid-copy (disk full, I/O error on a removable or network destination, permission on a child) leaves a half-written file under the final name that the card cannot clean up, and pressing Move again fails on every such item with "already exists". This extends the known "failed copy leaves truncated file under real name": on the shelf there is no journal step left to remove it.
- **Scenario:** Pile of three videos, Move to a USB stick that fills up on the second one. Card says "Moved 1 of 3 · no space left". The stick now holds video2.mp4 truncated at the failure point, indistinguishable by name from a good copy; the shelf still lists video2 and video3. Freeing space and pressing m again: video2 fails with "already exists" and the truncated file stays. Nothing in the card or in `flea shelf undo` can remove it.
- **Fix:** Either remove partials in the shelf path (after `report`, `discard` every `Step::Copied` whose `to` is a partial from a failed item), or journal `Copied` partials in undo.json so `z` removes them, and name the leftover in the card's sentence.

### S82. Undo of any cross-device move runs a full uncancellable copy-then-remove on the request loop

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/undo.rs:180`

- **What:** Step::Moved is journaled for every successful move, including one that move_any completed by EXDEV copy (opsreq.rs:321-323). Ctrl+Z reverses it with rename_path(to, from) (undo.rs:180); across devices that is EXDEV again, and renamecompat.rs:54 sends it into copy_then_remove, which builds a Progress with a private AtomicBool nobody can set and a no-op byte sink (renamecompat.rs:96-98). do_undo (opsdispatch.rs:214-221) runs ops.journal.undo() inline on the loop thread, not on the op thread the way start_redo does, so the whole tree is copied back with no transfer card, no cancel, and every listing, thumbnail and other request queued behind it. The known finding names only rename on four measured FUSE mounts; this arm is reached by the everyday USB-stick or NAS move.
- **Scenario:** Move a 20 GB folder from ~/Videos to a USB stick (copy+remove, with a card and a cancel). Press Ctrl+Z. The backend silently reads the whole tree back off the stick onto the internal disk on the request loop: the UI shows nothing, Escape/cancel has no target, the listing stops following changes, and pressing q makes the shell wait 30 s (quitDeadline) before killing itself while the backend goes on copying until the tree is done (stdin EOF becomes Event::Closed only after the loop is free). A slow stick or a network share stretches this to many minutes of an apparently frozen file manager.
- **Fix:** Reverse a Moved step whose dev differs (before.dev != after.dev, both are in ItemIdentity) through the transfer machinery instead of rename_path: start it as a cancellable op with an id, the Live cancel flag and the OpMsg progress channel, the way redo.rs replays it, and answer `undone` from TransferDone. Until then, refuse the undo with a message when rename_noreplace answers EXDEV rather than falling into copy_then_remove from undo.rs and shelfundo's move_back.

### S88. Closing the terminal while a TUI copy or move runs kills the backend mid-write and skips its cleanup

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/tui/wire.rs:13`

- **What:** The TUI starts `flea --backend` in its own process group and never changes its signal handling. The TUI catches SIGHUP, SIGINT and SIGTERM so it can shut down cleanly: on quit the backend cancels the running operation and removes the partial file (run.rs:429-434). The backend itself still dies on SIGHUP. Pressing q during a transfer also cancels it without asking and prints nothing once the screen is restored.
- **Scenario:** 1. In `flea --tui`, paste a 10 GB file onto a USB stick. 2. Close the terminal window or tab, or lose the SSH session, while it copies. 3. The shell sends SIGHUP to the whole job: the TUI and the backend share one process group. The TUI's handler sets STOP, but the backend is killed at once, so drain() never runs. Result: a truncated file sits at the destination under its real name. This is the known 'failed copy leaves truncated file' state, now reached by an everyday action rather than by an I/O error. Separately, q mid-transfer (actions.rs:591) has no confirmation; the backend cancels the transfer after the terminal is restored, so the user never sees 'Cancelled'.
- **Fix:** Spawn the backend with .process_group(0), or have `--backend` ignore SIGHUP and SIGINT and rely on stdin EOF or the quit line. The TUI's own handler then drives quit and drain. When transfer_id > 0, make q ask for a second press, or print the drain outcome to stderr after the terminal is restored.

### S89. --preview-image publishes the jailed decoder's output without checking it is a PNG, and the window then picks a decoder by content

**Medium**, PLAUSIBLE (votes: Medium, Medium, Low). `src/previewimage.rs:74`

- **What:** The render counts as successful if the jailed thumbnailer exited 0 and wrote any non-empty file. Those bytes are renamed to <md5>.png and handed to Qt's Image in the UI process. Qt picks a decoder by content, the same behaviour the module header gives as the reason for the fix (a .jpg that is PostScript reached the kimageformats EPS plugin and so Ghostscript). The jail's output is attacker-shaped whenever the jailed decoder is compromised, so the jail boundary is only one hop deep.
- **Scenario:** A crafted PDF, video or office file exploits evince, ffmpegthumbnailer or gsf-office-thumbnailer inside bwrap. That process can still write the bound output file, so it writes an EPS document there. `flea --preview-image` renames it to $XDG_RUNTIME_DIR/flea-preview/<hash>.png and prints the path (previewimage.rs:73-77). PreviewImage.qml sets source to that file. The PNG handler refuses it, the fallback content probe picks EPS, and Ghostscript runs unjailed in qs with full user rights. That is the escape the change was meant to close.
- **Fix:** Before the rename (and on the cache-hit path), require the 8-byte PNG signature followed by an IHDR chunk, and delete the temp otherwise. Better still, re-encode inside the jail to a strict format, or validate the full chunk structure and CRC.

### S93. FileManager1 ShowItems lets a sandboxed app open a Flea window with the cursor already on a PDF it wrote, so the unjailed in-process QtPdf parses it without a keypress

**Medium**, PLAUSIBLE (votes: Low, Low, Low). `tools/flea-filemanager1:108`

- **What:** items_to_show() turns any existing path into `flea --select <path>` (line 108). main.rs:307 turns that into FLEA_SELECT, shell.qml:518 and Pane.qml:27 put the cursor on that row, and the preview column is on and automatic by default (ViewState.qml:155-156). PreviewColumn.qml:216 then loads PreviewPdf.qml (QtPdf, in-process) for a PDF row. On this box xdg-desktop-portal 1.22.1 implements OpenURI.OpenDirectory by calling org.freedesktop.FileManager1.ShowItems (strings in /usr/lib/xdg-desktop-portal: 'This is implemented using the org.freedesktop.FileManager1.ShowItems() D-Bus API'). That means any Flatpak app, with no talk-name and no dialog, can make Flea open a window with the cursor on a file of its choosing.
- **Scenario:** A sandboxed app writes a crafted evil.pdf into its own ~/.var/app/<id>/data, which sits at the same path on the host. It then calls org.freedesktop.portal.OpenURI.OpenDirectory with an fd to that file. xdg-desktop-portal calls ShowItems(['file:///home/u/.var/app/<id>/data/evil.pdf']), flea-filemanager1 spawns `flea --select …/evil.pdf`, the window opens with the cursor on evil.pdf, and QtPdf parses attacker bytes in the UI process with the user's full rights. The user pressed nothing. The same route turns every known 'on cursor arrival' hazard (slow or broken-xref PDF stalls, etc.) into something the app can trigger from outside. The known items (PDF unjailed, FileManager1 existence leak and window cap) do not cover this zero-click trigger.
- **Fix:** Treat windows opened by FileManager1 as untrusted reveals. Pass a flag such as --select-quiet or FLEA_SELECT_NOPREVIEW, so the first cursor placement does not start automatic previews: for that row, require a cursor move or Space before PDF, media or text readers load. Alternatively, have flea-filemanager1 open the parent with no selection for callers other than the user's own session tools. Moving PDF rendering into the jailed thumbnailer, as the diff already does for images, closes the root cause.

### S94. An app-supplied accept_label of any length pushes the real Cancel button off-screen and covers the 'Requested by' line, so the app can draw its own fake 'Cancel' and identity

**Medium**, PLAUSIBLE (votes: Medium, Medium, Medium). `ui/PickerChrome.qml:175`

- **What:** The accept button's label is req.accept verbatim (Picker.js:73-79). Framed's width is caption.implicitWidth with no cap and no elide (PickerChrome.qml:47, 91-103). The buttons Row is anchored only to the right (161-165), so a long label grows it leftward past x=0. The Cancel button (first in the Row) ends up outside the window, the titles Column (anchored right to buttons.left, 129-134) gets a negative width, and the accept button is drawn over where the title and 'Requested by <app_id>' were. The only visible control is then the primary button, and the app chooses what text shows on its visible right-hand end.
- **Scenario:** A Flatpak app calls OpenFile with directory=true, current_folder=$HOME and accept_label = 400 spaces + 'Requested by org.gnome.Nautilus        Cancel'. The header shows the app's forged identity and a 'Cancel' at the right edge, which is really the accept button. canAccept is true in folder mode as soon as the listing loads (picker.qml:77-78). One click on that 'Cancel' runs accept() → mark(win.path) → finish(OK) (picker.qml:229-237, 362). xdg-desktop-portal then exports the whole home directory to the app. Escape still cancels, but the visible 'Cancel' does not. This differs from the known Enter/Right-on-a-file-row item: the trigger is a visual spoof of the refusal control plus the identity line.
- **Fix:** Cap the accept label: elide it to a small maximum width, strip control and newline characters, and refuse or replace labels that equal 'Cancel', 'Close' or 'No'. Put the Row's left edge under a minimum titles width so Cancel and 'Requested by' can never be pushed out. In folder mode with nothing marked, make the button say what it grants, for example 'Share <folder name>', instead of the app's words.

### S99. A directory read that fails partway (EIO, ENOTCONN, EUCLEAN) is shown as a complete listing, often as an empty folder

**Medium**, CONFIRMED (votes: Medium, Medium, Medium). `src/backend/scan.rs:18`

- **What:** scan(), peek_line(), Search::read_one() and dirsize children() all iterate read_dir with .flatten() or filter_map(Result::ok). On Linux, std's ReadDir yields a getdents failure once as Some(Err) and then stops. So the Err that flatten drops is not one unreadable entry, as AGENTS.md:4305 assumes. It is every entry that was not read yet. The listing is then answered as a success: list returns n:0 with no error, peek returns n:0 without failed:true, search reports a normal finish, and dirsize reports an exact entries:0 next to a partial size.
- **Scenario:** Where this happens: an sshfs, rclone or cifs share whose connection drops or whose daemon dies between opendir and getdents; a USB or SD card that returns EIO on a directory block; ext4 returning EUCLEAN on a corrupt directory block. What the user sees: the pane says 'This directory is empty; add a file to see it here.' The columns view shows an empty child column rather than the 'could not read' tile. The Items column says 0 items. A directory that fails halfway shows only its first N entries, with no marker. Plausible chain to data loss: on a flaky card, the user selects all (Ctrl+A) in DCIM/100CANON, moves the files to ~/Pictures, sees the source now 'empty', and formats or discards the card. The entries that were never listed were never moved. Search also reports 'no matches' for a subtree it could not read.
- **Fix:** In scan(), iterate explicitly and return Err(from_io("scan", path, &e)) on the first Err. Alternatively return the partial Listing with a 'partial' flag that listed and peeked carry, so the client can show 'could not be read completely' instead of the empty state. In children(), return None on any Err. In search, count skipped or failed directories and report them in the searched line. Update the AGENTS.md:4305 corner note.

### S100. A search's final rank reorders every row while the client still acts on discovery-order indices, so Esc then Delete trashes a different file

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/searchreq.rs:42`

- **What:** While a search runs, the client shows rows in discovery order and allows Delete, dd, cut, copy and the menu on them. Ops.trash sends row indices (Backend.qml:179), and Focus.js has no searchRunning gate. finish_search() ranks the listing, which is a full permutation, before writing 'searched'. This happens on a natural finish and on the searchcancel that Esc sends (Focus.js:116, Search.cancel). Any trash, paths, transfer or menuaction request that the client sends before it processes 'searched' is resolved by resolve_rows against the ranked order. In a deep search that can be a file in a completely different subfolder. The success line is 'Moved 1 item to Trash' (Ops.js trashed/items) and does not name the file.
- **Scenario:** The user runs Ctrl+Shift+F 'keep' over a large tree. Results stream in and the cursor sits on row 0, which shows dir0000/zz_keep_2.txt. The user presses Esc to stop the walk and immediately presses Delete. The backend handles searchcancel (rank) and then trash rows:[0], which is now dir0000/zz_keep_0.txt, or with a real tree any best-ranked match anywhere in the subtree. That file goes to Trash. The UI then resets the cursor to 0 and says only 'Moved 1 item to Trash'. Ctrl+X followed by paste moves the wrong file the same way. The same race exists without Esc when the walk ends on its own within the round trip of the keypress.
- **Fix:** Give each listing generation a token: the backend bumps it on every rank, sort or list and echoes it in listed, searched and rows lines, and the client sends it with trash, paths, transfer and menuaction; the backend refuses a stale one ('the results were re-ordered, try again'). A smaller fix: have the client send paths (row.n joined to the base) rather than indices for destructive actions while searchMode is set, or refuse trash, cut and menu actions while searchRunning is true.

### S101. Columns view peeks the folder under the cursor on the backend's only loop thread, so passing over a stale network mount point wedges the pane

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `ui/ColumnsArea.qml:59`

- **What:** In columns view, every cursor move onto a directory row calls ask(childPath), which sends peek. run.rs:338 answers it synchronously on the request loop with peek_line -> scan -> read_dir. dirsize.rs refuses a list of unbounded mount types precisely because 'one getdents ... that never returns is not bounded'. peek has no such refusal and is not moved off the loop, so the user does not have to open the mount. With the cursor passing over a hung sshfs, rclone or tresorit mount point, or an autofs key whose server is down, all later list, window, cancel and quit requests queue behind it indefinitely.
- **Scenario:** The user is in ~ in columns view and presses j past ~/TresoritDrive (or an sshfs mount whose server went away, or /net/host under autofs). The child-column peek blocks in getdents or in the automount. The pane stops answering: navigation, window and dirsize requests get no reply until the FUSE daemon or the network times out, which may be never. A quit then orphans the backend, as in the known run.rs item.
- **Fix:** Run peek on a worker thread with an Event reply, as dirsize and meta already do, and drop stale replies by path. Before scanning in peek, also apply the dirsize refuses()/unbounded_mounts() rule to the child path and answer failed:true with a 'network mount' reason.

### S107. Holding Delete in the TUI trashes one file after another

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/tui/actions.rs:405`

- **What:** In the TUI, every Delete keypress sends `trash` for the cursor row (`m.indices()`). The TUI does not dedupe or throttle repeats, and a terminal gives no way to tell a key-repeat from a fresh press. When a trash finishes, the listing reloads and the same cursor index now points at the next file, so each later repeat trashes that file. The known held-key finding covers only the GUI (ui/js/Focus.js). The TUI is a separate code path, and the GUI's fix (checking event.isAutoRepeat) cannot work there.
- **Scenario:** The user runs `flea --tui` in a folder and holds Delete a little too long (about 1 s). Ctrl+Z restores only one file per press, and after 50 operations the ring is full, so the rest must be restored by hand from the Trash view. Pressing q loses the in-memory undo ring entirely.
- **Fix:** In the TUI, treat trash (and undo/paste/newFolder) as edge-triggered. Ignore a repeat of the same destructive key that arrives within about 500 ms of the last one or while the previous trash's reply and re-list are outstanding, and pin the request to the row's path/identity rather than its index. Alternatively, enable the kitty keyboard protocol's event-type flag (`\x1b[>3u`) where supported and drop repeat events.

### S108. The new LocalSend gate is opt-out, so a fresh install still announces on the LAN at every row right-click

**Medium**, CONFIRMED (votes: Medium, Low, Low). `ui/LocalSend.qml:33`

- **What:** The uncommitted fix skips LocalSend discovery only when the 'localsend' menu row is hidden in Settings. The shipped default hidden list (src/uischema.rs:43 DEFAULTS and ViewState.defaultMenuHidden) does not include 'localsend'. On any box with localsend-cli installed (this one has /usr/bin/localsend-cli), a menu opened on any row therefore starts a localsend-cli run, at most once per 15 s warm window. That includes folder rows, where the LocalSend row cannot even be used. The run announces this machine on multicast port 53317 and is a receiver for up to 4 s, and localsend.rs:77-79 says paired devices are accepted without a prompt.
- **Scenario:** On a café or office network, the user presses m or right-clicks any file or folder in Flea. The device alias is broadcast to the LAN and an inbound receiver runs for up to 4 s, though the user never chose 'Send with LocalSend' and may never use LocalSend. This repeats every 15 s of menu use.
- **Fix:** Make discovery opt-in: add 'localsend' to the shipped menu.hidden default (and to ViewState.defaultMenuHidden), or run discovery only when the LocalSend row itself is hovered or expanded, never on menu open. Also skip it when the cursor row is a directory.

### S111. Undo of a copy now trashes it on the destination volume, so a mistaken copy onto a USB stick or share stays there as a hidden .Trash-1000 entry

**Medium**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/undo.rs:247`

- **What:** The uncommitted fix changes undo of Created and Copied steps from delete to gio trash (discard(), lines 183, 193, 247-261). For a file outside the home filesystem, GIO does not move it to ~/.local/share/Trash. It trashes into the topdir of that filesystem ($topdir/.Trash/$uid or $topdir/.Trash-$uid; libgio contains '.Trash-%s' and 'Unable to find or create trash directory'). So an undone copy onto removable media or a writable network mount stays on that medium in full. Undo still reports success, and the entry is in a dot-directory that Flea hides by default.
- **Scenario:** The user copies ~/private/taxes-2025.pdf onto a FAT stick by mistake while preparing it for someone else, sees the error and presses Ctrl+Z. Flea reports the copy undone. The PDF is now at /run/media/<u>/STICK/.Trash-1000/files/taxes-2025.pdf. On FAT or exFAT the 0700 mode means nothing. The stick is ejected and handed over with the file on it. On a CIFS or NFS share where GLib can create .Trash-1000, the file stays on the server, where server-side ACLs apply rather than the client's 0700. Before this diff, the same undo removed the copy.
- **Fix:** In discard(), compare st_dev of the path with st_dev of the home trash (XDG_DATA_HOME/Trash). Trash only when they match. On any other filesystem, delete the item the undo created, as before; the identity checks on Copied already make sure it is the operation's own copy. Otherwise, say in the undo message that the item went to that volume's trash. Also consider an 'empty this volume's trash before eject' prompt like Nautilus has.

### S115. With nothing chosen, Move, Copy, Zip and Send also take every pinned row, even when the card says "Empty"

**Medium**, CONFIRMED (votes: Low, Low, Low). `shelf/Model.js:564`

- **What:** actionPaths() treats "none chosen" as the loose pile plus every pinned row (only captures are left out). ShelfCard.key() dispatches action letters whenever any row exists, pinned rows included, and does not check stripShown. So on a shelf that shows "Empty" but has pinned rows, m or t followed by 1 moves or sends every pinned item. The README says an action with nothing chosen takes "the pile". Model.tooltip, loose() and emptyShown all leave pins out of that count, on the grounds that a pinned row is not an item anybody sent. dragMoves() treats a pinned row as copy-only. The Move action does the opposite.
- **Scenario:** The user pins ~/Documents/Tax (pins act as permanent bookmarks and are listed in Settings). Later they drop one screenshot on the shelf, open the card and press t then 1 to Taildrop it to their phone. runChosen sends [screenshot, ~/Documents/Tax] to the peer, and a send cannot be undone. Same with m then 1: the recent destination is a USB stick, and the whole pinned tree is moved across devices, copied and then removed from home. With no loose items at all, the header says "Empty" while m/t/a/y still act on every pin.
- **Fix:** Make "whole" mean the loose pile: skip PINNED as well as CAPTURE in actionPaths/wholeCount. A pinned row should only be acted on when it is explicitly chosen. At minimum, when nothing is chosen and loose()==0, no action key should do anything.

### S116. The chosen set is read again when the destination is picked; if every chosen row has left the card by then, the action silently widens to the whole pile and its pins

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `shelf/Panel.qml:208`

- **What:** actOn() opens the destination or peer flyout without saving the paths it will act on. runChosen() calls Model.actionPaths(card.chosen, card.rows) again when a row (or the Choose-a-folder reply) is picked. actionPaths drops chosen paths that are no longer among the rows, and when none are left it falls back to the whole pile plus pins. While the card is open (drawing=true) the captures are re-listed every second and the pile every 5 s. A chosen capture row that is pushed out of the newest-N tray, or a chosen pile row taken off the pile by another bar's card or a pane token drop, therefore turns a one-item action into an action on everything.
- **Scenario:** The user marks one screenshot with v and presses t (the flyout says "Send 1 item to"). Before they press 1, a new capture lands: they take another screenshot, or a recording finishes, and the tray keeps only the newest N. The chosen capture is no longer among the rows, so the title quietly becomes "Send 4 items to" and pressing 1 sends the loose pile and every pinned folder to the peer. The same happens with m/c, where the destination can wait for the Choose-a-folder picker indefinitely.
- **Fix:** Save the path list in actOn (root.pendingPaths = actionPaths(...)) and use exactly that list in runChosen. If the list was a chosen subset and some of those paths have since left the pile, refuse or say so; never fall back to the whole pile.

### S121. A drag between two different GVFS locations (phone to NAS, one server to another) is treated as a move and removes the originals

**Medium**, PLAUSIBLE (votes: Medium, Medium, Low). `ui/js/Drag.js:260`

- **What:** verbFor picks move or copy by comparing st_dev, following Finder's rule that a drag across volumes copies. Every GVFS location (each SMB share, SFTP host, WebDAV server, MTP phone) lives under the single fuse.gvfsd-fuse mount at /run/user/UID/gvfs, so they all report the same st_dev. A drag between two unrelated remote volumes is therefore a Move, and the source is deleted after the transfer.
- **Scenario:** Dual pane: left pane on the phone (/run/user/1000/gvfs/mtp:host=.../DCIM), right pane on the NAS (/run/user/1000/gvfs/smb-share:server=nas,share=photos). The user drags the camera folder to the NAS to back it up. Both listed lines carry dev 0:82, so the verb is "move" and the backend runs move_any. gvfsd-fuse either moves the files itself with copy+delete, or answers EXDEV, and Flea copies and then runs remove_copied on the phone. The photos are deleted from the phone. Combined with the known discarded close(2) errors on upload-at-close backends (MTP, WebDAV), a failed upload still removes the source.
- **Fix:** Treat the gvfs FUSE mount as one volume per first path component. Either send a volume key of dev plus the first component under /run/user/UID/gvfs on the listed line, or send dev 0 for any path under a fuse.gvfsd-fuse mount so verbFor copies. Apply the same rule to tab devs.

### S122. Undo of a copy now runs three unbounded gio processes per step, including two full trash listings, on the request loop

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/backend/undo.rs:253`

- **What:** The uncommitted fix routes Created/Copied undo through trash::trash. Each step spawns `gio trash --list` twice (the whole trash, across every mounted volume) and `gio trash` once, with no timeout, all inside Journal::undo, which do_undo calls synchronously on the backend's single request loop. The UI gets no listing, navigation or preview while it runs. Cost grows with steps times trash size. A stalled mount under any path, or a gvfsd-trash stuck on one, turns the stall into an indefinite wedge. Before the diff this path used only local unlink/rmdir syscalls.
- **Scenario:** The user copies 1,000 photos into the wrong folder and presses z. Every step lists the whole Trash twice, so with an old trash of a few thousand items the window stops answering for about a minute. If the destination is on a mount that stops answering, gio's lstat never returns and the request loop is wedged until the backend is killed.
- **Fix:** Run undo on the operation thread the way redo already does (claim the slot, send a terminal message), or batch all Created/Copied paths of an entry into one trash_checked call. Put a watchdog on every gio child (the timeout helper from child.rs) and make it cancellable.

### S126. The new --sheet byte budget misses the reader's intermediates: a 160 KB .ods drives the unjailed `flea --sheet` to 815 MB and a 290 KB .xlsx to 1.1 GB, with about 3.5 GB within reach, on cursor arrival

**Medium**, CONFIRMED (votes: Medium, Medium, Low). `src/sheetods.rs:105`

- **What:** Only bsdtar runs in the jail. The XML readers run in the un-rlimited `flea --sheet` process that PreviewLines/PreviewTable spawn with no size gate and no timeout. In sheetods::rows the `<text:s text:c="256"/>` arm (lines 105-108) appends up to 256 spaces before the `text.len() >= limits.bytes` guard at line 111 is reached, so the 64 MB cell cap never applies to space runs. That is about 12 bytes of output per input byte, over a content.xml of up to MAX_PART_BYTES (256 MB). In sheetxlsx::shared_strings (lines 44-66) the whole sharedStrings.xml becomes a Vec<String> with no count or byte budget: each 5-byte `<si/>` costs a 24-byte String. The Limits added in the diff cap only the rows written out, not these structures.
- **Scenario:** A spreadsheet received by mail or LocalSend is a 600 KB .ods whose content.xml is 256 MB of `<text:s text:c="256"/>` inside one cell. Resting the cursor on it in the preview column spawns `flea --sheet <path> 15`. The process holds the 256 MB part twice (the Vec and from_utf8_lossy) plus roughly 3 GB of spaces before clip() cuts the cell to 4096 chars. On an 8 GB laptop that is enough to push the desktop into swap or the OOM killer. A .xlsx does the same with a small package whose sharedStrings holds ~50M empty `<si/>`. The same happens in a flat .fods up to 256 MB, which is read in-process with no jail at all.
- **Fix:** Move the `text.len() >= limits.bytes` check (better: `>= cell_chars` bytes) ahead of the `s`, `tab` and `line-break` arms, and stop appending to `text` once it passes cell_chars. Give shared_strings a count and byte budget, and store only indices the first rows reference, or cap each string at cell_chars. Run `flea --sheet` itself under prlimit (address-space and CPU limits, as the jail's tools already are) and give the GUI's converter a timeout.

### S3. A slow-rendering PDF page monopolises Qt's pixmap-reader thread, starving every other asynchronous image in the app

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/PreviewPdf.qml:80`

- **What:** PdfPageImage renders on the QQuickPixmapReader thread (asynchronous: true, line 87) with no time limit and no cancellation; the column's thumbnail frame (ui/PreviewColumn.qml:117-130, asynchronous Image) and PreviewImage share that single thread, so while a hostile page renders nothing else decodes. Every cursor visit, zoom step or resize (rerenderIfNeeded, lines 106-117) queues another full render.
- **Scenario:** A 10 KB valid PDF whose page draws one image through nested Form XObjects: 20^3 = 8000 draws took 10.2 s to render; 20^5 would take roughly an hour per render. The GUI stays responsive, but thumbnails and image previews stop appearing for that long, and revisiting the row re-queues it; only a restart clears the queue.
- **Fix:** Open the in-process page only for PDFs whose jailed thumbnail (evince-thumbnailer, prlimit --cpu=30, 20 s JOB_TIMEOUT) succeeded, which bounds render cost by proxy; or give PreviewPdf its own QQmlEngine/image provider so the shared reader thread is never held.

### S4. Known item is wrong as stated: local playlists cannot fetch remote URLs by default; the real residual is an inherited QT_FFMPEG_PROTOCOL_WHITELIST

**Low**, CONFIRMED (votes: Low, Low, Low). `src/gui.rs:49`

- **What:** The earlier finding "media playlists (.m3u) may fetch remote URLs via FFmpeg on Space" does not hold on this build: Qt 6.11.2's ffmpeg plugin opens a local file through FFmpeg's `file` protocol, which installs the default whitelist `file,crypto,data`, and FFmpeg 9.0.1 rejects every nested http/rtp open from HLS (.m3u/.m3u8/.vlc are the AUDIO kind), DASH manifests disguised as .mp4 (probed by content) and SDP (.sdp is the VIDEO kind via generic-icon) before any socket. FFmpeg 9 additionally refuses to probe HLS under a non-playlist extension. The plugin does honour QT_FFMPEG_PROTOCOL_WHITELIST from the environment (string present in /usr/lib/qt6/plugins/multimedia/libffmpegmediaplugin.so) and qs_command passes the inherited environment to Quickshell untouched, so a session exporting a wider list silently re-enables remote fetches from any .mp4/.m3u on Space.
- **Scenario:** Today: Space on a hostile .m3u/.mp4/.sdp fails with "could not be played" and no packet leaves. If the user (or a wrapper, or a future Flea change wanting remote streams) sets QT_FFMPEG_PROTOCOL_WHITELIST to include http/https, a DASH manifest saved as holiday.mp4 phones home to an attacker host the moment Space is pressed, leaking the IP and the fact the file was opened.
- **Fix:** In src/gui.rs qs_command add `cmd.env("QT_FFMPEG_PROTOCOL_WHITELIST", "file,crypto,data")` (or just `file`) unconditionally, note it in AGENTS.md as load-bearing, and update the known-issues list to say the default already blocks and this pins it.

### S5. TUI Sixel preview forwards jailed decoder stdout to the terminal unvalidated

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/tui/mod.rs:312`

- **What:** On a Sixel terminal the bytes that ImageMagick/pdftoppm emit inside the bwrap jail are written verbatim to the TUI's stdout, so the jail's only output channel doubles as an unfiltered terminal-escape channel; the Kitty path is safe because it base64-encodes the same bytes.
- **Scenario:** User previews a crafted image or PDF in `flea --tui` on a Sixel-capable terminal (foot, wezterm, contour, xterm). If the decoder is compromised by the file (the exact class the jail exists for), its stdout can contain OSC 52 (clipboard write), title/mode changes or terminal queries; src/tui/mod.rs:312-317 writes `image` with `write_all` unchanged. Replies to injected queries land in the TUI's input decoder (string-control replies are discarded, but CSI replies are mapped to keys by `csi()`).
- **Fix:** Before writing a Sixel frame, verify it is exactly one DCS sixel sequence: starts with `\x1bP`, parameters then `q`, body restricted to 0x3F-0x7E plus `#`, `$`, `-`, `!`, digits and `;`, ends with `\x1b\\`, nothing after; drop the frame (and report 'Image preview: bad stream') otherwise. Alternatively have the jail emit PNG only and sixel-encode in-process.

### S6. TUI starts an unjailed mpv on cursor movement, so playlists fetch remote URLs without a keypress

**Low**, PLAUSIBLE (votes: Low, Medium, Medium). `src/tui/mod.rs:174`

- **What:** With the shipped defaults (`preview.column: true`, `preview.loadOn: automatic`) the TUI launches mpv on the row under the cursor as soon as it is a media kind; mpv runs with the user's full rights (not under bwrap like the image/PDF jobs) and a .m3u/.vlc playlist (audio/x-mpegurl, hence an audio icon) is resolved by mpv, which still has http/https enabled (`--ytdl=no` only disables youtube-dl). The already-known item says this happens 'on Space'; in the TUI it happens on j/k.
- **Scenario:** User opens a downloaded folder in `flea --tui` and arrows past `mix.m3u` containing `http://attacker/x.mp3`; src/tui/mod.rs:174-202 calls `media::Player::start` immediately (preview_loaded is true because loadOn defaults to automatic, src/uischema.rs:36), mpv opens the playlist, resolves the entry and contacts the attacker host (IP/time-of-day leak, tracking), and any ffmpeg demuxer bug in the media itself runs unjailed.
- **Fix:** Do not auto-start the player: require the explicit `preview`/`loadPreview` action for audio/video (treat `loadOn: automatic` as text/metadata only, like the GUI column). Skip playlist MIME types (audio/x-mpegurl, application/vnd.apple.mpegurl, audio/x-scpls, application/xspf+xml) entirely, and add `--no-ytdl --network-timeout=0` plus `--demuxer-lavf-o=protocol_whitelist=file,pipe` or run mpv through the same bwrap wrapper with `--unshare-net`.

### S11. FileChooser impl calls are not bound to xdg-desktop-portal: any session-bus peer can put arbitrary text under 'Requested by' and can Close another app's request

**Low**, CONFIRMED (votes: Low, Low, Low). `tools/flea-portal:308`

- **What:** on_call() ignores _sender. app_id is taken from the call and drawn as 'Requested by <app_id>' (ui/js/Picker.js:68-69, PickerChrome.qml:148-158), the one line the design log calls a portal-vouched identity. The Request object exported at each handle accepts Close from any peer.
- **Scenario:** An unsandboxed process (or a Flatpak with --socket=session-bus / a talk-name grant) calls org.freedesktop.impl.portal.desktop.flea OpenFile directly with app_id 'org.mozilla.firefox' and a Firefox-like title; the chooser shows a desktop-vouched-looking identity the desktop never vouched for. The same peer can enumerate handles (they embed the sender's unique name and a token) and call Close on a live request of another app, which makes that app's chooser vanish with response 2. Same-user only, so the file-access gain is nil; the harm is the trust label and a denial of another app's dialog.
- **Fix:** At the start of on_call, compare _sender with the current owner of org.freedesktop.portal.Desktop (one GetNameOwner call, cached per owner change) and return AccessDenied otherwise; apply the same check to Request.Close.

### S12. Idle-exit race: on_quit() quits unconditionally 500 ms after releasing the name, orphaning a request that was dispatched in between

**Low**, CONFIRMED (votes: Low, Low, Low). `tools/flea-portal:365`

- **What:** on_idle() releases the bus name and schedules on_quit() 500 ms later; on_quit() calls loop.quit() without checking self.picks. The exported object stays registered on the connection, so a method call already routed to this owner (queued before the daemon processed ReleaseName) is dispatched to on_call, spawns a picker and registers a Request, and then the process exits. cancel_idle_exit() cannot cancel the on_quit source because it is not tracked in idle_source.
- **Scenario:** An app opens a chooser at exactly the 30 s idle boundary. The picker window appears, the user picks a file, but flea-portal is gone: the invocation dies with the connection so the app receives NoReply/response failure, the picker's reply.json and its flea-picker-* directory leak in $XDG_RUNTIME_DIR, and the window lingers until the user closes it. FileManager1 has the same shape but its answer() completes synchronously inside on_call, so only the portal is affected.
- **Fix:** In on_quit(), if self.picks is non-empty, re-own the name (Gio.bus_own_name again) and return without quitting; alternatively keep the name until the loop is really idle and use GDBus's own exit-on-close after ReleaseName, or track the quit source so on_call can cancel it.

### S13. SaveFiles answers with the folder URI instead of one URI per requested file, and never shows or collision-checks the names the app will write

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/picker.qml:90`

- **What:** mode 'savefiles' is treated as a plain folder pick (folderMode), the request's files[] (decoded in flea-portal:158) is never read by the window, and finish() returns the marked folder's URI; flea-portal answer() (283-292) forwards it verbatim. The impl interface (org.freedesktop.impl.portal.FileChooser.SaveFiles, /usr/share/dbus-1/interfaces) requires 'the uri corresponding to each file given by options, in the same order' and allows the backend to uniquify colliding names. The user therefore approves a folder without seeing which names will land in it or whether they overwrite something.
- **Scenario:** An app calls SaveFiles(files=[a.txt, b.txt]). The user picks ~/Documents. Flea returns [file:///home/u/Documents]. The frontend registers the returned URIs for the app as save targets (for a sandboxed app: a named document whose parent is /home/u and name Documents), so the app gets no per-file handles and, depending on the document portal's handling of a directory as a named document, either an error or a doc that is not what it asked for; a host app expecting uris[0] to be a.txt's path gets the directory. tests/portal.sh:169-171 only asserts the files decode, not the answer shape.
- **Fix:** For mode 'savefiles', build folder/name for each files[] entry in finish() (refuse names that fail validName), run the same save/review identity check per name so existing files get the 'Use this location' review (or a uniquified name), list the names in the save strip, and return the URIs in request order.

### S14. FileManager1 stats caller-supplied paths on its main loop, so one URI on a hung mount stalls every 'Show in folder' on the box

**Low**, CONFIRMED (votes: Low, Low, Low). `tools/flea-filemanager1:105`

- **What:** items_to_show() calls os.path.lexists(path) and folders_to_show() calls os.path.isdir(path) synchronously inside the D-Bus handler of a single-threaded service. A path under a wedged gvfs/FUSE, NFS or MTP mount blocks the stat for as long as the kernel waits, and every other FileManager1 call queues behind it.
- **Scenario:** A download completes into a gvfs-mounted share whose server has gone away; Chromium calls ShowItems on it. The stat hangs, Chromium's call times out, and until it returns every browser/app reveal on the box hangs too. The same stat is what the earlier review flagged as the path-existence leak; main.rs:307-310 already says a missing --select target opens the parent with nothing selected, so the check buys nothing.
- **Fix:** Drop the lexists/isdir checks and hand the path to 'flea --select' / 'flea <dir>' as-is (the window already copes with a missing or non-directory target), or do the check in the child. This also closes the known path-existence leak.

### S16. One unreadable Trash entry makes the whole Trash view (and the sweep) fail closed

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/trashbrowse.rs:115`

- **What:** parse_list returns Err for the entire listing when any line lacks a trash:/// URI or an absolute original (trashbrowse.rs:105-127). gio prints an entry that has no .trashinfo as "trash:///name\t(null)", so a single orphan in Trash/files makes every Trash op fail: list, select, prepare, restore (all), valid(); Empty Trash, Restore and the daily sweep are all unavailable until the orphan is removed with another tool. During any Flea Trash-view delete the quarantine directory Trash/files/.flea-delete-<pid>-<n> is itself such an entry, so another Flea window (or a mid-operation refresh) listing the trash gets this error transiently. The undo-side parser (src/backend/trash.rs parse_list) skips such lines instead of failing, so undo is unaffected.
- **Scenario:** A file lands in ~/.local/share/Trash/files without a .trashinfo (manual move, a broken trasher, or a crash between gio's info write and its rename). Opening Trash shows 'GIO returned an invalid Trash identity.'; Empty Trash and Restore do nothing; with auto-empty on, the sweep fails every day and records nothing.
- **Fix:** Skip (and count) rows without an absolute original instead of failing the whole listing; show them as 'N items could not be read' and refuse only actions targeting them. Ignore names matching ^\.flea-delete-\d+-\d+$ in the listing. Keep the strict rule for rows that ARE acted on (identity check already covers that).

### S17. Unreplayable recovery journals persist forever with no way to resolve them from the UI

**Low**, PLAUSIBLE (votes: Low, Low, Medium). `src/backend/trashdelete/recovery.rs:103`

- **What:** replay() refuses when the source directory's dev/ino no longer match (recovery.rs:103), when the quarantine vanished without a completion marker (:111), or when the original name is taken again (:171, RENAME_NOREPLACE). All three are correct fail-safe choices, but the journal is then preserved forever: every Trash open reports 'Interrupted file operation needs attention' with the raw error, the payload stays in a hidden .flea-delete-* directory, and nothing in the UI lets the user keep or discard it.
- **Scenario:** Permanent delete on a USB stick is interrupted; the stick is re-plugged later and gets a different st_dev (or the folder is on a drive moved to another machine). Every Trash open shows the error; the preserved files are only reachable by hand-renaming <parent>/.flea-delete-<pid>-<n>/payload. Likewise if the user recreated a folder with the deleted name before recovery ran.
- **Fix:** For a journal that fails validation, include the quarantine path and the preserved item count in the notice and offer a resolution: 'Keep' (rename payload to <name>.recovered beside the quarantine and complete the journal) or 'Dismiss' (complete the journal, leave the directory). At minimum document the manual recovery in the notice.

### S19. dirsize refusal of an unbounded mount still lstat()s and read_dir()s the mount on the walker thread

**Low**, CONFIRMED (votes: Low, Medium, Low). `src/backend/dirsize.rs:120`

- **What:** walk_cancellable refuses an unbounded mount at :117 but then calls path.symlink_metadata() (:118) and children(path) = read_dir (:120) on that very mount, and dirsizereq.rs:131 mtime_of() lstats it first; those are the calls the refusal exists to avoid. The loop is protected by the generation bump (run.rs:391-401), but a refused answer is partial and never cached (dirsizereq.rs:166), so every re-list of the parent queues the row again and spawns another thread that hangs.
- **Scenario:** Folder sizes are on by default (ui/ViewState.qml:81). With ~/TresoritDrive's server not answering, every visit to ~ (or every changed/re-list of it) spawns a walker thread that blocks in lstat/getdents on the mount; the next navigation abandons it (dirsize_running=false) and the next visit spawns another. Over a session this leaks blocked threads (each with a 2 MiB stack reservation, possibly in D state) and makes the Items/Size cells for that row wait on a hang instead of answering at once. No wedge of the loop, no data loss.
- **Fix:** On refusal answer { bytes: 0 or the cached st_size, partial: true, entries: None } without touching the path, and skip mtime_of for a path that refuses(); if a count is wanted, do the read_dir under a short bounded helper thread instead of the walker itself.

### S20. No cap on search matches; a one-letter deep search accumulates every path in RAM and sorts it on the loop

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/search.rs:72`

- **What:** Every match is pushed into the listing arena and scores (search.rs:70-73) with no upper bound, and finish_search ranks the whole set on the loop thread (search.rs:85-104). The client refuses only an empty query (ui/js/Search.js:31-34); a one-character query matches nearly everything. Beyond 4 GiB of names the arena's u32 offsets wrap (listing.rs:32-42) and name() slices garbage or panics, taking the backend down rather than erroring.
- **Scenario:** Measured in an isolated backend on a synthetic tree: 200,000 matches cost +9.6 MB RSS (5.3 MB -> 14.9 MB VmHWM) and 86 ms, so it scales linearly; a deep search for 'a' from / over a few million files is a few hundred MB on the backend plus a multi-second rank() sort during which the loop answers nothing. The 4 GiB wrap needs ~50M matches (a large NAS), so it is theoretical for a desktop.
- **Fix:** Stop the walk at a result ceiling (e.g. 100k) and say so in the searched line (a truncated flag the strip can draw), and guard Listing::push with a hard error when names.len() would exceed u32::MAX.

### S21. Loop-thread stats reach paths the user did not open: symlink targets in the window and recently-used paths in listpaths

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/listpaths.rs:330`

- **What:** Two loop-thread stats follow into other trees: meta.rs:41-46 stat()s and readlink()s a symlink's target for every symlink row in the window (write_window, run.rs:233/251), and listpaths.rs:330 lstat()s every absolute path the picker's Recent hands over from recently-used.xbel, a file every desktop application writes. The stat_range half is a documented, accepted trade-off (AGENTS.md ~4436-4458); the listpaths half is not covered by that note. Neither is changed by the uncommitted diff.
- **Scenario:** A symlink in ~ pointing into a gvfs/NFS mount whose server is gone, or a Recent entry under such a mount (or under an autofs trigger, which the stat then mounts over the network), blocks the request loop for as long as the kernel waits; with a hard NFS mount that is indefinitely. The exposure is bounded to the ~350 rows of one window and to the Recent list, and there is no data loss.
- **Fix:** Same asynchronous stat path for both call sites, or at least skip paths under dirsize::unbounded_mounts() in listpaths (a Recent entry on a hung share is not worth a stalled loop) and stat symlink targets under those mounts as target_is_dir=false.

### S22. A failed extract of an archive holding a read-only directory leaves a hidden .flea-work-ext-* tree in the browsed folder

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/archivework.rs:53`

- **What:** Work::drop removes the staging directory with a bare remove_dir_all; when bsdtar restored a directory with no owner write bit (its deferred permission fixup runs even on a failed exit), the removal fails and the whole partially extracted tree stays behind under a dot-name in the operator's folder. copyfile.rs has owner_can_write/remove_tree for exactly this case but Work does not use it.
- **Scenario:** Extract an archive that carries a 0500 directory member and, later, a '..' member (or any member bsdtar refuses): bsdtar exits 1, Flea reports the failure and publishes no destination, but /<folder>/.flea-work-ext-<pid>-<n>/out/locked/inside.txt remains, hidden, unremovable without chmod, and never cleaned by later runs ('Nothing is ever removed here'). Repeated attempts each leave another copy of everything extracted before the bad member.
- **Fix:** In Work::drop, on remove_dir_all failure walk the tree with O_NOFOLLOW|O_DIRECTORY descriptors restoring 0o700 on directories (reuse copyfile::owner_can_write / remove_tree) and retry; consider also sweeping stale .flea-work-<tag>-<otherpid>-* whose pid no longer exists.

### S23. Compress archives the wrong content for a selected file whose name begins with '@'

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/archive.rs:83`

- **What:** compress_argv terminates option parsing with '--' before the member names, but bsdtar's '@name' is a file-argument convention, not an option: in -c mode it opens 'name' as an archive and adds that archive's members instead of the file. The '--' guard does not cover it.
- **Scenario:** The operator selects '@inner.tar' (beside an ordinary 'inner.tar') and compresses to zip: the produced archive contains inner.txt, the member of inner.tar, and not the selected file. With no sibling of the un-prefixed name the compress fails with 'bsdtar: Error exit delayed from previous errors' rather than archiving the file.
- **Fix:** Prefix each relative member name with './' in compress_argv (bsdtar only treats a leading '@' specially), or refuse names beginning with '@' for the bsdtar formats with a plain message; 7z already receives absolute paths and is unaffected.

### S24. A cross-device move that has to keep changed entries leaves an unjournaled complete copy and a hollowed source, while the message says the original was kept

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/opsreq.rs:326`

- **What:** move_any (copyfile.rs:296-299) removes every unchanged source entry and returns kept_error when anything changed; one_item's Err arm journals only p.partial, which is None because the copy succeeded, so the finished destination tree is not in the undo journal, the item is pushed to retryPaths (where a retry fails with 'already exists'), and movesource.rs:95's wording ('the original was kept beside the copy') describes a source that now holds only the changed entries.
- **Scenario:** Move a folder to a USB stick while a download inside it is still being written: the unchanged files are deleted from the source, the growing file stays, the item is reported as failed with 'the original was kept beside the copy', undo has nothing to reverse for it, and the operator is left with a full copy on the stick plus a source folder containing one file, believing the original is whole.
- **Fix:** On kept>0 still push undo::copied(src, dst, source) so undo trashes the destination copy, mark the item as a partial success rather than a failure, and word it as 'N items that changed during the copy were left in <src>; everything else moved'.

### S25. Duplicate cannot be cancelled and blocks every other write operation until it finishes

**Low**, CONFIRMED (votes: Low, Medium, Low). `src/backend/opsdispatch.rs:166`

- **What:** start_duplicate claims the one-at-a-time slot with ops.claim_transfer() but discards the cancel flag it minted; ops::duplicate (ops.rs:83-86) runs copy_any against a private AtomicBool that nothing ever sets, and no transferstarted line is emitted so the client shows no card with a cancel. A transfercancel for that id sets a flag nobody polls.
- **Scenario:** Ctrl+D on a large folder (tens of GB): the copy runs to completion or to disk-full with no way to stop it, and every rename, mkdir, new file, trash, paste and undo in the meantime answers 'an operation is already running'.
- **Fix:** Pass the claimed cancel Arc into ops::duplicate's Progress (the cancel path already removes the partial tree), and emit transferstarted/transferdone so the status card and its cancel apply, or refuse to duplicate a directory tree above a size without the transfer card.

### S26. Rename on rclone/megafs/gvfs-WebDAV mounts falls back to a synchronous copy-then-remove on the backend's request loop

**Low**, CONFIRMED (votes: Low, Low, Medium). `src/backend/opsdispatch.rs:187`

- **What:** do_rename runs ops::rename on the reader loop (run.rs:329); renamecompat::rename_path (renamecompat.rs:51-57) turns an EINVAL on fuse.rclone directories / fuse.megafs, or EIO under /run/user/*/gvfs/dav:, into copy_then_remove, which copies the whole tree with no progress, no cancel and no slot claimed, while the loop is blocked.
- **Scenario:** F2 on a 20 GB folder on an rclone mount: the backend re-downloads and re-uploads the tree before answering; until then no listing, thumbnail, cancel or quit request is processed and the window shows a pending rename with nothing to stop it.
- **Fix:** When rename_noreplace answers EINVAL/EIO on one of the measured mounts, hand the fallback to the operation thread with the transfer card (claim_transfer, progress, cancel) instead of running it inline; keep the inline path for the atomic rename only.

### S28. Every ui.json patch, favourites edit and lastPath update is passed on a child's argv, visible to all local users

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/ViewState.qml:314`

- **What:** ui.json is written by spawning `flea --ui-state '<json patch>'` and favourites by `flea --favourites '<json op>'`; shell.qml:120 calls ViewState.rememberLastPath on every pane path change, so each folder the operator visits, and every favourite label/path, briefly appears in /proc/<pid>/cmdline. The state file itself is 0600 in a 0700 dir, but the write path is not private.
- **Scenario:** Another local account (or any process in a shared /proc namespace: this box mounts proc without hidepid) runs `ps -ef` in a loop or watches /proc and reads `{"lastPath":"/home/u/Documents/Divorce"}` and `{"op":"add","record":{"label":"NAS","path":"smb://…"}}` as they are written. Same for `flea --open <path>` and `flea shelf add <paths>`, though that is common to every desktop file manager.
- **Fix:** Have `flea --ui-state` and `flea --favourites` accept the JSON on stdin (e.g. when the argument is `-` or absent-with-stdin), and write it from QML with Process.stdinEnabled + write(); or batch lastPath writes and send them through the already-open backend pipe as a `uistate` request.

### S29. replace_file renames over hypr bindings.lua and mimeapps.list without fsync

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/userfile.rs:64`

- **What:** userfile::replace_file/write_new writes the temp file and renames it over the real target with no sync_all, unlike uistore::write_new (uistore.rs:210) which syncs before rename. Callers: hyprkeys.rs:33/43/56 (~/.config/hypr/bindings.lua), defaults.rs:87/104 (mimeapps.list), chooser.rs:78/104 (portals.conf).
- **Scenario:** `flea --default` appends its block to bindings.lua and the box loses power or crashes within the writeback window. On filesystems without a rename-flush heuristic the rename can be journaled before the data, leaving a zero-length or truncated bindings.lua; Hyprland then starts with no user bindings (hyprland.lua requires the file) until the operator restores it. ext4's auto_da_alloc mitigates this, btrfs/xfs behaviour depends on version and mount options.
- **Fix:** Call `file.sync_all()` after write_all in userfile::write_new (and optionally fsync the parent directory), matching uistore.

### S30. Run script: a second script chosen while one is still running is silently dropped and a failure is reported under the wrong name

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/Scripts.qml:34`

- **What:** Scripts.run reuses one `runner` Process: it overwrites runner.script and runner.command, then sets running = true. Quickshell's Process.running setter starts a process only when none is running, so while a long script (batch convert, OCR) is still executing a second Run script pick does nothing and says nothing, and the first script's eventual non-zero exit is attributed to the second script's name in the status line.
- **Scenario:** Operator runs `convert-to-webp` on 200 photos (takes a minute), then picks `ocr.sh` on a PDF. Nothing happens for the PDF, no message. When convert-to-webp fails with a stderr line, the status centre prints 'ocr.sh · <that line>'.
- **Fix:** Guard `if (runner.running) { root.said(name + " is still running.", true); return }` before assigning, or instantiate one Process per run via a Component so scripts can run concurrently and each reports under its own name.

### S33. Production window exposes a read-everything IPC seam to every same-user process

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/shell.qml:539`

- **What:** ui/shell.qml instantiates Flea.Ipc unconditionally, so the IpcHandler target "flea" (ui/Ipc.qml:40) is live in every production window, reachable by any same-user process with `qs -p /usr/share/flea/ui ipc call flea <fn>` (the tests do exactly this via omarchy-drive, tests/ui.sh:132). The seam is read-only but returns content, not just geometry: previewText() (Ipc.qml:502) returns the full text of the file shown in the Quick Look (FileView loads up to 500 MB, ui/PreviewText.qml:20), columnTextLines() the columns preview text, keyDeliveryState() (Ipc.qml:72-84) the internal cut/copy clipboard paths, search and filter queries and back/forward history, providerState() (Ipc.qml:153-170) the selected paths, tailnet peer list and the raw contents of ~/.dropbox/info.json shipped as facts.dropboxInfo (src/backend/providers.rs:64-70), trashState() (Ipc.qml:218-232) trashed names with original paths, railEntries()/settingsModel() favourites and saved network places, networkUri()/networkFields() the host and username typed into the network dialog, pathBarText(), renameEditorText(), rowAt(i). The socket is $XDG_RUNTIME_DIR/quickshell/by-id/<id>/ipc.sock mode srwxr-xr-x; only the 0700 /run/user/<uid> keeps other users out, and Flatpak apps do not get xdg-run/quickshell unless granted. So this crosses no user boundary, but it makes Flea a live oracle of what the user is browsing and reading for any process in the session (a third-party Omarchy shell plugin, a helper with xdg-run access), which no other file manager offers.
- **Scenario:** A same-user process runs `qs -p /usr/share/flea/ui ipc call flea previewText` every second and receives the text of whatever document the user has open in Quick Look, plus their search queries and browsing history via keyDeliveryState, without touching any file.
- **Fix:** Wrap Flea.Ipc in a Loader with `active: Quickshell.env("FLEA_IPC") === "1"` (set by tests/ui.sh and tools/flea-*), or at minimum drop the content readers (previewText, columnTextLines, keyDeliveryState.clipboard/history, providerState.facts.dropboxInfo, trashState.rows) from the production build and send only the parsed Dropbox path from providers.rs instead of the raw info.json.

### S34. Thumbnails and preview renders are made for files on remote, removable and encrypted volumes with no local-only gate

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/js/Thumbs.js:88`

- **What:** The only thumbnail gate is the Settings choice off/images/media (ui/js/Thumbs.js:88-91, ui/js/Settings.js:330); the backend request path (src/backend/thumbreq.rs, thumbs.rs, run.rs) and src/previewimage.rs:49 carry no mount-type check, although src/backend/dirsize.rs:25-43 already has an is_unbounded()/refuses() list of network and FUSE types that du refuses. So browsing an SMB/SFTP/WebDAV share through GVFS, a LUKS/gocryptfs volume or a USB stick writes 256 px thumbnails (0600, keyed by md5 of the file URI, with Thumb::URI inside the PNG per spec) into ~/.cache/thumbnails/large and full-size renders into $XDG_RUNTIME_DIR/flea-preview. Those survive the unmount and are readable by every app with home or xdg-cache access, which most Flatpaks have. GNOME's default (show-image-thumbnails=local-only) does not thumbnail remote locations for exactly this reason.
- **Scenario:** User mounts a work SMB share via the Network dialog, opens a folder of scanned documents in grid view, then unmounts; 256 px thumbnails of every visible page (readable enough to identify the document) remain in ~/.cache/thumbnails/large indefinitely, and a Flatpak with --filesystem=home can read them.
- **Fix:** Reuse dirsize::mount_type_in()/is_unbounded() in thumbreq (and in previewimage::render) to answer 'no thumbnail' for network and FUSE mounts, and add a 'Local files only' thumbnail setting; consider also skipping /run/media and /media mounts or offering 'never cache thumbnails for removable media'.

### S35. Thumbnail cache directories are created 0755 instead of the spec's 0700

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/thumbs.rs:170`

- **What:** thumbs.rs:170 and :229 create ~/.cache/thumbnails/large and ~/.cache/thumbnails/fail/flea with std::fs::create_dir_all, i.e. 0777 & ~umask = 0755, while the freedesktop thumbnail spec requires the thumbnail directories to be 0700 and every other writer on this box obeys that. Observed here: ~/.cache/thumbnails/fail/flea is 755 while the sibling fail/gnome-thumbnail-factory is 700. The PNGs themselves are 0600 (thumbwrite.rs TEMP_MODE), so the exposure is directory listing only: on a machine whose home or ~/.cache is traversable (~/.cache is 755 here; HOME_MODE 0700 in /etc/login.defs currently blocks it at ~), other local users can enumerate how many files failed and the md5 names, and if Flea is the first app to create ~/.cache/thumbnails on a fresh account, large/ is 755 too.
- **Scenario:** On a shared machine where the home directory is 0755 (older installs, or admins who relaxed it), another user lists ~user/.cache/thumbnails/fail/flea and large/ and learns file counts and md5(URI) names, which can be matched against guessed file names.
- **Fix:** Create the cache root, large/ and fail/flea with DirBuilder::new().recursive(true).mode(0o700) (as uistore does), and chmod an existing Flea-created fail/flea to 0700 once.

### S38. Move/copy/zip/send act on whatever currently lives at a pile path; only the drag-out is inode-pinned

**Low**, CONFIRMED (votes: Low, Low). `src/shelf.rs:271`

- **What:** item_of (shelf.rs:271-278) stores only `path` and `folder`. The strip verbs take the card's path strings verbatim (shelfops.rs:27-66 transfer, 168-182 send; shelfzip.rs:19-33), whereas a drag-out mints a token bound to dev/ino and refuses a path that now names another inode (shelfdrag.rs:108-129). A file replaced under the same name after it was shelved is therefore moved, zipped or sent to a Tailscale peer without any check.
- **Scenario:** User shelves ~/Downloads/report.pdf, later deletes it and the browser saves a different, private document under the same name. `t` (send) or `m` on the pile now hands the new file to the peer or folder; the row still reads report.pdf.
- **Fix:** Record dev/ino (and size) in the pile entry at `add`/`pin` and have transfer/zip/send verify them the way `unchanged()` does at redeem, reporting "<name> is not the file the shelf was holding" instead of acting.

### S39. A shell restart or plugin reload kills an in-flight shelf action with no cancel, no settle and no cleanup

**Low**, PLAUSIBLE (votes: Low, Low, Low). `shelf/ShelfActions.qml:80`

- **What:** The action `Process` (ShelfActions.qml:80-89) is an ordinary Quickshell child; when the bar reloads its QML or `omarchy restart shell` runs, the object is destroyed and the child `flea shelf move|copy|zip` is killed. The CLI carries no signal handling (shelfops.rs:21-23) and relies on returning normally to settle the pile, remove the cancel marker, drop the zip reservation and remove the work dir (shelfzip.rs:75-88, archivework.rs Work::drop).
- **Scenario:** Cross-device move of a folder from the shelf is half-way when the user runs `omarchy restart shell` (the memory notes say plugin edits require exactly that). The engine dies mid copy_any: the source is intact (movesource.rs removes nothing until the copy is complete), but a partial tree sits at the destination under the real name and the card, when it comes back, shows nothing about it. For a zip, `.flea-work-arc-<pid>-n/` is left in the pile's common ancestor (often $HOME) and a 0-byte `shelf-<date>.zip` reservation stays in archives/, so the next zip of that day becomes `-2`.
- **Fix:** Run the action detached in its own session/process group and have the card re-attach through a status file in the state dir (the cancel marker pattern already exists), or at least stage cross-device copies under a temp name and have the next shelf verb sweep stale `.flea-work-*`/0-byte reservations.

### S43. Ctrl+E opens the volume instead of ejecting it once 'Show unmounted drives' is switched on

**Low**, CONFIRMED (votes: Medium, Low). `ui/js/Eject.js:144`

- **What:** Eject.release dispatches rows[0].action of Mounts.railMenu(entry). With the Settings switch 'Show unmounted drives' (Settings.js:388 -> places.showUnmounted -> DeviceMounts.showUnmounted) every volume row is built with volumeMenu:true (Devices.js:136), and railMenu then answers [Open, Unmount, Eject] (Mounts.js:154-161), so rows[0] is 'openVolume' and RailMenu.release routes it to devices.activate. The key the sheet advertises as Eject re-opens the stick instead, from both a listing and the rail. tests/js/mounts.js:151-176 only covers rows without volumeMenu.
- **Scenario:** User turns on 'Show unmounted drives', later copies files to a FAT USB stick, presses Ctrl+E as they always do, sees the listing 'open' (no error), assumes the stick was ejected and pulls it; unflushed page-cache writes are lost.
- **Fix:** In Eject.release pick the row whose action is 'eject' (falling back to 'unmount'/'unmountPhone'), not rows[0]; add a volumeMenu:true case to tests/js/mounts.js.

### S46. Media probe runs on any file type: a FIFO named like a clip blocks the jailed ffprobe for the whole watchdog and eats a writer's bytes

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/metareq.rs:78`

- **What:** metareq::read calls mediaprobe::probe(path) for every row the client hints as media with no regular-file check, unlike the thumbnail path (meta.rs:25 thumbnailable() refuses fifos, sockets and devices) and every other reader in the same function (imagesize and linecount go through regfile::open_regular, whose comment and the test at metareq.rs:401 state 'a meta request must never eat a pipe someone else is reading'). bwrap binds the FIFO into the jail and ffprobe blocks in open(2) until the 5 s watchdog (mediaprobe.rs:18) plus ~2 s namespace teardown; a FIFO with a blocked writer has its bytes consumed. Each cursor rest spawns a fresh unbounded thread (metareq.rs spawn), so a folder of such names holds several bwrap+ffprobe trees at once.
- **Scenario:** A tarball extracted with bsdtar contains a named pipe `trailer.mp4` (tar carries FIFOs by default). The cursor rests on it: the column shows Loading for ~7 s while a jailed ffprobe sits in open(); a script that later writes into that pipe has its first bytes swallowed by the probe. The backend loop is not blocked (own thread), so this is a stall and a pipe-integrity violation rather than a wedge.
- **Fix:** Gate the probe the way the thumbnail path is gated: `if media && path.symlink_metadata().map(|m| m.file_type().is_file()).unwrap_or(false)` (or reuse regfile::open_regular's stat) before spawning ffprobe; optionally cap concurrent meta threads.

### S47. Quick Look ignores the symlink gate the column enforces, so Space on a planted link reaches the in-process PDF/media readers

**Low**, CONFIRMED (votes: Low, Low). `ui/Preview.qml:168`

- **What:** Facts.state (Facts.js:44) returns SYMLINK before any kind test, so the column never opens a PdfDocument or asks ffprobe for a symlink row. Preview.load() instead classifies with Kinds.quickLookKind(icon, name) (Preview.qml:168, Kinds.js:122), which looks at the icon and the extension only; PreviewKeys.open (line 39) and SelectionPreview's Space handler only refuse directories. A symlink named x.pdf or x.mp4 therefore goes straight into the synchronous QtPdf load or the QtMultimedia player, and askMedia() (Preview.qml:193) runs ffprobe on the link target. This is how the known FIFO/stalled-file GUI freezes are reachable from a row the column itself refuses to preview.
- **Scenario:** An extracted archive contains `report.pdf -> /path/to/a/fifo` (or a link to a file on a hung mount). The column shows the symlink tile and asks nothing; the user presses Space to see what the link is. quickLookKind says 'pdf', PdfViewer loads the target synchronously on the GUI thread and the window freezes for good (known wedge, now with one keypress on a row that looked safe).
- **Fix:** Pass row.p into open()/follow() and let quickLookKind return SYMLINK (or the target's kind after a stat that refuses non-regular targets) so the overlay declines what the column declines; make askMedia() follow the same rule.

### S49. --terminal / TUI terminal launch: a newline in the directory name truncates the working directory xdg-terminal-exec receives

**Low**, CONFIRMED (votes: Low, Low, Low). `src/terminal.rs:33`

- **What:** open_terminal() and tui::terminal::launch() pass the canonical directory as one argv `--dir=<path>`. The reference xdg-terminal-exec on this box parses that option with `IFS='=' read -r _opt XTE__DIRVAL <<- EOF` (script lines 1252-1257), which reads only the first line, so everything after a '\n' in the directory name is dropped before it is turned into the terminal's --working-directory= or a `cd`.
- **Scenario:** User opens a terminal (Ctrl+T in the GUI, or `openTerminal`/`windowNew` in the TUI) from a folder named `Reports\nold`. Flea validates it is a directory and hands `--dir=/home/u/Reports\nold` over; xdg-terminal-exec turns that into `/home/u/Reports`. If that path exists (an unrelated sibling) the terminal silently opens there; for `windowNew` the new `flea --tui <path>` still gets the right argv path, so the terminal cwd and the Flea window disagree. Not data-destroying, but a command the user then runs in that shell runs in the wrong directory.
- **Fix:** Refuse (with the existing 'could not be opened' sentence) or fall back to the parent when the canonical path contains '\n' or '\r', since the option syntax cannot carry it; alternatively pass the directory through `--` plus `sh -c 'cd "$1" && exec ...'` is not available here, so a refusal is the honest option.

### S50. Bulk rename SIGKILLs a suspended $EDITOR and can leave the /tmp scratch directory behind

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/tui/terminal.rs:160`

- **What:** EditorJob::status() treats a stopped child (Ctrl+Z inside the editor) as an error; run_editor() then calls close(), which sends SIGKILL to the editor's whole process group and never resumes it first. The editor cannot flush or remove its swap/backup files, so NamesFile::cleanup() removes `names` but remove_dir() of /tmp/flea-rename-XXXXXX fails on the leftover (e.g. `.names.swp`), reported as 'Bulk rename scratch files remain at ...'.
- **Scenario:** User multi-selects files, presses rename, and in nvim/vim presses Ctrl+Z out of habit. Flea reports 'Bulk rename editor suspended; rename cancelled', kills the editor with SIGKILL, and a 0700 /tmp/flea-rename-* directory containing the editor's swap file (holding the list of selected file names) stays on disk until reboot. Repeated occurrences accumulate; no data loss on the browsed files themselves.
- **Fix:** On a stopped child send SIGCONT then SIGTERM and wait briefly before SIGKILL (or simply resume it with SIGCONT and keep waiting, treating suspend as 'still running'), and make cleanup() remove the scratch directory recursively since it is Flea's own mktemp -d.

### S51. flea shelf choose shares one fixed reply.json between concurrent invocations

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/shelfplaces.rs:98`

- **What:** choose() always uses $XDG_STATE_HOME/omarchy/flea-shelf/reply.json: it unlinks it, spawns `flea --pick <that path>`, waits for the picker to exit, reads it, unlinks it. Two choosers started before either finished (two shelf cards, or a card plus a keybind) write and read the same file, so one can read the other's answer or find it already deleted.
- **Scenario:** Two 'Choose a folder' flyouts are open (e.g. two shelf actions launched from two bar instances after a shell restart). Picker A finishes and its reply is read; picker B's reply then lands in the same path. If A's read happens after B's write, the file A moves to is the folder chosen for B, and B later finds no reply and reports 'the chooser answered nothing'.
- **Fix:** Give each choose() its own mkdtemp-style directory under XDG_RUNTIME_DIR (as tools/flea-portal does) or include the pid in the reply file name.

### S55. Large SaveFiles/filters option makes the chooser fail outright: request rides in one env var past MAX_ARG_STRLEN

**Low**, CONFIRMED (votes: Low, Low, Low). `tools/flea-portal:194`

- **What:** The whole portal request (title, files list, every filter glob) is JSON-encoded into the single FLEA_PICKER environment variable of the qs child. Linux caps one argv/env string at 128 KiB, so a SaveFiles call with a few thousand names, or a filters list of that size, makes spawnv fail with 'Argument list too long'; the portal answers RESPONSE_OTHER and no window ever opens. The request-scoped mkdtemp already exists and could carry the request as a file.
- **Scenario:** A browser or gallery Flatpak bulk-saves ~4000 images through SaveFiles (or an app hands over a very long filter list). Reproduced on a private bus with the stub picker: 4000 x 'photo-NNNNN-with-a-longish-name.jpeg' -> 'flea-portal: the picker did not start (Failed to execute child process ... (Argument list too long))', response 2 in 0.6 s, the app's save fails with no dialog. Requests from other apps are unaffected.
- **Fix:** Write the request to request.json inside the per-request mkdtemp (0700, already removed by clean()) and pass its path, keeping FLEA_PICKER only as the dev seam; or truncate/limit files and filters before encoding and tell the picker the list was cut.

### S59. Permanent delete inside a folder reached through a symlink is refused with a 'Trash directory' error

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/trashdelete.rs:217`

- **What:** snapshot_tree() and PathReview::delete_after() open the item's parent with open_dir(), which sets O_NOFOLLOW|O_DIRECTORY (trashdelete.rs:182-188). If the listing base is a symlink to a directory (Flea does not canonicalise the list path), the open fails and the menu's Delete permanently answers 'Could not open Trash directory <folder>: Not a directory' for an ordinary folder. Fail-closed, but the action is unavailable and the message points at the wrong subsystem.
- **Scenario:** ~/link -> ~/real; the user navigates into ~/link, picks Delete permanently on a file: the dialog never opens and the card shows 'Could not open Trash directory /home/u/link: Not a directory (os error 20)'.
- **Fix:** Resolve the parent with canonicalize() (or open the parent without O_NOFOLLOW and pin its identity) before the O_NOFOLLOW walk, and word the error per caller ('Could not open folder …').

### S61. Watch pump drains an overflowed inotify queue at 8 KB per 100 ms, so a 0.2 s burst produces 6.6 s of `changed` lines and ~16 full re-lists after the writer has stopped

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/watch.rs:276`

- **What:** pump() does one blocking read of at most BUF=8192 bytes (~250 events) and then sleeps COALESCE=100 ms, so it drains at ~2,500 events/s while the kernel queue holds up to max_queued_events=16384; a burst that overflows the queue keeps yielding one Changed per 100 ms until the backlog is gone, long after the directory stopped changing, and the client re-lists on each 400 ms settle.
- **Scenario:** User has a 20k-file folder open (e.g. a download/extract target). A tool creates 20,000 files in 212 ms (measured). Backend emits 67 `changed` lines spread over 6.6 s; ui/PaneWire.qml watchSettle (400 ms, not restarted) therefore re-lists the 20k directory about 16 times (scan+sort+window+thumb/dirsize re-requests) after the burst is over. Events dropped by IN_Q_OVERFLOW (wd -1) are silently discarded by descriptors()/is_current, which is correct for eventual consistency but the trailing storm is pure waste and, with dirsizes forgotten on every structural burst, the Size column blanks and refills repeatedly.
- **Fix:** Open the fd with IN_NONBLOCK and, after the first blocking-style wakeup (poll), read until EAGAIN before sending one Changed and sleeping; alternatively use a larger buffer (e.g. 64-256 KB) and treat an IN_Q_OVERFLOW (wd == -1) event as 'drain everything now and send one Changed for the current wd'.

### S63. Search and folder-size walks cross every mount boundary, including autofs triggers, so a deep search or a size walk mounts and reads shares the user never opened

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/search.rs:53`

- **What:** Search::read_one() and dirsize::walk_into() call read_dir/lstat on every subdirectory regardless of filesystem; neither checks st_dev nor the mount type 'autofs', so an autofs indirect map in browse mode (/net, /misc, or any admin-configured map) is triggered by the walk, and the search runs on the request loop itself.
- **Scenario:** Ctrl+Shift+F from / or from a parent that contains an autofs map directory. read_one() read_dir's the browse-mode map root, gets the export keys with d_type DIR, and read_dir('/net/host') triggers the automount of an NFS export (network traffic, credentials prompts via the automounter, and a lookup that blocks until mount timeout). Because the search walk runs on the one request thread (run.rs:117 tick_walkers), the whole backend, including searchcancel handling, is stalled for the automount's duration; the same happens on the walker thread for a dirsize row.
- **Fix:** Stop both walks at device boundaries (compare DirEntry::metadata().dev() with the root's dev) or at least add "autofs" to is_unbounded() and check mount type for the search root and each descended directory the way dirsize already does; move the search walk off the request loop (its own thread with the existing generation/stop pattern) so a blocked read_dir cannot wedge cancel.

### S65. A fifo under an image or archive name hangs Convert/Extract forever and leaves a hidden .flea-work-* directory in the browsed folder

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/archivework.rs:63`

- **What:** The Convert and Extract menu rows are gated by the row's name only (icon from MIME regardless of st_mode; rowIsArchive is !d && isArchive(name)), and run_boxed blocks in Command::output() on the jailed tool. A fifo named photo.png or pack.tar makes magick/bsdtar block on open inside bwrap; the CPU limit never fires on blocked I/O, there is no cancel for archive/convert jobs, and the Work guard is never dropped. bsdtar extracts FIFOTYPE members, so a hostile archive can plant such a fifo in the folder the user just extracted into.
- **Scenario:** mkfifo p.png; open the context menu on it and choose Convert (offered, icon is image-x-generic). Reproduced through the isolated backend: 'convertstarted' is answered and no 'convertdone' ever follows; same for Extract on a fifo named p.tar. After quit the folder holds .flea-work-cvt-<pid>-1 and .flea-work-ext-<pid>-2. The status bar keeps 'Converting p.png' for the life of the session and the thread is leaked.
- **Fix:** Before spawning, open the input with regfile::open_if_regular (O_NONBLOCK|O_NOFOLLOW) and refuse anything that is not a regular file, for extract, convert and archive_produced_count alike; add a wall-clock deadline (kill the child) so a stalled tool answers an error and drops Work.

### S66. Quit while an extract, compress or convert is running orphans its .flea-work-* directory with a partial tree inside

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/run.rs:432`

- **What:** drain() waits only for ops.live (the transfer slot). Archive and convert jobs run on detached threads that never claim the slot (archivereq.rs:125-131 claim_id + thread::spawn), so on quit the process exits under them: bwrap --die-with-parent kills the tool mid-write and Work::drop never runs. This adds two triggers (quit, and the stall above) to the known 'failed extract of a read-only directory leaves a hidden work tree' finding, and the leftover here can be a large half-extracted tree of attacker-named files, hidden from the listing until dotfiles are shown.
- **Scenario:** Start extracting a multi-GB archive, then close the window (or the GUI restarts the backend). The browsed folder keeps .flea-work-ext-<pid>-<n>/out/<partial tree> forever; nothing in Flea ever sweeps it and its name is not offered for cleanup.
- **Fix:** Register archive/convert jobs so drain can cancel/kill them and remove their Work dirs before exit; on startup (or when listing a directory) sweep .flea-work-<tag>-<pid>-* whose pid is no longer alive, since the name already encodes the owner pid.

### S68. Into-itself guard is based on canonicalize, which cannot see a folder reachable through a bind mount

**Low**, PLAUSIBLE (votes: Low, Medium, Low). `src/backend/opsreq.rs:250`

- **What:** dest_real.starts_with(src_real) compares symlink-resolved strings. A directory bind-mounted at a second path (or the same btrfs subvolume mounted twice) is the same inode under two unrelated strings, so a drop of the folder onto its own subtree via the other path passes both the Drag.js string check and this one. rename(2) answers EXDEV across the two mounts, so move_any falls into copy_any, which reads the source while writing into its own descendant; copy_dir_entries iterates a live read_dir and the tree grows until the disk is full.
- **Scenario:** mount --bind ~/Music /srv/music (a media-server or container setup). In Flea, drag ~/Music from one tab onto /srv/music/incoming in another tab, or Ctrl-drag to copy. Neither guard fires; the copy recurses into its own output.
- **Fix:** Walk dest_real and each of its ancestors with symlink_metadata and refuse when any (dev, ino) equals the source's (dev, ino); that is the check the kernel applies for same-mount renames.

### S70. A script that cannot start is reported as nothing at all

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/Scripts.qml:55`

- **What:** Scripts.qml's runner Process only handles `onExited`; the codebase's own measurement (ui/ViewState.qml:389-395) says a Quickshell 0.3.1 Process that fails to start emits no `exited`, only `running` going false. A script whose exec fails (CRLF shebang -> ENOENT, interpreter missing, binary without shebang -> ENOEXEC, or a workingDirectory on a stale mount) therefore produces no status-centre message, and the operator assumes it ran.
- **Scenario:** Save ~/.config/flea/scripts/shred.sh with Windows line endings (`#!/bin/bash\r`), chmod +x, select files, Menu > Run script > shred. `find -perm -u+x` lists it, `runner.command = [script, ...paths]` is set, exec fails with ENOENT, no `exited` fires, `root.said` is never called; the same happens if paths[0]'s directory vanished between menu open and run (runner.workingDirectory invalid).
- **Fix:** Mirror ViewState's writer: add `onRunningChanged: if (!runner.running && runner.script.length && !exitedSeen) root.said(name + " could not be started", true)`, and check `runner.workingDirectory` exists before setting running.

### S71. Dropbox share link sits on wl-copy's argv, readable by every local user for as long as the clipboard holds it

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/ShareLink.qml:64`

- **What:** Copy Share Link hands the public URL to wl-copy as a command-line argument; wl-copy forks and keeps serving the selection in the background with that same cmdline, so `ps`/`/proc/<pid>/cmdline` shows the link to any other account on the machine (no hidepid on /proc here) until the clipboard is next replaced.
- **Scenario:** On a shared box, user A picks Copy Share Link on a private Dropbox file. User B runs `ps -eo args | grep wl-copy` minutes later and reads `wl-copy https://www.dropbox.com/scl/fi/...` — a public link needing no login. The Copy Path row (ui/Opener.qml:84) avoids this by piping through stdin (the path is only on `sh`'s argv for the pipeline's lifetime).
- **Fix:** Write the URL on stdin the way Opener.copyText does (`sh -c "printf '%s' \"$1\" | wl-copy" _ url`, or Quickshell's Process.write with stdinEnabled), so the daemonised wl-copy carries no secret on its argv.

### S72. LocalSend inbox is a predictably named 0700 directory in shared /tmp: another local user can pre-create the names and disable LocalSend, and a SIGKILLed backend leaves peer-sent files behind

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/localsend.rs:88`

- **What:** Inbox::new() builds `<temp_dir>/flea-localsend-<backend pid>-<n>` with a monotonically increasing n and refuses (correctly) any pre-existing name; but the pid is public and /tmp is world-writable, so a hostile local account can create those names first and every discovery/send then fails with 'LocalSend could not make its private folder'. Cleanup is Drop-only, so a killed backend (the known hung-mount quit path, OOM, `kill -9`) leaves the inbox and whatever a paired peer pushed into it in /tmp until reboot.
- **Scenario:** User B runs `for n in $(seq 0 200); do mkdir /tmp/flea-localsend-$(pgrep -f 'flea --backend')-$n; done`. User A's context menu now shows LocalSend errored on every open. Separately: a paired phone sends a photo while A's backend is later killed by the known orphaned-backend quit; the 0700 dir with the photo stays in /tmp (tmpfs) for the session.
- **Fix:** Put the inbox under $XDG_RUNTIME_DIR (already user-private tmpfs; mirror previewimage::private_dir's uid/mode check) with a random suffix from /dev/urandom like thumbwrite::exclusive_temp, and sweep `flea-localsend-*` directories of dead pids at backend start.

### S73. Copy Share Link is offered for directories, so one keypress publishes a whole folder tree (known share-link item is incomplete)

**Low**, CONFIRMED (votes: Low, Low, Info). `ui/js/Menu.js:174`

- **What:** The sharelink row is gated only on `rowInDropbox` and dropbox-cli being installed; it is not restricted to files. `dropbox-cli sharelink <folder>` creates a public folder link, so the already-known 'public with no confirmation' issue also covers entire folders, which the earlier round described as a per-file link.
- **Scenario:** Cursor on `~/Dropbox/Tax 2025/` (a folder), open the menu, pick Copy Share Link: the whole folder becomes reachable by anyone with the URL, and the status bar just says 'Share link copied to the clipboard.'
- **Fix:** Either hide/disable the row for directories, or show a confirmation naming the item and stating that the link is public (the same confirmation the known item asks for), with stronger wording for a folder.

### S74. After a restore, `z` flip-flops between two piles forever and the recorded shelf move can never be undone

**Low**, CONFIRMED (votes: Low, Low, Low). `src/shelfundo.rs:161`

- **What:** `undo()` picks whichever of the move journal and the newest kept pile is newer, but `summon::restore_at` (src/summon.rs:190) re-keeps the pile it displaced with a fresh `now_ms()` stamp, so once a restore has replaced a non-empty pile the newest pile is always newer than the move and every subsequent `z` just swaps the two piles; the move the card promised with "z undoes" is unreachable.
- **Scenario:** Move one.txt from the shelf to Work ("Moved 1 item to Work · z undoes"). shift-x clears the shelf, drop another file on the rail, press z: the old pile comes back and the one-file pile is kept with a new timestamp. Press z again: that pile comes back instead of the move. z, z, z... always "Put N items back"; one.txt stays in Work and undo.json is never consumed.
- **Fix:** Have `restore_at` keep the displaced pile under the timestamp it was cleared/created with (or under the restored pile's original `at`), or make `undo` treat a pile whose `at` is newer than the last undo press as a restore artefact; alternatively record an explicit "last undoable action" marker instead of comparing two clocks.

### S75. shift-x clear and a pile restore both drop the pinned rows the README says are always there

**Low**, CONFIRMED (votes: Low, Medium, Low). `src/shelf.rs:194`

- **What:** `Shelf::clear` and `Shelf::put` swap the entire item list, pinned entries included, so a clear moves every pin into the five-deep pile history and a restore (right click on the bar mark, menu row, or `z`) replaces the current pins with whatever the old pile held; after five more clears the pins are gone for good.
- **Scenario:** Pin ~/Work/inbox on the shelf (README: "A pinned row is always there"). Press shift-x six times over a week (each after dropping something): the pin is in a pile that has fallen off the five-entry history and is silently lost. Or: right-click the bar mark once and the current pins vanish from the card, replaced by the older pile's rows.
- **Fix:** In `clear` take only non-pinned entries, and in `put` merge the restored pile with the current pinned entries (dedupe by path) so pins survive both directions.

### S77. A shelf zip staged in a shared common ancestor such as /tmp is world-readable for the whole relocate copy

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/shelfzip.rs:78`

- **What:** The archive is built inside a work directory created with default modes in the pile's common ancestor (`Work::new`, archivework.rs:35, plain `create_dir` → 0755 under umask 022), renamed to `<ancestor>/.flea-shelf-<pid>-<date>.zip` with bsdtar's default 0644, and only then moved to the 0700 state directory; when the ancestor is on another filesystem (Omarchy's /tmp is tmpfs) `relocate` falls back to a full `std::fs::copy` (shelfzip.rs:131), so the complete archive sits world-readable in /tmp for the duration of that copy, and stays there hidden if the final `remove_file` fails (only said on stderr the card never shows).
- **Scenario:** On a shared workstation the user extracts a confidential bundle under /tmp/review, drops /tmp/review/a.pdf and /tmp/review/b.xlsx on the shelf and presses a. For the seconds-to-minutes the multi-hundred-MB zip is being copied into ~/.local/state, `/tmp/.flea-shelf-<pid>-2026-09-23.zip` is readable by every other local account.
- **Fix:** Create the work directory 0700 (`DirBuilder::mode(0o700)`), chmod the staged archive 0600 before the rename out of the work dir, and skip the intermediate rename to the ancestor by copying straight from the work dir to the reserved name.

### S78. Shelf zip inherits the thumbnailer's 30 CPU-second jail, so a pile over roughly 1.3–2 GB always fails with an empty error

**Low**, PLAUSIBLE (votes: Low, Medium, Low). `src/backend/archiveops.rs:30`

- **What:** `compress` runs bsdtar through `run_boxed` → `sandbox::wrap`, which prepends `prlimit --cpu=30` (sandbox.rs:8). Deflate is CPU-bound and single-threaded; measured on this box bsdtar spends ~14.7 CPU-s per GB of incompressible data and ~23.6 CPU-s per GB of text, so a pile beyond about 2 GB (media) or 1.3 GB (documents) is killed by SIGXCPU. The work dir cleans up, so nothing is lost, but the card only says "That pile could not be zipped." and the operator has no way to know why or to zip the pile at all.
- **Scenario:** Drop a folder of RAW photos (3 GB) on the shelf and press a. After ~30 s of CPU the jail kills bsdtar; the card reports failure with no reason; every retry fails identically.
- **Fix:** Give archive creation its own limit (much larger CPU budget or none, keeping the address-space and namespace jail), or scale the CPU budget from the summed size of `names`, and surface the last stderr line in the card.

### S79. One pile entry on a stalled mount permanently wedges the card's single-flight processes: no sizes, no thumbnails, no drag-out, no action until the shell restarts

**Low**, PLAUSIBLE (votes: Low, Low, Low). `shelf/ShelfService.qml:88`

- **What:** Every helper the card spawns is a single `Process` gated by `.running` with no timeout: `measure` (askSize, line 265) stats rows in drawn order, `thumber` (line 235) stats every wanted path in one call, `mint` (mintDrag, line 88) stats every carried path in `flea shelf drag-begin`, and `action` (ShelfActions.qml:29/45/59) refuses while a previous move/copy/zip is alive. A path on a mount that has stopped answering (gvfs/SMB/NFS/autofs) puts that child into an uninterruptible stat, and from then on the corresponding slot is dead for the life of the bar; `flea shelf cancel` cannot reach an engine stuck in stat.
- **Scenario:** Drop a file from a mounted SMB share on the rail (the rail takes URIs from any application), then the laptop leaves the office. Open the card: the size ask for that row hangs `measure`, so no row below ever gets a size; the thumb ask hangs `thumber`. Press m on the pile: `flea shelf move` stalls on `symlink_metadata` of that entry, esc writes the cancel marker but the engine never returns to check it, and every later action says "The shelf is still busy with the last one" until `omarchy restart shell`. Pressing x on the row removes the reference but does not unstick the already-running children.
- **Fix:** Give each helper Process a watchdog (Quickshell `Timer` that sets `running = false` after N seconds and marks the path as unanswered), order size/thumb asks so a stalled path is skipped after a timeout, and let esc kill the action process when the cancel marker is not acknowledged within a few seconds.

### S83. movesource snapshot walk before a cross-device move ignores the cancel flag and reports no progress

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/movesource.rs:40`

- **What:** move_any now stats the entire source tree (snapshot -> walk, symlink_metadata + read_dir per entry) before the first byte is copied (copyfile.rs:294). walk takes no cancel and publishes nothing, while the parallel spawn_total sweep does honour the flag via dirsize::walk_while. On GVFS FUSE mounts every getattr is a round trip to the backend (no readdirplus), so the pre-walk of a large tree from an SMB/SFTP/MTP source is a long uncancellable phase during which the card shows 0 bytes and a cancel only lands when the walk ends. remove_copied then re-stats every entry a third time.
- **Scenario:** Cut a folder of 30,000 photos from an SMB share mounted through gvfs and paste it into ~/Pictures. Press cancel a few seconds in: nothing happens until the snapshot walk (tens of thousands of query_info round trips) completes, then the copy starts and the cancel is finally noticed on the first chunk. On a phone over MTP the same walk is much slower.
- **Fix:** Thread the transfer's cancel flag into snapshot (walk_while-style closure checked per entry) and return the cancel error so one_item reports `cancelled` and skips; optionally publish the walk's entry count through on_bytes so the card is not blank during it.

### S84. EXDEV move whose removal phase errors leaves an unjournaled complete copy and a partly removed source (extends known kept-entries finding)

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/copyfile.rs:296`

- **What:** The known finding covers the Ok(kept) arm. The same shape is reached through the Err arm: remove_copied removes files in read_dir order and propagates the first I/O error (movesource.rs:61,73-74,85 use `?`). move_any returns that error after copy_any already succeeded, one_item's Err arm only journals p.partial, which is None after a complete copy (opsreq.rs:325-330), so the destination tree is complete and unjournaled, the source is hollowed up to the failing entry, and the transfer card shows the raw io error (ENOENT/EIO), not the kept sentence.
- **Scenario:** Move a folder off a camera card mounted at /run/media/u/CARD to ~/Photos. While the copy runs the camera app (or a sync client) deletes one already-copied file inside the folder, or the card answers EIO on one unlink. remove_copied unlinks the earlier siblings, hits the missing/erroring entry, returns Err; the item is reported failed, undo has nothing for it, and the user is left with the full copy under ~/Photos and a half-emptied folder on the card.
- **Fix:** In remove_at, treat a child that no longer stats (ENOENT) as kept rather than an error, collect per-entry removal errors and keep walking, and in move_any always return the kept/error together with a journaled Copied step for dst (steps.push(undo::copied(src, dst, ...))) so the complete copy is undoable and the message names both halves.

### S85. FAT/exFAT inode numbers are not stable, so the new snapshot guard can spuriously refuse to finish a move from a stick

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/movesource.rs:62`

- **What:** Seen and the directory check compare (dev, ino) taken before the copy against a fresh symlink_metadata after it (movesource.rs:62,69). vfat and exfat assign st_ino with iunique() when an inode is instantiated and hand out a new number if the inode was evicted from the cache and re-read; there is no fd holding a source file open between its copy and its removal, and a multi-GB copy is exactly what pressures the inode cache. A file or directory whose inode was evicted mid-move compares as changed, is kept, its parent directory is not removed, and the move ends with 'N items appeared or changed there while it was copied, so the original was kept beside the copy' over an untouched source. Fail-safe (nothing is lost), but it leaves a hollowed remnant on the stick plus a full copy, and the same identity model is what undo's ItemIdentity.same_item relies on for Moved/Copied steps on those filesystems.
- **Scenario:** Move a 40 GB folder of photos from a FAT32 stick to the internal disk on a box with a few GB of RAM. Page cache pressure evicts unreferenced FAT inodes during the copy; remove_copied stats them again, gets new inode numbers, keeps those files and every parent directory, and the operator is told items changed while nothing did. The stick now holds a scattered subset of the originals beside the complete copy at the destination.
- **Fix:** When the source's statfs magic is vfat (0x4D44) or exfat (0x2011BAB0), compare kind + len + mtime instead of ino for files and skip the (dev, ino) identity for directories, or hold each copied file's source fd until its removal so the inode cannot be evicted between the two stats.

### S86. A gio unmount or eject that never exits parks the rail's release actions and later presses are dropped silently

**Low**, PLAUSIBLE (votes: Low, Low). `ui/DeviceMounts.qml:169`

- **What:** The listing, mount and eject-verdict legs are all bounded by timers, but the three release processes are not: DeviceMounts ejectProcess/unmountProcess, PhoneMounts unmountProcess and NetworkMounts unmountProcess have no timeout. While one runs, DeviceMounts.unmount (:169), PhoneMounts.unmount (:46) and NetworkMounts.unmount (:439) `return` with no message, and eject answers 'Still ejecting; wait for its result' forever. gvfs mount/unmount D-Bus calls use unbounded timeouts, and udisks' unmount of a stick whose writeback has stalled blocks in umount(2) for as long as the kernel retries.
- **Scenario:** A USB stick starts throwing I/O errors during a copy; the operator picks Eject. udisks' umount waits out the kernel's retries for minutes; meanwhile every further Eject press says 'Still ejecting', and Unmount on any other volume or share does nothing and says nothing, until the shell is restarted. The same for Unmount on a phone whose MTP session has hung.
- **Fix:** Give each release Process a bounded wait (the 15 s the mount leg already uses), on expiry stop the process, clear _ejectDevice/_unmountLabel and post 'X is still unmounting; do not unplug it yet' as an error, and have unmount()/eject() answer a message instead of a bare return when a release is already in flight.

### S90. TUI filter with no matches leaves the cursor on a hidden row; Delete trashes a file the list says is not there

**Low**, CONFIRMED (votes: Low, Low, Low). `src/tui/model.rs:466`

- **What:** apply_filter() moves the cursor to `shown.first()` or, when nothing matches, to `self.top`. That row is loaded, so every action guard passes, but it is filtered out of view. Delete/dd, Enter, cut and duplicate all act on it.
- **Scenario:** In a folder with a.txt first, type a filter `zzz` and press Enter. The list reads 'Nothing matches zzz'. Pressing Delete sends trash for rows [top] (actions.rs:405-408, model.rs:469-475), and a.txt, which the user never saw, goes to the Trash. Enter opens it through gio. Rename opens an inline editor on a row that is not drawn. move_by returns early when shown is empty (model.rs:510), so the cursor cannot even be moved off it.
- **Fix:** When shown is empty, clear the cursor instead (for example a sentinel that fails rows.contains_key), or make act() refuse cursor-based actions when the filter is non-empty and the cursor is not in shown().

### S91. TUI cut is never spent by its paste; a later paste moves whatever now lives at the old paths

**Low**, CONFIRMED (votes: Low, Low, Low). `src/tui/actions.rs:383`

- **What:** `m.clipboard` and `m.cut` are set once (model.rs:760-762, actions.rs:379) and never cleared. After a cut and paste, every later paste sends another `transfer op=move` of the same source paths. The GUI spends a cut on paste (ui/js/Ops.js:219-221).
- **Scenario:** Cut ~/Downloads/statement.pdf in the TUI and paste it into ~/Documents. The browser later saves a new statement.pdf in ~/Downloads under the same, now free, name. Days later in ~/Projects the user presses p p, perhaps expecting an earlier copy, and the new statement.pdf is moved out of Downloads with no prompt. The 'N items cut' message expired long ago, so nothing shows that a cut is still armed.
- **Fix:** On the transferdone of a move that came from a cut, clear m.clipboard and set m.cut=false, as Ops.js does. Show the armed state in the footer.

### S92. TUI permanent-delete dialog moves focus to Delete when the mouse passes over it; the next Enter or Space deletes permanently

**Low**, CONFIRMED (votes: Low, Low, Low). `src/tui/actions.rs:613`

- **What:** deletion_key() takes pointer motion events with no button pressed (any-motion mode 1003 is on while the dialog is open) and sets `deletion.destructive = true` when the pointer passes over the Delete button. Enter and Space then run the destructive action. Focus does not go back to Cancel when the pointer leaves.
- **Scenario:** Shift+Delete on a folder opens the review with Cancel focused. While reading the item list, the user nudges the trackpad so the pointer crosses the red Delete button, and focus moves there. They press Enter expecting the safe default, and the permanent delete runs.
- **Fix:** Only a click (button 0, no motion) should change or activate the destructive choice; use hover for highlight at most, never for keyboard focus.

### S95. Open-mode symlink rows show only the first hop, but the app receives the fully resolved final target

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/meta.rs:44`

- **What:** A picker row's link mark is read_link() of the entry, one hop only (meta.rs:44-50 → rows.rs:61 'l' → Row.qml:48 ' -> target'). The picker's mark follows the whole chain (picker.rs:116 Held::open(path, true)) and replies with the link's own path (Picker.js:166). xdg-desktop-portal's register_document opens that path following symlinks, so the document portal exports the final target. The on-screen name and arrow can describe a harmless-looking file while the exported object is anything the chain reaches.
- **Scenario:** A sandboxed app steers current_folder to its own ~/.var/app/<id>/data/exports, which it can write. There it plants 'backup.json -> backup-latest.json' and 'backup-latest.json -> /home/u/.config/<password-manager>/vault' or '~/.ssh/id_ed25519'. The picker row reads 'backup.json -> backup-latest.json'. The user marks it and presses Open, and the app receives read access to the private file. Relative targets that would reveal the destination can be hidden behind the second hop.
- **Fix:** When a mark is a symlink, have the backend return the canonical resolved path (for example readlink of /proc/self/fd/<held target fd>). Show it in the footer ('backup.json resolves to ~/.ssh/id_ed25519'), and/or answer with the canonical path so the displayed name, the reviewed object and the exported object agree.

### S96. The Recent location shows only basenames, so two recent files with the same name look identical and nothing shows which directory the handed-over file is in

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/PickerList.qml:48`

- **What:** In Recent, each row is drawn as Match.base(row.n) (PickerList.qml:48-50), the path strip says only 'Recent' (PickerChrome.qml:243), and the footer shows only a count and size. The URI answered is rowPath = '/' + full path (Picker.js:189-191). No surface in the open-mode picker shows the directory of a Recent entry before it is sent.
- **Scenario:** recently-used.xbel holds both ~/Documents/taxes/report.pdf and ~/Downloads/report.pdf. An app asks for a file, and the user opens Recent and marks the 'report.pdf' they think is the downloaded one. The app receives the tax document instead. Any host application that writes the shared xbel can also plant an entry with a familiar basename pointing into a private directory.
- **Fix:** In Recent, draw the parent directory (tilde form) as a second column or muted suffix, as the search view's location column already does (Match.location). Or show the cursor row's full path in the path strip while in Recent.

### S97. In file mode, Enter on an unmarked file sends the files marked earlier, possibly from other folders and no longer on screen, not the row under the cursor

**Low**, CONFIRMED (votes: Low, Low, Low). `ui/picker.qml:206`

- **What:** activate() on a non-directory row calls accept() (picker.qml:198-207), which submits win.marks (239-243) whatever the cursor row is. Marks survive navigation by design (Picker.js:136-145), and marks outside the current folder are invisible: the footer shows only 'N selected · size'. The hint 'Space select · Enter open/send' does not say that Enter sends earlier marks rather than this file.
- **Scenario:** The user marks ~/private/id_backup.txt by mistake (Space), navigates to ~/Downloads, puts the cursor on photo.jpg and presses Enter expecting to send photo.jpg. The app receives id_backup.txt. In multiple mode it receives every file marked anywhere during the session. This is the file-mode sibling of the known folder-mode Enter item, which covered only the implicit current-directory answer.
- **Fix:** When the cursor row is a markable, unmarked file and marks exist elsewhere, refuse with 'Enter sends the N marked items; press Space here or clear marks'. Or make Enter on an unmarked file toggle it rather than send. Also list the marked names, or at least their folders, in the footer before the answer leaves.

### S98. The Trash listing is all-or-nothing on gio's raw output, so one trashed path containing a newline or non-UTF-8 bytes disables the Trash view, Restore, Empty Trash and the 30-day sweep, with no way to fix it inside Flea

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/trashbrowse.rs:101`

- **What:** This makes the known 'one unreadable trash entry makes the whole trash view and the sweep fail closed' item more complete, with triggers that are easy to hit and that fail at the listing level rather than per item. `gio trash --list` prints each item's trash::orig-path as raw bytes (trash.rs's new comment records the reproduction: a newline and tab in a trashed name forges a line). trashbrowse's `gio()` rejects the whole output if any byte sequence is not valid UTF-8 ('GIO returned invalid text.', line 101). `parse_list` rejects the whole listing if any line has no tab or does not start with trash:/// (lines 108-116). A newline inside an original path always produces such a line. Every Trash operation calls list() first: list/window, select, prepare, valid, and restore when restoring all. So one such item leaves the view showing an error. Restore, Delete permanently and Empty Trash become unreachable, and the daily sweep fails every day without saying so. Flea cannot remove the offending item itself, because deleting it needs a listing. Only `gio trash --empty` or a manual rm in a terminal clears it. The data is safe, since everything fails closed, but this is a denial of service against Trash management that other people can trigger.
- **Scenario:** 1) A tarball, zip or git checkout creates a file or folder whose name contains a newline, for example 'readme\n.txt'. The user trashes it: Flea's own trash.rs passes it to gio intact because the name is valid UTF-8. 2) From then on, opening Trash shows 'GIO returned an unreadable Trash listing.', and Empty Trash, Restore and the auto-empty sweep all stop working. Variant 1: a Latin-1 or CP437 name trashed by Nautilus, trash-put or KDE. Variant 2: a USB stick whose .Trash-1000 was filled on another system. Mounting that stick is enough to kill the Trash view, because gio lists the trash of every mounted volume.
- **Fix:** Parse the listing per item and fail only that item. Read stdout as bytes, split on '\n' only where the next line starts with 'trash:///' (gio escapes the URI, so it never contains a raw newline), or better, take orig-path from `gio info` per URI, where values are escaped. Show items whose names cannot be decoded with an 'unreadable name' row that can still be selected by URI. Let Empty Trash skip items it cannot review, and report them as 'not deleted', instead of refusing the whole Trash.

### S102. Listing arena u32 offsets wrap silently in release, so a search over 4 GiB of match paths names the wrong file for a row

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/listing.rs:34`

- **What:** Listing::push computes off = names.len() as u32 and len = names.len() as u32 - off. name() slices names[off..off+len]. The release profile has no overflow-checks, so after 4 GiB of names, off wraps to a small value and name(i) returns bytes from the start of the arena, which is another row's text. It does not panic. AGENTS.md:277 notes that 'a later search phase reusing this arena could' reach the limit. search.rs now pushes full relative paths into this arena with no match cap.
- **Scenario:** A deep search with a one-letter query over a tree with tens of millions of files, such as a backup or snapshot tree, accumulates more than 4 GiB of relative paths. Rows past the wrap resolve to substrings of earlier rows' names, so trash or paths on such a row acts on a path the user never saw. This needs more than 4 GiB of RAM for names first, so it is unlikely on a desktop.
- **Fix:** Cap the search's match count and bytes well below u32::MAX and end the walk as partial. Alternatively, have push() refuse, returning false, once names.len() would exceed u32::MAX, instead of wrapping.

### S104. The 'into itself' check misses case-insensitive filesystems (FAT, exFAT), so copying a folder into a different-case spelling of its own subfolder fills the stick and ends in the crash above

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/opsreq.rs:250`

- **What:** dest_real.starts_with(src_real) compares canonicalize() output byte for byte. realpath does not fold case, so on vfat or exFAT '/run/media/u/STICK/photos/sub' and '/run/media/u/STICK/Photos' are the same directory but neither is a prefix of the other. ui/js/Drag.js canDropInto (line 210) is lexical as well. This is a second alias class next to the known bind-mount gap.
- **Scenario:** The destination path reaches Flea with different case from the source. Examples: a foreign drag or FileManager1/CLI path spelled '/…/STICK/photos' dropped onto Flea's listing of '/…/STICK/Photos/sub', or the reverse. copy_dir_at creates Photos/sub/Photos first, then walks Photos/sub, which already holds the new copy. Each level copies all the files again until ENOSPC fills the stick, or until it reaches the ~1,175-level stack overflow above and the backend aborts.
- **Fix:** Compare identities rather than strings: walk dest's ancestors with metadata() and refuse if any (dev, ino) equals the source directory's (dev, ino). This also closes the known bind-mount case.

### S105. Compress to .7z treats * and ? in selected names as wildcards and follows symlinks, so the archive can pick up unselected sibling files

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/archive.rs:69`

- **What:** The 7z branch passes each selected item to '7z a -bd <dest>' as an absolute path with no -spd (disable wildcard matching) and no -snl (store links as links). 7-Zip matches * and ? in operands against the directory. The jail ro-binds the whole parent directory (archiveops.rs:30 run_boxed(..., parent, ...)), so every sibling is visible to the match. Upstream 7-Zip on Linux also stores what a symlink points to by default, not the link itself.
- **Scenario:** A downloaded file is named 'download?id=3' or 'report*.pdf' (legal on Linux and common from wget or browsers). Compressing it to .7z also packs every sibling the pattern matches, for example 'report-salaries.pdf', into an archive the user is about to send. Separately, a selected folder holding a symlink to an unselected sibling folder in the same parent gets that sibling's contents packed in. The bsdtar formats end options with '--' and do not glob, so this is 7z only.
- **Fix:** Add '-spd' and '-snl' (and '--' before operands) to the 7z compress argv, and add a test with a sibling that matches '?'.

### S106. The stale-row-index race also reaches the 'paths' request: cut/copy to the clipboard and Compress can take the neighbouring file

**Low**, PLAUSIBLE (votes: Low, Low, Low). `ui/js/Ops.js:191`

- **What:** Ops.clip and Ops.compress send row indices ({c:'paths',rows}) and the backend resolves them against whatever listing it holds at that moment. That is the same mechanism as the known trash and drag-and-drop races, but those findings only name Delete, drops and Move to Dropbox.
- **Scenario:** During a background re-list, the user presses x on file A. The backend resolves the index against the new listing and answers with neighbour B. The status bar says 'Cut 1 item', and the later paste moves B, a file the user never marked. Compress in the same window archives B, and a user who then shares the archive leaks it.
- **Fix:** Send the names the client drew alongside the indices, or a listing generation, and have the backend refuse on any mismatch. Use one fix for trash, paths and rows-based transfers.

### S109. One refused open rule makes the launch-time settle erase the whole File types table from disk, with no backup

**Low**, CONFIRMED (votes: Low, Low, Low). `src/uistore.rs:93`

- **What:** settle() runs at every GUI and TUI launch and rewrites ui.json whenever any key fails validation. openrules::fits() is all-or-nothing, and merge() falls back to the shipped default (uistate.rs:104). So one rule this build does not accept, such as an uppercase ending from a hand edit, an app id without .desktop, or a shape a newer build writes, is enough: the next launch writes `"rules": []` over the user's whole table. The same applies to the other validated keys: a column or view word from a newer build is replaced by the default. No backup is kept; ui.json.broken is written only for unparseable JSON.
- **Scenario:** A user hand-adds `{"ends":".NRRD","app":"slicer.desktop"}` next to their existing rules, or alternates between the packaged /usr/bin/flea and a newer dev build in ~/code/flea/target/release (both exist on this box). The next launch of either front end silently deletes every Settings > File types rule from disk.
- **Fix:** On settle, keep the refused values rather than replacing them: skip the rewrite, or write the original to a ui.json.refused-<date> backup before rewriting. For list keys like open.rules, drop only the invalid entries (per-item filtering) instead of the whole table.

### S110. A symlinked ui.json stops the TUI from starting and makes every GUI navigation show a save error

**Low**, CONFIRMED (votes: Low, Low, Low). `src/tui/mod.rs:33`

- **What:** settle() sends anything that is not a regular file to update(), and update() refuses a symlink target. For a symlinked ui.json, settle therefore fails on every launch, even when the content is valid. The TUI propagates that error with `?` and exits. The GUI starts, but every setting change fails, and so does the automatic lastPath write on each folder change, so every navigation puts a red 'state' error in the status bar. By contrast, replace_file (bindings.lua, mimeapps.list) deliberately writes through dotfile symlinks.
- **Scenario:** A user keeps ~/.local/state/flea/ui.json in a dotfiles repo, symlinked by stow or chezmoi, which is plausible because Flea keeps real settings (favourites, open rules, theme) in the state dir. `flea --tui` exits immediately with 'ui.json is a symbolic link, so the state file was not written'. In the GUI, every setting is lost at exit and every folder change shows an error.
- **Fix:** In settle, treat a symlink whose target is a regular file the same as a regular file for the no-op check (left_as_it_is), and have the TUI log a settle failure instead of aborting, as main.rs:330-333 already does for the GUI. Either resolve the link and rename in the target's directory, as userfile::replace_file does, or report once that settings are read-only for this session instead of once per navigation.

### S112. Space preview renders and the shared thumbnail cache outlive permanent delete; nothing ever removes a cache entry

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/thumbs.rs:165`

- **What:** Flea writes a 256 px PNG into ~/.cache/thumbnails/large/<md5(uri)>.png for every image, video and PDF the cursor passes, plus fail/flea markers for files that fail. Each carries the full original path in Thumb::URI. No code path removes or invalidates these entries on trash, permanent delete, rename or move: the only cache users are lookup, publish and the pid temp sweep. The new Space renders (up to 4096 px, lossless) also stay in $XDG_RUNTIME_DIR/flea-preview after the source is permanently deleted. Omarchy runs no gnome-settings-daemon housekeeping, so on this box the cache is never aged out.
- **Scenario:** The user views a folder of private photos, then selects them and uses Delete permanently, and the confirmation says they cannot be recovered. Afterwards every 256 px thumbnail and its full path stay in ~/.cache/thumbnails/large, and the Space renders stay in the runtime tmpfs until logout. Any other same-user process can read them, including Flatpak apps granted home or xdg-cache access and backup tools that include ~/.cache. The known item 'thumbnails made for remote, removable and encrypted volumes' is about producing thumbnails; this one is about removing them on delete.
- **Fix:** After a successful permanent delete, and optionally after trash, move or rename, remove large/<md5(uri)>.png and fail/flea/<md5(uri)>.png for each removed path (for directories, each descendant already walked by the review manifest). Also drop any flea-preview render whose key names that URI. For a tree it is enough to remove the entries Flea itself could have written.

### S113. Compress writes its archive 0644 inside a 0755 '.flea-work' dir, whatever the modes of the private selection

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/archiveops.rs:24`

- **What:** compress() stages in Work::new(parent) (archivework.rs:35, plain create_dir, so 0777 & ~umask = 0755) and lets the jailed bsdtar or 7z create the archive with the default 0644. It then renames the archive into place with no chmod. The module header calls this 'a private directory', but it is not private. The archive of a 0700 folder or a 0600 file is readable by every account that can reach the parent, both while it is being written and afterwards. The shelf zip already opens its output with OWNER_ONLY_FILE (shelfzip.rs:162), so the pane's Compress is the inconsistent one. Convert output (ImageMagick) has the same 0644 default.
- **Scenario:** In a group-shared project directory, or on a multi-user box in /srv/share, the user has a 0700 folder 'hr-reviews' and presses Compress. hr-reviews.zip appears 0644, so every group member (and others, if the parent allows) can read the contents the folder's mode kept from them. They can also read the growing .flea-work-arc-*/archive.zip while it is written.
- **Fix:** Create the work directory with DirBuilder mode 0700. After the tool exits, chmod the staged archive to the intersection of the selected items' owner/group/other bits (at minimum 0600 when any selected item is not world-readable) before rename_noreplace. Apply the same rule to convert_one's output.

### S114. 'Passwords never on disk' is incomplete: a GUI or backend crash writes their full memory to disk through systemd-coredump

**Low**, PLAUSIBLE (votes: Low, Info). `ui/NetworkMounts.qml:17`

- **What:** Network passwords are cached for the session in the qs process's JS heap (_passwords, remember() at 262-266). The backend holds listing and preview data. Neither process opts out of core dumps: no PR_SET_DUMPABLE, MADV_DONTDUMP or RLIMIT_CORE=0 anywhere in src, ui or tools, and gui.rs:48 execs qs unchanged. On this box core_pattern pipes to systemd-coredump with default Storage=external, so any abort stores a full, persistent (zstd) image of that memory under /var/lib/systemd/coredump. Earlier rounds found reproducible aborts in both processes (e.g. the PdfViewer teardown abort, and a backend that aborts today).
- **Scenario:** The user mounts an SMB share with a typed password, which is cached per the known item. Later the known PDF-teardown abort takes down qs. systemd-coredump saves the qs heap, including the cleartext password and any previewed text, to /var/lib/systemd/coredump/core.qs.*.zst. It stays there until tmpfiles ages it out (weeks), survives reboot, and is readable by the user and root and by tools like coredumpctl, crash reporters or this repo's diagnose-crash workflow.
- **Fix:** In gui.rs, before exec'ing qs, and in the backend at startup, call setrlimit(RLIMIT_CORE, 0) (systemd-coredump honours it and stores no core) or prctl(PR_SET_DUMPABLE, 0). Alternatively, keep credentials out of the qs heap: hand them straight to flea-gio-auth and let the keyring hold them.

### S117. The line-based shelf CLI replies (thumb, captures) can be injected by names containing a newline: the bar decodes a file of the attacker's choosing in-process, and a capture row can name a relative path

**Low**, CONFIRMED (votes: Low, Low, Low). `src/shelfthumb.rs:27`

- **What:** `flea shelf thumb` prints "<cache>\t<path>" per path, and Model.thumbsFrom splits on \n and the first tab, letting later lines win. A pile path containing "\n<ABS>\t<OTHER-ROW-PATH>" (built from directory components, since the separator after the newline supplies the leading '/') maps another row to <ABS>. ShelfRow then loads Image{source:"file://"+<ABS>} inside the Omarchy shell process, skipping the jailed thumbnailer entirely. `flea shelf captures` prints "<ms> <path>", so a capture file named "screenshot-x\n9999999999999 .netrc\n.png" adds a newest capture row whose path is the relative ".netrc". shelfops::transfer and send, unlike zip, do not refuse relative paths. Choosing that row and pressing t, c or m (or clicking it, which runs add) resolves it against the shell's working directory.
- **Scenario:** An extracted archive or clone contains the directory chain a\n/<known abs dir>/evil.svg\t/<known abs dir>/key.txt. The user shelves the deep file together with key.txt. The card's thumb run returns an injected line, and key.txt's row is decoded from evil.svg (or any Qt/KImageFormats-parsed file) unjailed in quickshell. For captures: a crafted name in ~/Pictures shows a ".netrc" capture row at the top of the tray.
- **Fix:** Use NUL-separated or JSON output for thumb/captures/places, or refuse to print (and skip) any path containing \n, \r or \t. In the bar, accept a thumbnail only if it is under the thumbnail cache root. Refuse non-absolute paths in shelf move/copy/send/add, as zip already does.

### S118. The Choose-a-folder reply is trimmed, so a chosen folder whose name ends in whitespace (including NBSP or U+3000) is swapped for a different folder

**Low**, CONFIRMED (votes: Low, Low, Low). `shelf/ShelfActions.qml:218`

- **What:** shelfplaces::choose prints the chosen directory exactly. ShelfActions.chooser does String(text).trim() on it before runChosen(dest). JS trim strips trailing spaces, tabs, newlines, NBSP, U+3000 and similar. Run.lines, used for places, deliberately strips only \r, because "a path may legally begin or end with a space". This one reader does not. It is the same bug class as the earlier "select-file host path" finding: a destination that is not what was chosen.
- **Scenario:** The user picks "/home/u/Invoices " (a name pasted from a web page) in the picker. The shelf moves the pile into "/home/u/Invoices" if that folder exists, a different folder with private contents, or otherwise fails with a no-such-file error. Either way the wrong path is written to dests.json and offered as recent destination 1 from then on.
- **Fix:** Strip only the single trailing "\n" println adds, the way Run.lines does, or have choose print JSON.

### S119. The plugin refresh writes through a symlinked plugin directory or file, so it overwrites and prunes files outside ~/.config (extends the known refresh finding)

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/shelfplugin.rs:55`

- **What:** install() calls create_dir_all(dest) then std::fs::copy into dest/<name> for every packaged file. std::fs::copy opens the destination with O_TRUNC and follows symlinks. install() then remove_file()s every dest entry the package does not ship. If ~/.config/omarchy/plugins/io.github.thisisgm.flea-shelf is a symlink to a working tree (the usual plugin-development setup; the manifest's fleaCommand setting exists for development trees), or any single file in it is a symlink, then every GUI launch whose packaged version differs from the tree's overwrites the tree's QML with /usr/share/flea/shelf and deletes the tree's new files. The known finding covers edits under ~/.config only; this reaches whatever the link points at.
- **Scenario:** A developer runs `ln -s ~/code/flea/shelf ~/.config/omarchy/plugins/io.github.thisisgm.flea-shelf` and bumps the manifest version. Launching the installed /usr/bin/flea runs refresh(): the versions differ, so uncommitted Panel.qml and friends in ~/code/flea/shelf are overwritten with the packaged copies, and any new, not-yet-packaged file there is unlinked.
- **Fix:** Refuse (and report) when dest or any entry in it is a symlink or not a regular file. Copy to a temp name with create_new and rename it into place, and never delete anything the installer did not itself write (keep a manifest of the files it installed).

### S123. Undo of any copy onto a GVFS share or phone now always fails and spends the journal entry; the truncated partial of a failed copy can no longer be removed with z

**Low**, CONFIRMED (votes: Low, Low, Low). `src/backend/undo.rs:247`

- **What:** discard() refuses whenever gio trash fails. GLib refuses to trash on system-internal mounts, and fuse.gvfsd-fuse is on that list: every SMB/SFTP/WebDAV/FTP/AFC/MTP path. So undo of a Copied or Created step whose target is on a GVFS mount now always errors. Journal::undo has already popped the entry, so the whole operation is spent at the first refusal and no redo is recorded. Before the diff, z removed the copy, including the truncated partial that a failed copy journals.
- **Scenario:** A copy of 40 photos to an Android phone over MTP fails halfway when the phone locks. The known truncated file sits under its real name, and a retry refuses it as "already exists". The journal holds the partial so z can take it back. Now z answers "it could not be moved to the Trash, so undo left it in place", the entry is gone, and the partial stays with no route back through undo. The same happens for any mistaken copy into a NAS share over GVFS.
- **Fix:** Before discard, check whether the target sits on a mount gio cannot trash into (the same predicate as Mounts.trashable, extended to all of /run/user/UID/gvfs). For a step this operation created and whose identity still matches, keep the old exact removal for a partial the operation itself left, or say that plainly. In either case, push a failed entry back onto the journal instead of dropping it.

### S124. Trash on a stalled mount holds the operation slot forever with no timeout and no cancel

**Low**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/opsreq.rs:337`

- **What:** start_trash claims the single operation slot, then run_trash calls trash_checked. That lstat's every path and runs three gio children with Command::output() and no timeout. The cancel flag from claim_transfer is never passed in, and no transferstarted line is sent, so the UI cannot offer a cancel. If a path is on a mount that stops answering (NFS hard, sshfs, a GVFS backend waiting on a dead server), the Trashed message never arrives, ops.live is never finished(), and every later transfer, trash, duplicate, undo, redo and new file is refused with "an operation is already running" until the backend is restarted.
- **Scenario:** The user presses d on a file in an sshfs or NFS folder whose server has just gone away. The trash never returns. From then on, a copy between two local folders, a z, or a d anywhere answers "an operation is already running", with no visible operation and no way to cancel it short of quitting. Quitting leaves the orphaned gio or backend behind, as in the known run.rs finding.
- **Fix:** Run the gio children under child.rs run_with_timeout, kill them on cancel, and emit a started line so the status bar can cancel. When the timeout fires, release the slot and report the paths as failed, not trashed.

### S7. TUI event loop blocks synchronously on `gio open`

**Info**, CONFIRMED (votes: Info, Info, Low). `src/tui/actions.rs:325`

- **What:** Enter on a file (and the menu's Open) calls `crate::open::open`, which waits on `gio open` with `.status()`; for a DBusActivatable handler that does not answer `org.freedesktop.Application.Open`, gio waits the D-Bus reply timeout, leaving the raw-mode TUI frozen and un-cancelable (STOP is only checked between frames).
- **Scenario:** Default handler for an archive/document type is a DBusActivatable app that is wedged or slow to activate; user presses Enter in `flea --tui`; the screen stops redrawing and Ctrl+C is queued until gio returns (documented 0.32-0.75 s normally, up to the D-Bus timeout when the app hangs). No data is touched; it self-heals.
- **Fix:** Run `open::open` on a worker thread (or spawn `flea --open <path>` as a tracked child in `model.launches`, the way `openTerminal` already does) and surface its exit code when it arrives.

### S8. `--select file://<host>/path` on the CLI resolves to a path relative to the working directory

**Info**, CONFIRMED (votes: Info, Info, Info). `src/main.rs:94`

- **What:** `select_target` strips the literal `file://` and keeps whatever follows, so a URI with an authority becomes the relative path `host/path`; the window then opens `./host` relative to the caller's cwd. Not reachable from the D-Bus service, which refuses foreign hosts itself.
- **Scenario:** `flea --select file://nas/share/doc.pdf` (a URI copied from a browser or script) opens `./nas/share` instead of failing; verified with `--print-target`: `file://host/etc/passwd` printed `host/etc host/etc/passwd`.
- **Fix:** Require the remainder after `file://` to start with `/` (or `localhost/`), and otherwise call `usage()`; percent-decode after that check.

### S31. ~/.config/flea/scripts that is a symlink, or scripts that are symlinks, are never listed

**Info**, CONFIRMED (votes: Info, Info, Info). `ui/Scripts.qml:44`

- **What:** The lister runs `find <dir> -maxdepth 1 -type f -perm -u+x` with the default -P, which neither descends a symlinked starting directory nor matches symlinked files, so a dotfiles-managed scripts directory yields an empty submenu with no explanation.
- **Scenario:** Operator keeps ~/.config/flea/scripts as a symlink into their dotfiles repo (or symlinks individual scripts into it). The Run script row never appears (Menu.js:123-124 drops the row when the list is empty) and nothing says why.
- **Fix:** Use `find -H` so the command-line directory is followed while entries are not, and document that scripts must be regular files; or say in the status centre when the directory exists but lists nothing.

### S40. The rail takes file:// URIs from any application, so a foreign drag can plant an arbitrary local path on the shelf

**Info**, PLAUSIBLE (votes: Info, Info). `shelf/ShelfRail.qml:84`

- **What:** ShelfRail.qml:69-95 accepts any `text/uri-list` drop and Model.pathsFromUris (Model.js:196-207) turns every `file://` line into a path that `flea shelf add` records if it exists. Nothing checks that the dragging application could itself read the file; a `file://host/path` form even resolves relative to the bar's cwd (shelf.rs:272 std::path::absolute). Web content can set text/uri-list on a drag in some browsers.
- **Scenario:** A page's drag payload names `file:///home/<user>/.ssh/id_ed25519`; the user drops it on the edge rail thinking it is the page's image. The pile now holds the key; a later `t`/`c`/`m` on the whole pile sends or copies it along with the rest. The row does show its leaf name and full-path tooltip, so this needs the user not to look.
- **Fix:** Show a distinct transient for a drop whose source is not Flea ("Added <n> from <app>"), and consider refusing `file://` URIs with a non-empty host; with identity recorded per entry (previous finding) a planted path is at least pinned to what was dropped.

### S52. Shelf plugin refresh overwrites and prunes the user's own edits under ~/.config/omarchy/plugins at every launch after an upgrade

**Info**, CONFIRMED (votes: Info, Low, Low). `src/shelfplugin.rs:56`

- **What:** install() copies the packaged plugin files over the user's plugin directory and then deletes every file there that the packaged source does not ship. refresh() runs it on every GUI start whenever the manifest versions differ, and sync() on the Settings toggle, without any prompt or backup.
- **Scenario:** A user who customised Panel.qml (or dropped an extra helper file) in ~/.config/omarchy/plugins/io.github.thisisgm.flea-shelf/ loses those edits silently the first time Flea starts after a package upgrade bumps the plugin version. Intended by design ('the copy in the user's own plugin directory is the one the bar reads'), but it is a write into the user's config tree that removes files.
- **Fix:** Skip the prune (or move extras aside) and say so in the console line; or only copy when the destination's manifest still carries the packaged marker so a user-modified copy is left alone with a warning.

### S56. A dangling symlink at the save name blocks that name entirely

**Info**, CONFIRMED (votes: Info, Info, Info). `src/backend/picker.rs:143`

- **What:** When the existing entry at folder/name is a symlink whose target is missing, the follow-open in the save probe fails and the whole probe errors ('Could not inspect ...: file or folder not found'), so saveReady never becomes true and the user cannot save under that name at all, although the sensible outcome is a collision review of the link. Safe direction (nothing is written) but a confusing dead end, and one an app-planted name can trigger.
- **Scenario:** ~/Downloads/dangling.txt -> /nonexistent; an app opens SaveFile with current_name=dangling.txt in that folder. Reproduced: the backend answers ok:false with the not-found error; the UI shows it as saveError and Save stays disabled until the user picks another name.
- **Fix:** Treat a dangling link as a collision with a link (see the symlink finding) rather than an inspection failure, with a message that says it is a broken link.

### S60. While the confirmation is open, every 2.5 s check re-spawns one gio per Trash item and re-walks every reviewed tree

**Info**, PLAUSIBLE (votes: Low, Low, Low). `src/backend/trashbrowse.rs:514`

- **What:** valid() runs list() plus, for every reviewed item, contents_unchanged() (lstat/readdir of the whole snapshot) and detail() (a gio info process). TrashView's Timer (ui/TrashView.qml:457) repeats it every 2.5 s after the previous one finishes, and the delete op runs it once more before the first unlink. prepare() already costs one detail() per item plus the snapshot, so Empty Trash on a large trash spends 3N+ process spawns before anything is deleted.
- **Scenario:** Trash with ~5,000 entries: Empty Trash takes a minute or more to show the dialog, then keeps a core busy spawning gio while the dialog waits, and the confirmed delete waits through another full validation. Not a data-safety issue (the trash worker is its own thread), but it makes the safety check itself expensive enough that users may reach for another tool.
- **Fix:** Have check compare the cheap signals first (gio list hash, payload/info identities) and only re-walk trees when the top level moved; batch attributes with one 'gio info' per volume or read the backing lstat directly instead of spawning per item.

### S80. `x` on a pinned row is a silent no-op, contradicting the README's "x is what takes it off the shelf"

**Info**, CONFIRMED (votes: Low, Low, Low). `shelf/Panel.qml:385`

- **What:** `onRemoveRequested` calls `shelf.forget(path)`, which is `Shelf::settle` (shelf.rs:66) and settle explicitly keeps pinned entries; the command exits 0 so the card shows nothing and the row stays. The trailing button on a pinned row is Unpin, but the keyboard x documented for pinned rows does nothing.
- **Scenario:** Cursor on a pinned row, press x: nothing happens, no message.
- **Fix:** Either make forget unpin-and-remove, or have the card route x on a pinned row to unpin and say so.

### S81. Pane ctrl-z after a shelf-token drop puts files back without restoring their shelf entries; a shelf z after a card move started from a second bar can cancel both moves

**Info**, PLAUSIBLE (votes: Info, Low, Info). `src/backend/shelfdrop.rs:83`

- **What:** Two cross-surface bookkeeping gaps: (a) a token drop's `TransferDone` entry goes into the pane's journal (opsdispatch.rs:273-278) and the forward thread settles the pile, but the pane's undo only reverses the files, so after ctrl-z the files are home and the shelf no longer lists them; (b) the shelf widget is instantiated per bar, so on a two-monitor box each card has its own `action` slot while `flea shelf cancel` writes one shared marker (shelfops.rs `cancel_file`), so esc in one card cancels a move started from the other. Neither loses data.
- **Scenario:** (a) Drag three pile rows into a Flea folder, then ctrl-z in Flea: files return, shelf shows an empty pile and the rows must be re-added. (b) Start a long move from monitor 1's card, open monitor 2's card and press esc: the first move is cancelled.
- **Fix:** (a) On pane undo of an entry that came from a shelf token, re-add the `from` paths (the entry could carry a `shelf: true` flag). (b) Key the cancel marker by the transfer's pid (print it in `transferstarted`) so esc cancels only the run the card started.


## Not reviewed

The completeness critic's list of surface no sweep covered, verbatim:

```text
UNREVIEWED OR THINLY REVIEWED SURFACE (none of the listed sweeps names these):
1. The new CSV/TSV/sheet table preview on the GUI side (ui/PreviewTable.qml, ui/js/Delimited.js, ui/PreviewLines.qml tabular branch). The fix only jailed bsdtar. I reviewed it: finding 1.
2. The spreadsheet readers themselves (src/sheet.rs, src/sheetxlsx.rs, src/sheetods.rs, src/sheetxml.rs) run in the unjailed flea --sheet. I reviewed them: finding 2. Also unreviewed: a FIFO named *.fods makes File::open block forever. The GUI kills it on a cursor change, so it is minor, and it is the same kind of name-over-file-type routing as the known FIFO findings.
3. The 500 MiB text preview (commit f8fe7a0, ui/PreviewText.qml). The gate is the listing's stale row.s, so a file that grew since listing, or a FUSE file that reports a small size, is read whole into the GUI. The author measured 3 GB resident at 500 MB. Not reviewed in depth.
4. src/backend/menu_actions.rs (554 lines) + menu_registry.rs (572) + providers.rs: the GIO app-registry child with its own pidfd/kill/poll code, New File (create_file, which redo also uses), and the provider status probes. Not reviewed; this is the largest backend code with no named sweep.
5. src/backend/permissions.rs + ui/PermissionsDialog.qml + ui/js/Permissions.js: reviewed briefly above (see safeguards).
6. src/backend/redo.rs: reviewed (see safeguards). The redo stack has never been checked against the new trash-based undo, or against undo.rs's step reversal that is not atomic.
7. src/open.rs / ui/Opener.qml: Enter on a downloaded .desktop, .sh or AppImage defers entirely to `gio open` and the mime default. Flea has no refusal or confirmation of its own. Not reviewed.
8. src/backend/mime.rs (in-process content sniffing), linecount.rs, thumbwrite.rs/thumbcache.rs (cache PNG writing and naming), src/launcher/prewarm.rs, src/gui.rs/vulkan.rs/heap.rs/thp.rs (the environment handed to qs; one finding exists), packaging/*.service and flea.portal (D-Bus activation), tools/flea-gio-auth (expect script: prompt matching and injection by URI), and ui/ConvertDialog.qml, OpenWithDialog.qml, MediaSound.qml. None reviewed.

LIKELY DUPLICATES AMONG COLLECTED FINDINGS:
- The EXDEV kept-entries data-loss path is reported three times. 'A cross-device move that has to keep changed entries leaves an unjournaled complete copy and a hollowed source' (opsreq.rs), 'Cross-device move: a child that cannot be removed aborts remove_copied … (Err arm …)' (copyfile.rs) and 'EXDEV move whose removal phase errors leaves an unjournaled complete copy and a partly removed source' (copyfile.rs) are one root cause. The last two are the same finding.
- The stale row-index race appears as the known 'Delete during a background re-list', 'A search's final rank reorders every row … Esc then Delete' and 'The stale-row-index race also reaches the paths request'. These are one root cause (index-addressed ops) with three triggers.
- Key auto-repeat appears as 'Held Delete/Enter/Ctrl+Z/Ctrl+V auto-repeat' (Focus.js), 'Held x/Delete in Settings' and 'Holding Delete in the TUI'. One class across three surfaces; they could be merged into one finding with three sites.
- The Trash view failing closed appears as 'One unreadable Trash entry makes the whole Trash view … fail closed', 'The Trash listing is all-or-nothing on gio's raw output (newline/non-UTF-8)' and 'Flea's quarantine lives inside Trash/files … kills the Trash view and the auto-sweep'. These are the same all-or-nothing parse. The first two overlap most.
- Search walks appear as 'Deep search walks … into hung FUSE/network mounts' and 'Search and folder-size walks cross every mount boundary, including autofs'. They overlap heavily. On the dirsize side, 'dirsize refusal … still lstat()s the mount' and 'Folder-size mount refusal is lexical on the un-canonicalised base' are two gaps in one refusal check.
- The into-itself guard appears as the bind-mount blindness (canonicalize) and the case-insensitive miss. Same guard, two holes.
- Blocking on the single request loop appears as 'A mount that stops answering blocks the backend's only request thread', 'Trash on a stalled mount holds the operation slot forever', 'Columns view peeks … on the backend's only loop thread', 'Loop-thread stats reach paths the user did not open', 'Rename on rclone … synchronous copy-then-remove on the request loop', 'Undo of any cross-device move runs … on the request loop', 'Undo of a copy now runs three unbounded gio processes … on the request loop' and 'Duplicate cannot be cancelled'. One architectural class; several could be consolidated.
- Name-based kind routing that ignores file type appears as 'A FIFO named *.pdf freezes the GUI thread', 'A FIFO … under a media name wedges … at teardown', 'Media probe runs on any file type: a FIFO named like a clip' and 'A fifo under an image or archive name hangs Convert/Extract'. Same root with four sinks. The .fods open-blocks note above is a fifth.
- Leftover .flea-work-* directories appear as 'failed extract … read-only directory leaves .flea-work-ext-*', 'A fifo … leaves a hidden .flea-work-*' and 'Quit while an extract … orphans its .flea-work-*'. Same cleanup gap, three triggers.
- Preview renders that persist appear as 'Space preview keeps up to 48 full-size lossless renders … never cleaned at exit' and 'Space preview renders and the shared thumbnail cache outlive permanent delete'. They overlap on the preview renders.
- The trash-based undo change appears as 'Undo of a copy now trashes it on the destination volume (.Trash-1000)', 'Undo of any copy onto a GVFS share or phone now always fails and spends the journal entry' and 'Undo of a copy now runs three unbounded gio processes'. Three facets of the same diff hunk (undo.rs discard()). They are distinct consequences but should be tracked as one fix.
- The shelf chosen-set widening appears as 'With nothing chosen, Move, Copy, Zip and Send also take every pinned row' and 'The chosen set is read again … silently widens to the whole pile and its pins'. They overlap.
- The renamecompat fallback appears as 'Renaming a folder on a case-insensitive rclone remote can delete the folder's contents' and 'Rename on rclone/megafs/gvfs-WebDAV mounts falls back to a synchronous copy-then-remove on the request loop'. Same code path, different consequences.
- 'A failed (non-cancelled) shelf move or copy leaves a truncated file under the real name' repeats the known 'failed copy leaves truncated file under real name' on the shelf path.
- 'Shelf plugin refresh writes through a symlinked plugin directory' already calls itself an extension of the refresh-prunes-edits finding.

```

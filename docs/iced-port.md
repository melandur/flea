# The iced port

The operator ruled on 2026-10-29 that the whole window moves from QML on Quickshell to Rust with
iced, staged so the QML tree stays shippable until the iced one reaches parity.

## Decisions that hold for every stage

- **`crates/flea-iced` is its own workspace with its own `Cargo.lock`.** The backend crate keeps an
  empty `[dependencies]` and a byte-identical `Cargo.lock`; only the new front end pulls in iced.
- **The backend stays a child process, `$FLEA_BIN --backend`, over the same line protocol.** The
  plan first said "turn the protocol into an in-process library API" and that was dropped before any
  code: `heap::pin_mmap_threshold`, the thumbnail pool's threads and the drain on `quit` were all
  measured inside a process of their own, and moving them into iced's allocator would void those
  measurements. It also leaves `tests/protocol.sh`, `tests/thumbs.sh` and `tests/ops.sh` valid
  unchanged, and `docs/protocol.md` stays the one contract.
- **Logic lives in toolkit-free modules with unit tests** (`pane.rs` is the model): every input is a
  method and every output an `Effect` the shell carries out, the way `ui/js/*.js` imported no QML.
- **A key that is not ported yet says so in the status bar** rather than doing nothing.
- The window runs under the app id `com.thisisgm.flea.iced` while both front ends exist.
- `FLEA_ICED_TRACE` writes every request to stderr, the counting seam `thumbRequests` and
  `listRequests` were in QML.

## Stages

1. **Listing pane (done, unmeasured).** List, held window, rule 1 to 3, watched re-read with the name
   anchor, `locate` for a parent's anchor past the first screenful, Up, Down, Left, Right into a
   directory, Enter and double click through `flea --open` (exit 3 navigates), F5, Ctrl+H persisted
   through `flea --ui-state`, sort, folders-first and wrap-at-ends read from `ui.json`, the Omarchy
   palette, quit drained through `quit` with the 30 s deadline. Driven live against a 100,001-entry
   directory: first screenful and count correct. **Not yet measured** against the QML window for
   first paint, scroll CPU or PSS, and the keyboard path has been unit tested but not driven live.
2. **Write operations (done, keyboard and mouse undriven).** `ops.rs` ports ui/js/Ops.js,
   Selection.js and Tap.js and PaneWire's write handlers: Space, Ctrl+A, click, Ctrl+click,
   Shift+click and a double click counted from two presses; Ctrl+C and Ctrl+X resolve rows through
   `paths`, Ctrl+V sends `transfer`, and a cut is spent by the line that accepts it and kept on a
   refusal; one running transfer with its sticky progress line, its queue, the first failure and
   the done line; Delete trashes with the cursor landing where the block was; F2 opens the inline
   editor, which keeps the draft and says why on a refusal; Ctrl+N makes a folder and opens its
   editor once the cursor lands on it; Ctrl+Z undoes; Escape unwinds an error, the editor, the
   selection, the running transfer and then the newest queued one. A selection or an open editor
   owes the watched re-read instead of running it, and every input pays the debt once it clears.
   `live_tests.rs` drives new folder, rename, undo twice, copy and paste through the real
   `flea --backend` when `FLEA_BIN` is set, and a mutation of the mkdir request turns it red.
   **Deliberate deviation:** the hints say "Ctrl+Z undoes" and "Ctrl+V pastes"; ui/js/Status.js
   still names `z` and `p`, bare letters the key cut removed. Not ported yet: duplicate (menu only),
   the held-Space paint, the transfer card, retry selection, trash undriven live (`gio trash`
   refuses /tmp).
3. **Marks, the filetype tier and thumbnails (done, unmeasured).** `build.rs` generates the mark
   paths from `ui/js/Icons.js`, the families from `ui/js/FileTypes.js` and the colours from
   `vendor/yazi-icons.toml` with the generator's own precedence, so nothing is copied by hand, and
   it fails the build when a family names a mark with no path. `marks.rs` draws them as SVG at
   stroke 2 with square caps and mitered joins, one cached handle per mark and colour.
   `contrast.rs` ports `ui/js/Contrast.js` and Qt 6's own integer `QColor::darker`, pinned to two
   values measured with Qt on this box, because float HSV is off by one in the last bit. The pane
   ports `ui/js/Thumbs.js`: the 70 ms first settle and 120 ms after, restarted by every viewport
   report and every arriving window, ask only visible rows carrying `t` in the map's absent state,
   cancel waiting rows that left, drop answers while a listing is in flight, cap at 240. A cache PNG
   is decoded off the UI thread at twice the 23 px slot, keyed by path and decoded again on every
   answer so a regenerated thumbnail cannot show its old frame. The live test thumbnails a real
   image through the real backend with `XDG_CACHE_HOME` sandboxed, asserts only the visible row was
   named, and goes red when the settle is mutated into a sweep.
   **Found while porting:** Stage 1 took muted from `dark_foreground`; `ui/js/Palette.js` takes the
   file's `muted` or `Qt.darker(foreground, 1.4)` and lifts it to 3:1, and symlink, executable and
   error are lifted to 4.5:1. Fixed. And the first live run drew no thumbnails: iced's scrollable
   reports only on a scroll, so the first settle named row 0 alone; the window's resize events now
   size the viewport and the pane starts from its row guess. Not ported: `display.hyprlandIcons`'s
   round caps, and the per-view variants (dual-pane and lifted inks).
4. **Rail, split in two.**
   - **4a, places and devices (done, keyboard and menu undriven live).** `rail.rs` ports
     `ui/js/Places.js`: Home and the XDG user folders, Trash, the stored favourites with
     `recordError`'s reasons, the sidebar marks and the width stops. `devices.rs` ports
     `ui/js/Devices.js`, `ui/js/Eject.js` and the `ui/DeviceMounts.qml` service as a pure state
     machine: the five-second lsblk poll with its 10 s bound (a listing that fails keeps the rows),
     mount then open once listed, `gio mount -u`, and `gio mount -e` whose verdict is read off the
     next listing and never off gio's exit status, each one-shot timer stale-proof by generation.
     `railshell.rs` and `railview.rs` are the shell's half: Tab moves the keyboard, Ctrl+G shows or
     hides the rail and saves `places.rail` as a one-leaf patch (measured: a nested patch merges leaf
     by leaf), Up, Down, Enter that stays, Right that hands the keyboard to what it opened, Escape,
     `m` and right click for the release menu, drawn over its row with a backdrop that takes the
     closing press. iced draws no ellipsis, so a label is cut to an estimated budget and the row
     clips besides; the first live run showed a 37-character drive name painting over the listing.
     Not ported: Trash view (says so), the trash count, phones, drive size, rail scrolling,
     favourite editing and the rail cursor's identity-preserving move across a poll (it clamps).
   - **4b-i, network places (done, menu and rename undriven live).** `protocols.rs` and
     `mounts.rs` port the URI and listing halves of `ui/js/Protocols.js` and `ui/js/Mounts.js`:
     the measured default-port table, the authority-first host and port reading (PR #21's
     `inbox@2026` trap is a test), `shareName`'s one-word connector cut against the URI's own host
     (issue #36's labels are tests), the canonical form, and the bookmark line edits. `network.rs`
     is `ui/NetworkMounts.qml` and `ui/MountListing.qml` as a state machine: the poll that keeps
     its rows on a failed listing and runs a re-read asked for mid-listing, one row per normalized
     place with a saved label winning, and an open of up to three legs each bounded at 15 s, where
     `gio info` decides and the mount's exit code never does. `process.rs` runs every gio call
     under its deadline with `LC_ALL=C`. `netshell.rs` reads the bookmarks file before every edit
     and refuses on a read that fails for any reason but absence, writes through an exclusive temp
     and a rename, answers Remove's three ways, and holds the rail's rename field and the share
     browser. A stubbed `gio` that speaks Spanish unless the locale is pinned drives a bare root to
     its share list and a share past a Spanish already-mounted refusal to its folder, and turns
     red when the pin is removed. **Deviation:** the unmount is bounded too, which QML's is not.
     **Gap:** a place that needs a password says signing in is not here yet, unless
     `places.rememberPasswords` is on, in which case a plain mount reads the keyring as QML does.
   - **4b-ii, signing in (done, the dialog not yet seen on screen).** `form.rs` ports the form
     half of `ui/js/Protocols.js` and `NetworkDialog.valuesFor`: the five protocols and the fields
     each shows, `encodeURIComponent`'s exact set, no default port ever written, the TLS box moving
     the scheme and its port together (advloop round 2's `dav://host:443` is a test), and a saved URI
     reopening the form with its own port. `netopen.rs` is `openShare`, `failMount` and
     `finishRequest`: a credentialed place runs `tools/flea-gio-auth` with the password on stdin and
     never in its argv (a stub helper records both), exit 3 and 5 become the host key question with
     its fingerprint, a refused password is forgotten while an unavailable helper keeps it, and gio
     info still decides after any helper refusal, so "already mounted" opens. The trust is spent by
     the one attempt it was given for. Every leg carries a generation, so a cancelled attempt's late
     answer cannot complete a newer one. A deadline answers a dialog's request and never reopens the
     dialog for a password, which the first port got wrong and its test caught. `dialog.rs` and
     `dialogview.rs` are the dialog: "Connect and save", "Retry", "Connect anyway", "Connecting...",
     "Saving...", the failure kept across a close and shown once on reopening the same place, and a
     connected place saved as a favourite through `flea --favourites`. Edit reopens the form on a
     saved place and rewrites its line once the new address connects. A network favourite opens
     through the same service. **Not ported:** Dropbox (its row, its status check and the dialog's
     install section), and the form's own focus ring beyond iced's Tab order.

   **The file budget is held here too.** `tools/flea-file-budget` does not scan `crates/`, but four
   files had passed its 400-line hard cap, `pane.rs` at 836. Tests moved to `*_tests.rs` beside each
   module, the way `src/shelf_tests.rs` sits beside `src/shelf.rs`, and `thumbs.rs`, `watch.rs`,
   `sentence.rs`, `opreplies.rs`, `view.rs`, `keys.rs` and `boot.rs` came out at their seams.
5. **Preview, split in two.**
   - **5a, the overlay and the kinds without a native library (done, not yet seen on screen).**
     `kinds.rs` is `ui/js/Kinds.js`. `delimited.rs` is `ui/js/Delimited.js`: quotes open a field
     only at its start, doubled quotes, newlines inside a cell, the BOM, the sniff's agreement and
     tie rule, and the number test, every one of whose cases is pinned to the JS regex's own answer
     taken with node (that run caught mixed group separators, `1,234.567.890`, which the first port
     refused). `preview.rs` is the overlay's state and `ui/js/PreviewKeys.js`: inset Up and Down move
     the listing with the preview following, settled at 120 ms so a held key loads the first file
     and the last and nothing between; Right and Space expand, Left leaves an inset preview, Escape
     leaves from either. `previewshell.rs` loads text into a read-only editor, reads a table, runs
     `flea --sheet` for a spreadsheet, renders an image through `flea --preview-image` in the
     thumbnail jail, and asks the backend for one archive row's index; every answer carries its path
     and is dropped if the cursor moved on. Enter on an archive opens this view unless `open.rules`
     claims the ending. A live test reads a real zip's index through the backend and renders a PNG
     through the jail. **Readings of the source over the prose:** Space only ever expands, which is
     what `PreviewKeys.act` does, although AGENTS.md says it also returns to inset. **Deviations:** a
     table's header row scrolls with its records so its columns stay aligned sideways, where QML pins
     it; text is drawn without syntax colours until `ui/js/Syntax.js` is ported as its own step.
   - **5b, PDF and media (done, not yet seen on screen).** The operator chose poppler-glib and
     GStreamer on 2026-10-29, both already installed on an Omarchy box. `pdf.rs` keeps one worker
     thread holding the open document, because poppler's objects stay on the thread that made them
     and re-parsing for every page turn would cost what QtPdf never did; a page is fitted to the
     frame at zoom 1, the ladder runs to 4 in quarter steps, and a test writes a real two-page PDF
     through cairo and reads its colours back at both zooms. `media.rs` is `playbin` with an RGBA app
     sink holding one frame and dropping the rest; a test encodes a two-second clip with GStreamer
     and plays it back to a red 64x48 frame and a two-second duration. `gst::init` runs on the first
     media preview and never at startup, rule 5's laziness carried over (unmeasured against QML's
     20 MB). Expanded Left and Right turn the page or seek 5 s; `m` mutes media only, one session
     flag; a playing file is drawn about 30 times a second and a paused one only polled; dropping
     the preview is what stops playback. **Not ported:** the PDF's Tab-walked control strip (its
     buttons are clickable), the media seek bar's scrubbing, the video poster mark, the sample rate
     fact. **Packaging, when the iced binary ships:** `poppler-glib`, `gstreamer`, `gst-plugins-base`
     and `gst-plugins-good` join `depends`, and `gst-libav` belongs in `optdepends` for codecs.
     **Test hygiene:** a stub a test has just written can be refused with `ETXTBSY` by a sibling
     test's fork still holding its write descriptor; `testexec::wait_executable` closes that, after
     one red in a full parallel run and six greens since.
6. **Settings, themes, tabs, split, search, filter and the path bar, in three parts.**
   - **6a, search, sort and the chrome (done, the keys undriven live).** `search.rs` is
     `ui/js/Search.js` and `ui/js/Sort.js`: Ctrl+F walks this folder and Ctrl+Shift+F every folder
     below; typing restarts a 120 ms timer and the settled query is walked once; a counter drops
     every line of a walk a newer one superseded; `searched` means the rows were just ranked, so the
     window is asked for again from row 0; Escape cancels a running walk and then returns; Right on
     the line waits for the walk of exactly this query; a single tap on a result reveals it in its
     folder; and a search listing defers the watched re-read like a selection does. Sort chords start
     size and date descending and reverse on a second press, header clicks start ascending, and the
     order rides every later listing this session. `searchkeys.rs` is the query line's own keys,
     type-ahead from the listing, typing back into the results, and the clipboard, with
     `QueryLine.oneLine` checked against the JS in node. `pathbar.rs` is `ui/js/PathBar.js` and
     `ui/js/Crumbs.js`; `chrome.rs` draws the crumbs, waits out the double-click interval before a
     single click navigates, opens the path bar on a double click, and completes with Tab through
     `peek`, matched on folder and hidden flag. A live test walks a real tree and reads the ranking
     back: the whole name, then a name the query starts, then a later word.
     **Not ported:** the filter, which the Default keymap no longer reaches and which arrives with
     the chrome's buttons; the search's location column as its own column (it is drawn muted beside
     the name).
   - **6b, split (done, Ctrl+T and Tab undriven live); tabs (not ported).** `split.rs` gives each
     pane its own backend child, as QML does. The focused pane is the shell's own `pane`, the other
     waits in a `Slot`, and a message for the other is run by swapping it in, handling it exactly as
     the focused pane's would be, and swapping back, so neither pane has a lesser code path; every
     message a pane schedules for itself carries its side, and every backend line its spawn key.
     Ctrl+T enters the split on the folder it was pressed in (2026-10-08) and hands the keyboard to
     the new pane (2026-09-18), leaving puts it back on the primary; Tab in the listing switches
     panes; a click in the other pane focuses it first; the view and the focused side are saved as
     one-leaf patches, and a window left split reopens split on the start path with its focus. A
     close quits both children and waits for exactly the ones it asked, so a dropped second pane's
     late exit cannot end the window before the primary has drained. Each spawn has its own
     subscription key, because a key per pane would never restart once that pane's child had exited.
     **Deviation:** leaving the split drops the second pane and its child rather than keeping it
     hidden, AGENTS.md rule 6; re-entering lists the folder again. **Tabs** have no key in the
     Default preset since the key cut and are opened from the listing's context menu, which is not
     ported; they arrive with it. Measured once: a sandboxed launch with `view: "dual"` drew both
     panes with the focus restored, and killing the window left no backend child behind.
   - **6c, the settings panel and the theme catalog.**
7. The picker for `tools/flea-portal`, then `flea --gui` switches to the iced binary, after a
   field-bench run says the switch does not lose a column Flea wins today.

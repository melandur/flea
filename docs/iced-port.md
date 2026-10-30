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
   still names `z` and `p`, bare letters the key cut removed. Not ported yet: the held-Space paint, the transfer card, retry selection, trash undriven live (`gio trash`
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
     it. Syntax colours arrived in 8d-ii-b-4.
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
   - **6c-i, the settings panel and the theme catalog (done, the panel not yet seen on screen).**
     `build.rs` generates the 95 Strata palettes with `tools/flea-themes-gen`'s eight tokens and its
     refusals, checked identical to `ui/js/Themes.js` line for line; a chosen palette's keys go ahead
     of the desktop's `colors.toml`, so they win and the rest, the selection fill among them, stays
     the desktop's. `settings.rs` and `places_rows.rs` are `ui/js/Settings.js` and its section files:
     every section's rows with their labels verbatim, `focusable`, and `changeSetting`'s one write
     rule; the Menus masters carry the shelf beside the hidden set as `ViewState` does.
     `settingspanel.rs` is the panel's keyboard and activation from `ui/SettingsPanel.qml`: `,` opens
     it from anywhere not being typed in, every other key stops at it. `settingsapply.rs` reads the
     merged document `flea --ui-state` prints with boot's own `Prefs` parse and applies what changed:
     the order (dropping this session's), hidden files, the wrap, thumbnails, file type colours, the
     theme and the rail, to both panes in the split. A live test puts every check, every value of
     every choice and both menu writers, 311 patches, through the real binary against a sandboxed
     state directory, and a misspelled key turns it red with the backend's own sentence.
     **Written but not yet applied in this port:** density, columns, the address bar, the view mode,
     the preview column and its loading, thumbnail size, text size (the crate's metrics are fixed at
     13 px on a 37 px row, which is what a 14 px Omarchy base gives), Hyprland-aware icons, the menus
     and key hints, the shelf. **Not ported:** favourite reordering and removal from the panel, the
     keyboard sheet, and File types (6c-ii).
   - **6c-ii, File types (done, the card not yet seen on screen).** `openrules.rs` is
     `ui/js/SettingsOpen.js`: the ending's shape and normalisation, the stored table with every
     ill-shaped rule left out, one rule per ending kept in ending order, and the whole table written
     back, because `src/uistate.rs` refuses a list holding one bad rule. `openrulecard.rs` is
     `ui/OpenRuleDialog.qml`: the ending, the search, the application list with its cursor, Save only
     with a valid ending and an application under the cursor; `x`, Delete and the row's own mark
     remove a rule. The catalogue is `menuaction` `installed` under a request id of the panel's own,
     asked when File types is shown and dropped with the panel. A live test reads the real catalogue
     and writes a rule built from it through the real validator. **A quirk carried over, not fixed:**
     `OpenWith.matching` lower-cases each label and not the typed text, so a search with a capital
     letter matches nothing, in QML and here alike.
7. **The chooser (done, its keyboard not yet driven live) and the opt-in launch (done); the default
   switch (not taken).**
   - `src/picker/` is the chooser window `flea --pick` opens for `tools/flea-portal`. `mod.rs` is
     `ui/js/Picker.js`, the request with every field defaulted, the labels, the chips, the name rule
     mirrored from `ops.rs`, `Format.fileUri`, and the reply written in Picker.js's own key order;
     `recent.rs` is `ui/js/Recent.js` plus the XBEL read `ui/PickerRecent.qml` left to Qt, here
     through `roxmltree`, a reader with no dependencies of its own, rather than a hand-rolled one;
     `state.rs` and `replies.rs` are `ui/picker.qml`'s rules between a key and a backend check: the
     newest listing only, a mark the backend reviews, `validate` on every listing, the save probe
     keyed on folder and name and probed again when either moved, the collision asked before a
     `review`, and exactly one answer. The reply is a temp beside the reply path and a rename, and
     the backend is told to quit only once it has landed. The tests are tests/js/picker.js and
     tests/js/recent.js case for case, the chooser's scenarios, and a live test against the real
     backend: a chip narrows the listing, a mark comes back with its bytes and survives All files,
     Recent lists the history's surviving local file and nothing remote, and a save over a file that
     exists asks first and answers that file's URI. Turning the chip's globs off on the wire turns it
     red. A sandboxed launch drew the chip row, the rail with Recent first, the listing and the
     footer.
   - **Deviations.** The rail lists Recent, the favourites, home and its folders and the mounted
     volumes; a network place, a phone and an unmounted volume say they open from the Flea window,
     because the chooser does not mount yet. A
     malformed history file reads as an empty Recent, where Qt's reader kept the bookmarks before
     the break. The focus ring is the rail, the list, the Filename field and the two collision
     buttons; the chrome's buttons answer the pointer only.
   - **The launch.** `FLEA_FRONTEND=iced` makes `flea --gui` and `flea --pick` exec `flea-iced`
     (`FLEA_ICED_BIN`, else beside the binary, else this tree's own `crates/flea-iced/target`), with
     `FLEA_BIN` handed over the way `qs` gets it and huge pages off across the exec exactly as for
     `qs`, which for iced is unmeasured. Asked for and not found, it refuses with exit 2 rather than
     falling back; empty or any other value is the `qs` window. `--select` is not honoured yet and
     says so. `tests/modes.sh` pins all of it against stubs and goes red with the seam disabled.
   - **The live keyboard run (2026-10-29), on the operator's desktop.** A sandboxed tree, state and
     cache, keys sent with `wtype` only while the iced window held focus AND its last `list` named
     the fixture, every step checked against `FLEA_ICED_TRACE` (requests, and now every key the
     window was handed), the disk and a screenshot. Driven and correct: the cursor keys, Right into a
     folder and Left back onto it, Ctrl+H (and `ui.json` written), F2 rename on disk, Ctrl+N then
     Escape then Ctrl+Z, Space, Ctrl+C, Ctrl+V into a folder with the status line, Ctrl+F and a
     ranked shallow search, Ctrl+T split with the keyboard on the new pane and its child quit on
     leaving, Right preview and Left close, `,` the settings panel walked by keyboard and its toggle
     written and re-listed, Ctrl+G (hides the rail, as in QML) and Tab into the rail. The chooser:
     two marks and Enter answered both URIs, a typed save name was probed, reviewed and answered,
     closing the window answered a refusal, and one Escape in the Filename field cancelled.
     **Two defects found and fixed.** A focused text field captures Escape, so no editor closed on
     it (`keys::captured_escape` forwards exactly the captured presses to the same router, in both
     windows). `.window(Settings)` replaces the whole window settings, so the earlier
     `.exit_on_close_request(false)` was undone: closing the window exited without `quit` and the
     drain, and a closed chooser wrote no reply at all, which the portal reads as a fault; the field
     now sits inside the settings. **Not explained:** the first run, before keys were traced, read
     each arrow as an enter-and-leave pair of `list` requests, and it never recurred; `wtype`'s
     keymap swap is the guess. **Not driven:** Enter on a rail place (it lists the operator's real
     folders), Super+W (a `wtype` key cannot fire a compositor bind; the close was driven with
     `hl.dsp.window.close` on the iced window's own address), trash (`gio trash` refuses `/tmp`).
     **The run took the operator's focus**, and once their own click moved the window onto a live
     SFTP mount, where two read-only `list`s went before the fixture guard existed.
   - **The same run, headless, without the operator's focus.** sway 1.12 run with
     `WLR_BACKENDS=headless` on its own socket, never as a session, with the iced window, `wtype`
     and `grim` all pointed at that socket alone; the operator's Hyprland never sees the window. 27
     checks, all passing, replaying every step above against the trace, the disk and the chooser's
     reply file. Both fixes were reverted one at a time and the run turned red on exactly the steps
     each one owns, so it tests what it says. **Three things this box needed**, each found by it
     failing: something here SIGKILLs any process named `sway` (the same binary under another name
     runs), so the run uses a renamed copy; `wtype`'s short-lived virtual keyboard sends its key
     before a client in a keyboard-less compositor has its keymap, so every call waits 300 ms
     first; and the editor's Escape is only a real test once text has been typed into it, because
     a field that never took focus lets Escape through uncaptured and passes with or without the
     fix. **Return is never pressed on a file there**: it runs the desktop's handler, which opened
     a terminal inside the test compositor on the first headless run. It is `tests/iced-headless.sh`, its
     fixture, state and screenshots under `FLEA_FIXTURE_ROOT/flea-iced-headless` through the
     sandbox guard, listed in `tests/run-all.sh`'s not-run inventory because sway is no dependency.
   - **The default stays `qs`.** Switching it waits on a field-bench run showing the switch loses no
     column Flea wins today, which has not been done.
8. **Parity, in four parts.** What the QML window does that the iced one does not, each part a
   commit of its own.
   - **8a, the listing menu (done, driven headless).** `menu.rs` ports ui/js/Menu.js: the one
     inventory in the Menus section's order, the file rows over a row and the background rows over
     empty space, each row's availability rule, `isHidden`'s lock on Open and the hidden toggle,
     the separators between groups, `Mounts.trashable`, the step that skips separators and greyed
     rows, and `Menu.clamp`, all unit tested. `menu.hidden` is read off `flea --ui-state` with the
     other preferences and follows a save from the Menus section. `menushell.rs` opens it on a
     right click, `m`, the Menu key and Shift+F10, with `Tap.tappedMenu`'s selection rule, takes
     every key while it is up as ContextMenu's keyCatcher does, and refuses a choice whose
     directory or target names changed since it opened. An open menu is one of the things that
     owes the watched re-read rather than running it, which a mutation of that rule turns red.
     Ported rows: Open, New Folder, Cut, Copy, Paste, Duplicate (the `duplicate` request, new to
     this crate), Rename, Select all, Move to Trash, Open in terminal (`flea --terminal`), Copy path
     (`wl-copy`, which outlives the window, and Ctrl+B with it), Add to Favorites, Sort by and its
     flyout (Sort.column, as Focus.act routes `sort:`), Show hidden files and Settings. **Two rules
     for what is not here yet**, the QML's own two for a missing capability: a row whose capability
     is probed (Compress, Extract, Convert, Run script, the shelf and the three send providers) is
     absent, because nothing has probed it; a row whose dialog is not ported (Open with, New File,
     Delete permanently, Move to, Copy to, Properties, Permissions) is offered and says it is not in
     this build yet. **Deliberate deviation:** a right click opens the menu under the clicked row,
     or under the last row over empty space, rather than at the pointer, because iced reports no
     position with the press and tracking the pointer would rebuild the view on every motion.
     `tests/iced-headless.sh` drives it: Right swallowed while it is up, Escape closing it,
     Duplicate, the hidden row both ways and Ctrl+B, 33 checks in all.
   - **8b-i, the dialog cards (done, driven headless and against the real backend).**
     `actioncard.rs` ports ui/MenuActionDialog.qml, ui/TrashConfirm.qml and ui/PermissionsDialog.qml
     as one state machine: Properties, New File, Move to, Copy to, Delete permanently and
     Permissions. Every row but New File and Permissions first sends `menuaction snapshot` naming
     the menu's rows and the cursor, and goes on only on its answer; Move to and Copy to validate the
     destination and then send `transfer` with the `menuId`, so the captured sources move, before
     the close that expires them; Delete permanently reviews, opens the destructive strip with
     Cancel focused, checks the set every 2.5 s as TrashConfirm does, reviews again on a changed set
     or a stale delete, and only Tab off Cancel and Enter sends the token; Permissions inspects the
     path, edits the octal mode or flips a bit, and applies. A created file and an applying mode
     hold the card until they answer, and a running deletion holds the slot for the next card.
     Ctrl+M opens New File. `card_live_tests.rs` drives every card through the real `flea --backend`
     with FLEA_BIN set, reading the disk after each, and a mutation of the transfer's `menuId` turns
     it red; `tests/iced-headless.sh` seeds an empty `menu.hidden` so every row is reachable and
     drives Properties, Ctrl+M and both ways out of the confirmation, 37 checks. **Deliberate
     deviations:** the check boxes are pointer-only, the octal field being the keyboard's way to
     the same bits, because iced's check box takes no focus; the survivors of a partial permanent
     deletion are not re-selected through the batched `locate` this crate does not send yet; and a
     re-review that finds nothing left says so and closes rather than asking to delete 0 items.
   - **8b-ii, the job rows (done, driven headless and against the real backend).** The menu now
     reads its probes when it opens, the backend's `formats` answer (asked with the first list and
     again on every menu) and `~/.config/flea/scripts` (`scripts.rs`, ui/Scripts.qml's `find`), and
     offers Compress with the formats bsdtar writes, Extract on a row `archives.rs` names an
     archive, Convert on one image row, and Run script with one row per script, each absent when its
     probe finds nothing, as in QML. Compress, Extract, Convert and Run script go through
     `activation.rs`, PaneMenuActions' snapshot and `activate`, which answers the captured paths,
     and the archive and convert requests carry its id as their `menuId`. `jobshell.rs` holds the
     archive job's status line with its progress, the done sentences of ui/js/Ops.js and the
     convert answers. `convertcard.rs` ports ConvertDialog.qml and ui/js/Convert.js: the formats,
     the destination name that never overwrites its source, and the `convert` check before anything
     is written. `openwith.rs` ports OpenWithDialog.qml and ui/js/OpenWith.js: the registry first
     with the default marked, every installed application after it, the search over both, and the
     "always" box. `card_live_tests.rs` drives compress, extract, convert and the catalogue through
     the real backend (nothing is launched from a test), and a mutation of the `activate` op turns
     it red; `tests/iced-headless.sh` gives Run script a config home of its own and drives
     Compress, Extract, Run script and an Open with that is cancelled, 41 checks.
     **Deliberate deviations:** Open with has no flyout of registered applications on the menu
     row itself; the row opens the card, which lists those same applications first. The Open with
     search is not a text field: the card reads the key's own text, which is the QML card's rule 8
     ("typing anywhere goes to the search") and keeps Space and Enter meaning the focused control,
     at the cost of pasting into the search. A script runs without a timeout, as ui/Scripts.qml's
     `Process` does, but on a worker thread, so a script that never exits ties up one thread and not
     the window.
   - **8c, the send rows (done; the services that leave the box are unit tested, not driven).**
     `providers.rs` ports ui/js/Taildrop.js, ui/js/Dropbox.js and ui/js/LocalSend.js: the providers
     block of the `formats` answer, the Dropbox account folder and the status whose exit code lies,
     `tailscale status --json`'s online Taildrop targets without Mullvad relays, LocalSend's device
     ids that pin the address, and every sentence. `sendshell.rs` asks every service again when a
     file menu opens, as ContextMenu's onSnapshotRequested does: the providers, `tailscale status`
     under a 15 s bound, `dropbox-cli status` under 4 s, and LocalSend's devices when the list is
     older than 15 s and the row is not hidden (discovery announces the box). The rows grey while
     they answer and are rebuilt as each one does, with `menufacts.rs` keeping the cursor on its
     action. A row refused by its service is drawn red. `brands.rs` draws the Tailscale, Dropbox,
     LocalSend and Flea marks as TailscaleMark.qml and the others do. Add to shelf runs `flea shelf
     add`, the shelf row following `shelf.enabled`; Taildrop runs the OEM send script detached;
     LocalSend asks the backend; Move to Dropbox and Copy Share Link capture the account folder
     with `providerDestination` between the snapshot and the activation, and the move names rows
     and the `menuId`. `card_live_tests.rs` drives Move to Dropbox through the real backend into a
     stand-in account folder, a mutation that skips the capture turns it red, and the refusal of a
     folder that is not a directory is asserted by its sentence; `tests/iced-headless.sh` drives
     Add to shelf into the sandbox's own pile, 42 checks. **Not driven, deliberately:** Taildrop is
     not installed here, LocalSend's discovery and send announce the box on the LAN, and a share
     link makes a real one on the account, so those three are covered by the parsers' and the
     menu's unit tests. The headless run hides their rows, and sets `shelf.enabled` in the file
     itself, because the patch path runs `omarchy plugin enable` on the operator's real bar.
     **Deliberate deviation:** QML asks the services a second time just before validating a chosen
     Taildrop, Dropbox or share link row; this asks once, when the menu opens, and the backend's
     own destination check and the send script's own report stand behind that.
   - **8d-i, the chrome's buttons, history, the filter and the key sheet (done; the buttons are
     pointer-only and undriven).** The chrome draws ChromeBar's Back and Up beside the crumbs, and
     Search, the four view modes and Settings at its right; Back follows `nav.rs`'s history, which
     ports Nav.js's back and forward stacks, and the mouse's back button follows history or climbs
     (Nav.mouseBack), refused behind an open menu, card or panel. List and the split are the two
     modes this build draws; columns and grid say so until 8d-ii. `filter.rs` ports ui/js/Filter.js:
     a query narrows the rows the pane holds in their own order, the cursor and every scroll move in
     view positions, the selection loses what the filter hides, a shift range and Select all take only
     what it shows, thumbnails are asked for the rows it draws, Escape takes it off first, a search
     or any new listing forgets it and a watched re-read keeps it, and a typing filter owes the
     watch. The status bar carries its query line and summary, and the no-match tile names the query
     back. **The filter is not reachable in this build, faithfully:** ChromeBar draws its button in
     the grid view alone and no key opens it, so it arrives with the grid; the unit tests drive it
     whole. While a filter stands the held window does not move, which is GridArea.requestIfDrifted's
     own rule (`pane.shown !== null`), and the grid is the only view that offers one. `keysheet.rs`
     is KeymapSheet.qml over sheetFor("default", "gui")'s own rows, read by running that function,
     with a test that checks the groups, their order and every base label against ui/js/Keymap.js;
     `?` opens it and any key closes it without reaching the listing. 44 headless checks.
   - **8d-ii-a, the grid (done; its Filter and Sort buttons are pointer-only and undriven).**
     `grid.rs` is GridArea.qml's geometry and ui/js/Grid.js's arrows: as many tiles across as the
     wider of Style.space(125), 146 px here, and the thumbnail with its padding allows; a tile row
     stands in for a text row in the viewport, the scroll and the held-window drift; every arrow
     needs a real cell to land on, so a step off the grid or across a tile row's end is taken and goes
     nowhere, and Left and Right are not the tree's in the grid. `gridview.rs` draws GridTile: the
     thumbnail or mark at `preview.thumbSize`'s pixels, decoded at that size as Image.sourceSize is,
     two centred caption lines, the cursor's accent fill and edge, and the rename field in the
     caption's place. The chrome trades Search for Filter and Sort (Sort.next) in the grid, both
     standing down during a search; choosing a mode is the view key, saved alone, a mode chosen
     while split leaves it because the split's panes are list-only, and leaving the grid closes its
     filter. A menu opens under its tile, and under a filter every menu opens under the row as drawn.
     `tests/iced-headless.sh` launches with `view: "grid"` and drives Right and Down, asking the
     backend for the listing's order rather than deriving it, 46 checks.
   - **8d-ii-b-1, the columns view (done; its neighbour columns are pointer-only and undriven).**
     `columns.rs` holds ColumnsArea's peek cache: the parent and the cursor row's folder asked with
     `peek` after every input, each once, and the whole cache dropped when the pane moves or its
     hidden flag changes; a peek this view did not ask for is the path bar's and is left to it.
     `columnsview.rs` draws ColumnPane and ColumnRow: a third for the parent, its row for the folder
     on screen lifted as the trail and the rest muted, a third for the pane's own listing, which is
     the scrollable whose viewport thumbnails and the held window follow and the one column with the
     accent, a size and the rename field, and the rest for the cursor's folder. A plain tap on a
     folder in the middle column enters it (Tap.tappedMiddle, the 2026-09-11 ruling); a neighbour's
     folder reveals on one tap and its file opens on two (Tap.tappedColumn). **A right click on a
     neighbour's row follows the code over keys.toml's pointer table:** the table says a peeked row
     has no menu, while ColumnPane routes it to menuOnNeighbour, which lists that folder and raises
     the menu on the row once it lands, and that is what this does. A folder the peek could not read
     draws nothing, because StateMessage there carries no message and the empty tile stands down
     for a locked folder. The menu opens under its row in the middle third. Peeks are driven
     headless from a window launched with `view: "columns"`, 48 checks.
   - **8d-ii-b-2, the preview column (done; PDF and media in it are 8d-ii-b-3).** `facts.rs` is
     ui/js/Facts.js with the half of ui/js/Kinds.js it reads: the twelve states in Facts.state's
     order, each state's rows with the canvas's labels verbatim and a label with no value left out,
     and the selection's summary, whose numbers become floors when the selection is wider than the
     held window. `colpreview.rs` is ui/SelectionPreview.qml: the cursor row loads 120 ms after the
     cursor settles, with a `meta` carrying the column's own token and the three hints its state
     names (no line count for a sheet), and its thumbnail when the pane has not asked; the directory,
     the cursor, the selection or the row's identity moving clears it, and only the newest token's
     answer lands, while the overlay's archive index keeps token 0. `columnpreview.rs` is
     ui/PreviewColumn.qml: the 16 by 10 frame holds the thumbnail decoded at its own 256 px, the first
     fourteen lines of a text file under a mebibyte (a muted gutter for code), the first records of a
     table or sheet, an archive's names and its "+ N more", a stack of up to three marks for a
     selection, or the kind's mark with the one line saying why it stands in; Error and Unsupported
     draw their own mark and sentence. The wire's `meta` reply is typed whole for it. `preview.column`
     and `preview.loadOn` are read and applied live. **Deviations:** the loading state is the word
     "reading" after the 150 ms hold-off with no animated mark, and code has its gutter but not
     ui/js/Syntax.js's colours. **Manual loading** waits for `loadPreview`, which no key in the
     Default preset binds, so with `loadOn: manual` the column says "no preview", in QML and here
     alike. A selection's Newest and Oldest skip rows with no mtime, where Facts.js let a zero reset
     the oldest. Driven headless: a settled text row asks for its meta with a line count and draws
     its first line and four facts, 49 checks.
   - **8d-ii-b-3, the preview column's PDF page and media strip (done).** `columnmedia.rs` is
     PreviewColumn.qml's pdfLoader and playerLoader. A PDF row renders its page on the poppler worker
     into the frame, centred and fitted, scrollable both ways once zoomed, again when the column is
     resized; under the frame the toolbar turns pages (each end greyed), zooms in quarter steps from
     1 to 4, and maximize opens the overlay on that file at the column's page and zoom, filling the
     window (SelectionPreview.onExpandRequested). The page count is the Pages fact, and a document
     poppler refuses is the Error tile's "This file could not be read.". A video or audio row draws
     MediaStrip under the frame: play, a hairline seek with a square accent handle that seeks once on
     release, the clock and the session's one mute flag shared with the overlay. **No player exists
     until play is pressed**, so browsing clips builds nothing, and it is dropped the moment the
     column leaves that file, the cursor leaves the column's subject, or the overlay opens; a video
     plays into the frame over its thumbnail. The duration is the probe's until the player knows its
     own. Driven headless through `examples/test-pointer.rs`, a zwlr_virtual_pointer_v1 click inside
     the private sway (which has no pointer device, and ydotool would click on the desktop instead):
     the PDF draws page 1 of 2 with Pages 2, next turns to 2 / 2, maximize opens the overlay there,
     and GStreamer's playbin is not loaded by a PDF or by moving onto a clip and is loaded by a
     press of play, 52 checks. **Not yet:** the column's own keys, which only focusPreview reaches and the
     Default preset binds nothing to, so the column is pointer-only here as it is in QML. **Seen in
     passing, not this stage's:** the overlay draws a page that fits its frame at the left rather
     than centred.
   - **8d-ii-b-4, syntax colours (done).** `syntaxlang.rs` is ui/js/SyntaxLanguages.js's tables,
     `syntax.rs` ui/js/Syntax.js's lexer and `syntaxmd.rs` ui/js/SyntaxMarkdown.js, its regular
     expressions spelled out by hand beside their patterns; `syntaxview.rs` is ui/js/SyntaxStyle.js,
     PyCharm's Monokai lifted to 4.5:1 on its ground (comments 3:1), keywords and parameters italic,
     headings and strong text bold. Where the JavaScript built StyledText and escaped every byte,
     this answers byte ranges and roles, so nothing from a file is ever markup. The overlay's text
     editor is coloured through iced's Highlighter, which keeps each line's starting state so a
     changed line restarts from the one above, on the overlay's surface and up to `MAX_BYTES`; the
     column's first lines are rich text on the frame's ground with tabs expanded to their stops, an
     open comment carried down as Syntax.lines does. tests/js/syntax.js is ported case for case,
     with the escaping and hard-space cases dropped because there is nothing to escape, and the
     headless suite screenshots a Rust file in both. iced's `advanced` feature is on for the
     Highlighter trait; it adds no crate. **Deviation:** the overlay colours a line at a time where
     the JavaScript coloured 4 KB runs, and a run carried a parameter list across its own line
     breaks, so a list's later lines draw their names plain here; a comment or string still carries.
   - **8d-iii-a, the Trash view (done; the live count and the sweep are 8d-iii-b).** `trash.rs` and
     `trashreceive.rs` are ui/TrashView.qml: the backend's one `trashbrowse` session listed a window
     at a time (with recovery on open), a selection held by URI and the identity each row was listed
     with, select-all as the backend's own snapshot with rows taken out of it, restore, and permanent
     deletion only through a prepared confirmation; one request in flight, and a reply taken only
     when it answers it. A Trash that changes under an open confirmation is prepared again over the
     same ask with its token, and a stale delete is reviewed again rather than failed; a finished
     operation leaves the items that failed selected. `trashview.rs` draws the view over the focused
     pane (its strip with Back, the greyed Up, the count and size, and Empty Trash; Name, Original
     location, Deleted; rows drawn only for the held window) and TrashConfirm, whose Cancel holds the
     keyboard until Tab or Right reaches the destructive button; the open card is checked every
     2.5 s. `trashshell.rs` holds its keys (every key while it is up, so nothing reaches the
     listing: the arrows, Space, Ctrl+A, Delete, F5, `m`, Tab to the rail, Escape and Backspace to
     close) and its two menus, Restore / Delete permanently over a selection and Open / Restore all /
     Empty Trash otherwise, the second also on the rail's Trash row, read on a fresh
     `gio info trash::item-count`. The rail's Trash row opens the view. `Message` moved out of
     `main.rs` into `message.rs` to make room. **Not ported, because nothing binds them since the key
     cut:** the dd pair and the shift-arrow extend. **Deviation:** the status bar here has one line,
     so a failure's detail rides after its headline rather than in QML's expandable queue. Driven
     headless on a private session bus whose gvfsd serves the sandbox's own Trash (the session bus's
     is the operator's), and only when that private listing holds exactly the suite's own file:
     Delete trashes the cursor row, the rail's row opens the view, Enter on the card's Cancel deletes
     nothing, the destructive button deletes from disk, and Restore from the menu puts a file back,
     57 checks.
   - **8d-iii-b, the Trash count's monitor, the badge and the 30-day sweep (done).** `trashwatch.rs`
     is ui/TrashMonitor.qml: `gio monitor --dir=trash:///` while the Trash row is shown or the view is
     up, tied to the window by setpriv's parent-death signal and a parent check as TrashMonitor's is,
     and killed when the subscription drops; each change settles 120 ms, then the count is read again
     with `gio info` (LC_ALL=C, bounded at 10 s) and an open view re-lists or re-prepares. The rail's
     Trash row draws the count when `places.trashCount` is on; a failed count or monitor is said only
     where it shows, the view or the badge. `trashsweep.rs` is TrashHost's sweep and TrashDates'
     expired and dayNumber: opt in through `trashAutoEmpty`, once a day by `trashSweptOn`, two seconds
     after the window opens, never once the view was used, and stopped by the view reaching for the
     Trash; it lists, prepares only what is past thirty days by its own unreadable-is-never-expired
     rule, deletes through the reviewed token, and records the day only when it ran to its end. Its
     replies are told from the view's by the sweep being in flight, as TrashHost does. Driven headless
     on the private bus: the badge after a Delete, and with one item backdated to 2020 and one fresh,
     the sweep deletes the old, keeps the fresh, writes today's day, and a second launch that day
     sweeps nothing; switched on only when the private Trash holds exactly those two, 60 checks.
   - **8d-iv-a, tabs and the Places menu (done; their strip is pointer-only, as in QML).** `tabs.rs`
     is ui/js/Tabs.js on the pane, so each pane of the split has its own: hidden tabs are snapshots of
     the path, history, cursor, view, dotfile answer, selection and order, and the one live listing
     is the current tab's. A new tab lands where Settings > View > Opening says, or on the Places
     row's own path; a switch to the same folder re-lists nothing and brings the cursor and the
     selection back, but the selection only when no listing ran since (issue 94's counter, which the
     pane now keeps); a switch to another folder lists it with the tab's own order and dotfile answer
     and lands on its index, through an anchor with no name, which is pendingCursor exactly. The
     search and the filter go with any switch, a walk's scope is the folder a tab records, and nine
     is the cap; every refusal is QML's sentence. `tabbar.rs` draws TabBar under the chrome across
     the window only while there are two: the current tab on the window's ground with the accent
     edge, a close mark whose target never shrinks, a middle press to close, and + for a new tab;
     a tab restores its own view outside the split. `placemenu.rs` and `menu.rs`'s `place_entries`
     are PlaceMenu: on Home and a favourite, Open, Open in new tab, Open in terminal, Copy path, and
     Add to or Remove from Favorites, each on the row's own path, and with the Extras row off a
     favourite keeps its one Remove; a remove names the expected records and refuses when the row
     moved. **No key opens a tab, and the default hidden set switches the Places menu off**, which
     is the key cut's and the shipped defaults' doing in QML too, so a tab is reached by turning
     Settings > Menus > Extras > Places row menu on. Driven headless: a favourite's menu opens its
     folder in a new tab, a press on the first tab lists it again, closing the other tab lists
     nothing, and Remove drops the favourite, 64 checks.
   - **8d-iv-b-1, density and the list's columns (done; Items is 8d-iv-b-2).** `listcols.rs` is
     ui/js/Columns.js and Theme.fileRowHeight: Mode, Size, Modified and Kind each drop at the width
     where keeping it would push the name under its twenty-character floor, Kind first and Mode
     last, the header and every row resolving one set from one width; the split's panes carry Size
     and Modified only, a search keeps Size alone, and the stored `columns` is what shows. A density
     stop scales the row padding, 31, 37 or 45 px, for the list, all three columns of the columns
     view and the Trash, and every viewport sum, scroll and menu placement built on them. The header
     reads Name, Mode, Size, Modified and Kind as Header.qml does (Mode sorts nothing), and its right
     click is Menu.headerEntries, Hide or Show per column then the dotfile toggle, saving the whole
     shown set. `examples/test-pointer.rs` takes a fifth argument for the right button. Driven
     headless at compact density: the header's Show Mode stores `["name","size","date","mode"]`.
   - **8d-iv-b-2, folder sizes and the Items column (done).** `dirsizes.rs` is ui/js/DirSizes.js and
     List.qml's requestDirSizes: the list view asks `dirsize` for its visible folder rows on the
     thumbnails' own settle, once a row, and only while `folderSizes` is on or Items is drawn; a
     scroll that moved the visible range sends one `dirsizecancel` when anything still waits; a
     `dirsized` for a listing still in flight is dropped, and the map is capped as the thumbnails'
     is. The `changed` line's `sizes` is read now: a re-read of the same folder keeps the answers
     unless a structural burst came first, Nav.js's rule. The Size cell draws a folder's walked total
     (a floor marked `>`) or `·`, and Items, which drops between Size and Modified by width and is
     never in the split's panes or a search, draws the folder's own child count or `·`; its head
     sorts nothing and the header menu gained its row. `join` and `parent` moved to `paths.rs` to
     make room in `pane.rs`. **Not done:** a filter change does not cancel the walk's queue as
     List.qml's does; a waiting row answers later instead, which costs nothing it would not.
     Driven headless: the list asks the walk, and Show Items stores the column, 67 checks.
   - **8d-iv-c, text size (done).** `display.textSize` is applied as iced's own window scale: the
     stored stop, landed on the nearest of Omarchy's seven, or Omarchy's `[font] base-size` while
     following it, over the 14 px base every metric in `metrics.rs` was taken at
     (`listcols::ui_scale`). Type, spacing, rows and marks scale together, which is what deriving
     every token from the size in force does in Theme.qml, and the monitor's scale stays the
     compositor's, on top; on this box Omarchy's base is 14, so following it draws exactly as
     before. **Deviations:** one ratio scales everything, where Theme.qml rounds each token on its
     own ladder, so a size can sit a pixel off QML's; Omarchy's base is read once at launch where
     Style.qml follows a theme change live; and the chooser window keeps its own fixed size. A
     headless launch at 20 is looked at, 68 checks.

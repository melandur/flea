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
4. Rail: places, devices, network, the share browser.
5. Preview: text, image (through `--preview-image`), archive, table, PDF, media.
6. Settings panel, theme catalog, tabs, split, search, filter, path bar.
7. The picker for `tools/flea-portal`, then `flea --gui` switches to the iced binary, after a
   field-bench run says the switch does not lose a column Flea wins today.

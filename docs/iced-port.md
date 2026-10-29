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
2. Selection, clipboard, trash, rename, new folder, undo, transfers and their status line.
3. Icons, the filetype tier and thumbnails, with the settle timer and the no-sweep tests.
4. Rail: places, devices, network, the share browser.
5. Preview: text, image (through `--preview-image`), archive, table, PDF, media.
6. Settings panel, theme catalog, tabs, split, search, filter, path bar.
7. The picker for `tools/flea-portal`, then `flea --gui` switches to the iced binary, after a
   field-bench run says the switch does not lose a column Flea wins today.

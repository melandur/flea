<p align="center">
  <img src="docs/images/icon.svg" width="72" height="72" alt="Flea">
</p>

<h1 align="center">Flea</h1>

<p align="center">
  <strong>The fastest GUI file manager on Linux.</strong><br>
  Keyboard-first. Built for Omarchy.
</p>

<p align="center">
  <a href="https://github.com/tcballard/omarchy-badges"><img src="https://raw.githubusercontent.com/tcballard/omarchy-badges/85f859029e236e784e7b05ada6dbe73506d07a91/badges/v1/built-for-omarchy.svg" alt="Built for Omarchy"></a>
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#views">Views</a> ·
  <a href="#performance">Performance</a> ·
  <a href="#keyboard">Keyboard</a> ·
  <a href="https://github.com/thisisgm/flea/releases">Releases</a>
</p>

<p align="center">
  <img src="docs/images/release-0.3.0-themes.gif" alt="Flea 0.3.0 cycling through ten Omarchy themes, with the iPhone on the Devices rail and a photo in the preview column">
</p>

Flea combines a Rust backend with a Quickshell interface that follows your Omarchy theme.
Large directories stay responsive: the window loads rows and requests thumbnails as you need them.

- **Three views.** List, columns and grid, with tabs and natural filename sorting.
- **Quick Look.** Press Space for images, PDFs, text, media and archive contents.
- **File operations.** Copy, move, rename, trash, compress and extract, with an undo journal.
- **Network and sharing.** SMB, SFTP, FTPS, WebDAV, NFS, Dropbox and Taildrop.
- **Devices.** USB drives, phones and cameras over MTP, PTP and AFC, mounted from the rail.
- **Desktop integration.** Default file manager, “Show in folder” and Open/Save dialogs.
- **Your settings.** Omarchy text sizes and configurable menus.
- **Marks and palettes.** Per-filetype marks and colours, and 95 bundled palettes beside your theme.

An iPhone lists once it has been unlocked and trusted on this machine: AFC needs the pairing record
that leaves behind, not an unlocked screen every time. Pairing also puts the phone on its tethering
USB configuration, so iOS shows Personal Hotspot as active while it is plugged in, and Flea never
uses it. If this machine should never route through the phone, tell NetworkManager so with a
`conf.d` drop-in carrying `unmanaged-devices=driver:ipheth`; that is your call, not the package's.

## Install

```bash
omarchy pkg add flea
```

Make Flea your default file manager, file chooser and the app opened by **Super+Shift+F**:

```bash
flea --default
systemctl --user restart xdg-desktop-portal
```

Update through Omarchy:

```bash
omarchy update
```

<details>
<summary>File chooser only, development package and removal</summary>

Use Flea for portal Open/Save dialogs without changing your default file manager:

```bash
flea --picker
systemctl --user restart xdg-desktop-portal
```

For the rolling development build:

```bash
omarchy pkg aur add flea-git
```

Before removing Flea, undo its desktop integration:

```bash
flea --default off
omarchy pkg drop flea
```

If you previously pinned another directory handler, restore that handler explicitly.
[Installation details](docs/install.md) cover dependencies, desktop integration and removal.

</details>

## Views

**List** for file details. **Grid** for photos and clips. **Columns** for browsing folders with a preview beside them.

<p align="center">
  <img src="docs/images/grid.png" width="49%" alt="Grid view with image and video thumbnails">
  <img src="docs/images/list.png" width="49%" alt="Columns view with the preview column, file details and the iPhone on the Devices rail">
</p>

Turn on **File type marks and colours** in Settings → Display and a row's mark and ink come from its
name rather than from its MIME class, so `.json`, `.toml` and `.md` stop sharing one document mark.
The marks are Flea's own lucide set; the colours are yazi's table, which is
[nvim-web-devicons'](https://github.com/nvim-tree/nvim-web-devicons). Settings → **Theme** carries 95
bundled palettes from [Strata](https://github.com/lgse/strata), with following your Omarchy theme
the default and what a fresh install does. See [`vendor/`](vendor/README.md) for what is imported
and under which licence.

**Space** opens Quick Look from any view, and so does **→**. Every kind behaves the same way in
it: **↑** and **↓** move to the next file with the preview following, **Space** fills the window and
**Space** again brings the inset surface back, and **←** leaves. Once it fills the window the arrows
move the content instead: a PDF's pages, a media file's playhead, a text file's or an archive
listing's own scroll. Page through a PDF, play media or inspect an archive.

<p align="center">
  <img src="docs/images/pdf.png" alt="PDF preview with page navigation">
</p>

**The shelf** lives in the Omarchy bar: send files there from any menu, drag them out into any app, pin the ones you keep coming back to. **Phones** are Devices rows over MTP, AFC and PTP.

<p align="center">
  <img src="docs/images/shelf.png" width="49%" alt="The shelf card open over Flea, with thumbnails, pins and the last three screenshots">
  <img src="docs/images/iphone.png" width="49%" alt="An iPhone camera roll browsed over AFC, with HEIC thumbnails and a preview">
</p>

## Performance

Measured on **8 September 2026**, using the source tree released as **0.1.6**.
Cold caches, three runs per app, medians below. Seven GUI apps on the same Omarchy machine.

### 100,000 files

Flea led this run in settled time, window memory and process-tree CPU.
PCManFM mapped its window sooner.

| File manager | First window | Settled | Window PSS | CPU |
|---|---:|---:|---:|---:|
| **Flea** | 749 ms | 1.21 s | 88.0 MiB | 0.89 s |
| Strata | 632 ms | 1.67 s | 93.9 MiB | 1.39 s |
| Dolphin | 699 ms | 5.43 s | 211.2 MiB | 8.97 s |
| Nemo | 780 ms | 22.23 s | 459.6 MiB | 25.34 s |
| Thunar | 534 ms | 22.72 s | 346.0 MiB | 22.33 s |
| PCManFM | 415 ms | 35.76 s | 109.9 MiB | 24.77 s |
| Nautilus | 798 ms | 79.83 s | 335.1 MiB | 18.61 s |

### 2,000 mixed files

The apps produced different amounts of thumbnail work, so settled time and CPU are
**not ranked**. These are observations, not an equal-work speed comparison.

| File manager | First window | Settled | Window PSS | CPU | Thumbnails |
|---|---:|---:|---:|---:|---:|
| **Flea** | 794 ms | 2.49 s | 102.9 MiB | 1.23 s | 36 |
| Dolphin | 682 ms | 14.28 s | 98.8 MiB | 68.18 s | 552 |
| Nautilus | 857 ms | 16.95 s | 172.4 MiB | 12.54 s | 541 |
| Nemo | 788 ms | 3.36 s | 53.7 MiB | 5.25 s | 60 |
| PCManFM | 389 ms | Timed out | 38.9 MiB | 90.19 s | 604 |
| Strata | 610 ms | 7.83 s | 87.2 MiB | 2.31 s | 205 |
| Thunar | 534 ms | 12.88 s | 41.1 MiB | 7.22 s | 221 |

“First window” records compositor registration. “Settled” means 500 ms without process-tree
CPU activity; neither measures input readiness.
PSS covers the window process, excluding helpers and GPU memory. Flea's separate backend used a median **5.3 MiB** on the large
listing and **2.1 MiB** on media. CPU includes the process tree; shared libraries affect PSS.
PCManFM did not settle in any media run. Results describe this machine and workload.

[Method](docs/benchmarks.md) · [Run details and TUI results](docs/bench/results-0.1.6.md) ·
[Scale CSV](docs/bench/f016c-scale.csv) · [Media CSV](docs/bench/f016c-media.csv)

## Keyboard

Press **?** for the full keymap, or **,** to change settings.

| Action | Keys |
|---|---|
| Move / parent / browse in | `↓` `↑` / `←` / `→` |
| Open with the default app | `Enter` |
| Quick Look / fill the window / leave it | `Space` or `→` / `Space` / `←` or `Esc` |
| Copy / cut / paste | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` |
| Copy the file's path | `Ctrl+B` |
| Rename / trash / undo | `F2` / `Delete` / `Ctrl+Z` |
| New folder / new file | `Ctrl+N` / `Ctrl+M` |
| Show or hide hidden files | `Ctrl+H` |
| Search | `Ctrl+F` |
| Show or hide the sidebar | `Ctrl+G` |
| Split the window in two, or join it | `Ctrl+T` |
| Context menu | `m`, `Menu` or `Shift+F10` |
| Settings / keymap sheet | `,` / `?` |
| Move focus | `Tab` |

That is the whole keyboard: the four arrows, the Ctrl chords and a handful of named keys. Nothing
else is bound, and there is one preset rather than four. Everything the old bare letters reached —
the views, tabs, sort, filter, the path bar, hidden files, the terminal, text size, select all —
is on the context menu, the chrome and the settings panel instead.
See [the full key table](keys.toml) for every binding and the pointer actions.

## Build

Requires Omarchy, Quickshell 0.3.1 or newer, and Rust. See
[installation details](docs/install.md) for runtime and optional dependencies.

```bash
cargo build --release
FLEA_UI="$PWD/ui" ./target/release/flea --gui
```

Pass a directory to open it, or `--select <path>` to reveal a file. The terminal interface
is not implemented yet.

```bash
./tests/run-all.sh
```

The headless runner builds both profiles and reports suites that need a display, credentials
or a package. [The test suites](tests/) cover file operations, previews, persistence and input;
[the protocol guide](docs/protocol.md) documents the backend.

## Support

If this saved you an afternoon, you can
[sponsor me on GitHub](https://github.com/sponsors/thisisgm) or
[buy me a coffee](https://buymeacoffee.com/thisisgm).

## Licence

[MIT](LICENSE). Flea also redistributes three third-party works, each under its own licence and
each recorded in [`vendor/`](vendor/README.md): the filetype colour table (nvim-web-devicons, MIT,
by way of [yazi](https://github.com/sxyazi/yazi)), the palette catalog
([Strata](https://github.com/lgse/strata), MIT) and the mark geometry
([lucide](https://github.com/lucide-icons/lucide), ISC).

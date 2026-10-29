// The window's first state: the saved settings, the start path, the backend child and the rail.
use crate::backend::{self, Backend};
use crate::pane::{self, Pane};
use crate::theme::Palette;
use crate::wire::Order;
use crate::{devices, prefs, rail, railshell, Flea, Message, ROW_H};
use iced::{keyboard, Task};
use std::collections::{HashMap, VecDeque};

pub(crate) fn boot() -> (Flea, Task<Message>) {
    let bin = backend::binary();
    let prefs = prefs::Prefs::load(&bin);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let argv = std::env::args().nth(1);
    let start = prefs.start_path(&home, argv.as_deref());
    let order = Order {
        by: prefs.sort_by.clone(),
        desc: prefs.sort_desc,
        folders_first: prefs.folders_first,
        group_by_kind: prefs.group_by_kind,
    };
    // 36 rows is the tiled height measured on this box; the first viewport report corrects it.
    let pane = Pane::new(prefs.hidden, 36)
        .with_order(order, prefs.wrap_at_ends)
        .with_view(&prefs.thumbnails, prefs.file_type_colours);
    let mut flea = Flea {
        pane,
        backend: None,
        palette: Palette::load(&prefs.theme_id),
        message: None,
        message_gen: 0,
        modifiers: keyboard::Modifiers::default(),
        last_tap: None,
        opening: false,
        quitting: false,
        viewport_h: 36.0 * ROW_H,
        scroll_y: 0.0,
        decoded: HashMap::new(),
        decoded_order: VecDeque::new(),
        rail: railshell::Rail {
            shown: prefs.rail_shown,
            width: rail::width(prefs.sidebar_width),
            user_dirs: std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".config"))
                .join("user-dirs.dirs")
                .to_str()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default(),
            home: home.clone(),
            favourites: prefs.favourites.clone(),
            show_trash: prefs.show_trash,
            show_devices: prefs.show_devices,
            show_network: prefs.show_network,
            ..Default::default()
        },
        devices: devices::Devices::new(
            std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default(),
            prefs.show_unmounted,
        ),
        network: crate::network::Network::new(prefs.remember_passwords),
        share_browser: None,
        dialog: None,
        dialog_memory: None,
        preview: Default::default(),
        surfaces: Default::default(),
        table_scroll: 0.0,
        window_w: 1280.0,
        prefs: prefs.clone(),
        pdf: None,
        playback: Default::default(),
        chrome: Default::default(),
        panel: Default::default(),
        menu: None,
        opening_terminal: false,
        card: None,
        card_ids: 0,
        jobs: Default::default(),
        sends: Default::default(),
        sheet_open: false,
        columns: Default::default(),
        pending_menu: None,
        colprev: Default::default(),
        trash: Default::default(),
        backend_key: 0,
        side: 0,
        other: None,
        spawns: 0,
        // A grid or columns view this port does not draw yet still comes back on leaving the split.
        dual_from: if prefs.view == "dual" { prefs.dual_from.clone() } else { prefs.view.clone() },
        quit_keys: vec![],
    };
    flea.rebuild_rail();
    // The device poll starts at once and re-arms itself, the Timer's triggeredOnStart in QML.
    let poll = if flea.rail.show_devices { flea.devices.tick() } else { vec![] };
    let polling = flea.run_devices(poll);
    let net = if flea.rail.show_network { flea.net_tick() } else { Task::none() };
    let polling = Task::batch([polling, net]);
    flea.backend_key = flea.spawn_key();
    let task = match Backend::spawn(flea.backend_key) {
        Ok(b) => {
            flea.backend = Some(b);
            let effects = flea.pane.open(start.clone(), None);
            // The menu's archive and convert rows read this probe, so it goes out with the first list.
            let first = Task::batch([flea.run(effects), flea.ask_formats()]);
            // A window left split opens both panes on the start path (the 2026-10-08 ruling), the
            // keyboard on the pane that held it.
            if prefs.view == "dual" {
                let second = flea.open_second(start, false);
                if prefs.dual_focus == 1 {
                    flea.swap_other();
                }
                Task::batch([first, second])
            } else {
                first
            }
        }
        Err(e) => {
            flea.pane.error = Some(pane::sentence("backend", &e, ""));
            Task::none()
        }
    };
    // The 30 day sweep runs off the startup path, two seconds in, as shell.qml's trashSweep does.
    let sweep = Task::perform(crate::sleep(std::time::Duration::from_secs(2)), |_| Message::Trash(crate::trashview::Msg::SweepDue));
    let count = flea.refresh_trash_count(false);
    (flea, Task::batch([task, polling, sweep, count]))
}

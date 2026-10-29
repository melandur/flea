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
        palette: Palette::load(),
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
    };
    flea.rebuild_rail();
    // The device poll starts at once and re-arms itself, the Timer's triggeredOnStart in QML.
    let poll = if flea.rail.show_devices { flea.devices.tick() } else { vec![] };
    let polling = flea.run_devices(poll);
    let net = if flea.rail.show_network { flea.net_tick() } else { Task::none() };
    let polling = Task::batch([polling, net]);
    let task = match Backend::spawn() {
        Ok(b) => {
            flea.backend = Some(b);
            let effects = flea.pane.open(start, None);
            flea.run(effects)
        }
        Err(e) => {
            flea.pane.error = Some(pane::sentence("backend", &e, ""));
            Task::none()
        }
    };
    (flea, Task::batch([task, polling]))
}

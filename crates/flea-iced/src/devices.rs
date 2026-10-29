// The DEVICES group: ui/js/Devices.js reads lsblk, ui/js/Eject.js judges an eject, and
// ui/DeviceMounts.qml runs the five-second poll, the mounts and the releases. No toolkit here:
// the service asks for processes and timers through Effect and is told their results.
//
// The verdict on an eject is read off the next listing, never off gio's exit status: an eject can
// exit 0 with the volume still mounted, and "safe to unplug" must only ever follow a listing that
// says so (AGENTS.md "The rail menu, tested with a stubbed gio and a stubbed lsblk").
use serde_json::Value;

pub const POLL_MS: u64 = 5000;
// A listing that stopped answering froze the rail for the life of the process; issue 2026-09-02.
pub const LIST_TIMEOUT_MS: u64 = 10000;
pub const MOUNT_TIMEOUT_MS: u64 = 15000;
pub const EJECT_VERDICT_MS: u64 = 15000;

pub const LSBLK: [&str; 5] = ["lsblk", "--bytes", "--json", "-o", "NAME,PATH,LABEL,MOUNTPOINTS,RM,TRAN,SIZE,TYPE,MODEL,FSTYPE,PARTTYPENAME"];

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    // "disk" is the system disk, drawn under the host's name; "volume" is everything else.
    pub kind: &'static str,
    pub label: String,
    pub device: String,
    pub path: String,
    pub mounted: bool,
    pub removable: bool,
    pub size: Option<u64>,
    // Built under Show unmounted volumes, which is what offers Mount, Open and Unmount on it.
    pub volume_menu: bool,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn kids(v: &Value) -> &[Value] {
    v.get("children").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn points(v: &Value) -> impl Iterator<Item = &str> {
    v.get("mountpoints").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str)
}

fn holds_root(v: &Value) -> bool {
    points(v).any(|p| p == "/") || kids(v).iter().any(holds_root)
}

fn mount_of(v: &Value) -> String {
    points(v).find(|p| p.starts_with('/')).unwrap_or("").to_string()
}

fn device_path(v: &Value) -> String {
    let p = s(v, "path");
    if p.is_empty() { format!("/dev/{}", s(v, "name")) } else { p }
}

fn unpluggable(v: &Value) -> bool {
    v.get("rm").and_then(Value::as_bool) == Some(true) || s(v, "tran").eq_ignore_ascii_case("usb")
}

fn browsable(v: &Value) -> bool {
    let fs = s(v, "fstype").to_lowercase();
    !fs.is_empty() && fs != "swap" && !fs.starts_with("crypto_") && !s(v, "parttypename").to_lowercase().contains("efi")
}

fn bytes(v: &Value) -> Option<u64> {
    v.get("size").and_then(Value::as_u64)
}

fn collect(nodes: &[Value], model: &str, unplugs: bool, out: &mut Vec<Device>, unmounted: bool) {
    for n in nodes {
        let own = if s(n, "model").is_empty() { model.to_string() } else { s(n, "model") };
        let pulls = unpluggable(n) || (unplugs && s(n, "type") == "part");
        let leaf = kids(n).is_empty();
        if !s(n, "name").is_empty() && leaf && (pulls || !mount_of(n).is_empty() || (unmounted && browsable(n))) {
            let path = mount_of(n);
            let label = if !s(n, "label").is_empty() { s(n, "label") } else if !own.is_empty() { own.clone() } else { s(n, "name") };
            out.push(Device {
                kind: "volume",
                label,
                device: device_path(n),
                mounted: !path.is_empty(),
                path,
                removable: pulls,
                size: bytes(n),
                volume_menu: unmounted,
            });
        }
        collect(kids(n), &own, pulls, out, unmounted);
    }
}

// Devices.parseDevices: the system disk first, under the host's name, then every leaf volume that
// can be pulled out or is mounted, or under Show unmounted, holds a filesystem worth browsing.
pub fn parse(body: &str, unmounted: bool, host: &str) -> Vec<Device> {
    let Ok(tree) = serde_json::from_str::<Value>(body) else { return vec![] };
    let nodes = tree.get("blockdevices").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = vec![];
    let system = nodes.iter().position(|n| !s(n, "name").is_empty() && holds_root(n));
    if let Some(i) = system {
        let n = &nodes[i];
        let label = if host.is_empty() { s(n, "name") } else { host.to_string() };
        out.push(Device { kind: "disk", label, device: device_path(n), path: "/".into(), mounted: true, removable: false, size: bytes(n), volume_menu: false });
    }
    for (i, n) in nodes.iter().enumerate() {
        let name = s(n, "name");
        if name.is_empty() || Some(i) == system || name.starts_with("zram") || name.starts_with("loop") {
            continue;
        }
        collect(std::slice::from_ref(n), "", false, &mut out, unmounted);
    }
    out
}

fn kernel_name(device: &str) -> &str {
    device.rsplit('/').next().unwrap_or(device)
}

fn holds(n: &Value, name: &str) -> bool {
    s(n, "name") == name || kids(n).iter().any(|k| holds(k, name))
}

fn in_use(n: &Value) -> bool {
    n.get("mountpoints").and_then(Value::as_array).is_some_and(|ps| ps.iter().any(|p| p.as_str().is_some_and(|s| !s.is_empty())))
}

fn any_mounted(n: &Value) -> bool {
    in_use(n) || kids(n).iter().any(any_mounted)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Safe,
    Mounted,
    Unknown,
}

// Eject.diskOf: Err is a listing that cannot answer; Ok(None) a drive that is gone, which is safe.
fn disk_of(body: &str, device: &str) -> Result<Option<Value>, ()> {
    let tree: Value = serde_json::from_str(body).map_err(|_| ())?;
    let nodes = tree.get("blockdevices").and_then(Value::as_array).filter(|n| !n.is_empty()).ok_or(())?;
    let name = kernel_name(device);
    if name.is_empty() {
        return Err(());
    }
    Ok(nodes.iter().find(|n| holds(n, name)).cloned())
}

pub fn verdict(body: &str, device: &str) -> Verdict {
    match disk_of(body, device) {
        Err(()) => Verdict::Unknown,
        Ok(None) => Verdict::Safe,
        Ok(Some(d)) => if any_mounted(&d) { Verdict::Mounted } else { Verdict::Safe },
    }
}

// The other volumes on the same drive that are still mounted, unless the asked one is among them.
pub fn blockers(body: &str, device: &str) -> Vec<String> {
    let Ok(Some(disk)) = disk_of(body, device) else { return vec![] };
    fn walk(n: &Value, out: &mut Vec<(String, String)>) {
        if in_use(n) {
            let label = if s(n, "label").is_empty() { s(n, "name") } else { s(n, "label") };
            out.push((s(n, "name"), label));
        }
        kids(n).iter().for_each(|k| walk(k, out));
    }
    let mut mounted = vec![];
    walk(&disk, &mut mounted);
    if mounted.iter().any(|(n, _)| n == kernel_name(device)) {
        return vec![];
    }
    mounted.into_iter().map(|(_, l)| l).collect()
}

pub fn sentence(v: Verdict, label: &str, others: &[String]) -> (String, bool) {
    match v {
        Verdict::Safe => (format!("Ejected {label}, it is safe to unplug."), false),
        Verdict::Mounted if others.is_empty() => (format!("{label} is still mounted; close what is using it."), true),
        Verdict::Mounted => {
            let tail = if others.len() > 1 { " are still mounted, eject those instead." } else { " is still mounted, eject that instead." };
            (format!("{label} could not be ejected; {} on the same drive{tail}", others.join(", ")), true)
        }
        Verdict::Unknown => (format!("Could not confirm {label} was ejected; do not unplug it yet."), true),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    // Run an argv; its exit code (None when it never started or was timed out) comes back to the
    // matching method. A listing carries its stdout too.
    List,
    Run(Job, Vec<String>),
    Timer(Timer, u64, u64),
    Message(String, bool),
    Opened(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Job {
    Mount,
    Unmount,
    Eject,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timer {
    Poll,
    Mount,
    EjectVerdict,
}

#[derive(Debug, Default)]
pub struct Devices {
    pub entries: Vec<Device>,
    pub show_unmounted: bool,
    pub host: String,
    listing: bool,
    listings_started: u64,
    pending_open: Option<(String, String)>,
    mounting: bool,
    unmount_label: String,
    unmounting: bool,
    eject: Option<(String, String)>,
    verdict_from: u64,
    generation: u64,
    // The generation each one-shot timer was armed with; a fire carrying any other is stale.
    mount_timer: u64,
    verdict_timer: u64,
}

impl Devices {
    pub fn new(host: String, show_unmounted: bool) -> Self {
        Devices { host, show_unmounted, ..Default::default() }
    }

    fn timer(&mut self, which: Timer, ms: u64) -> Effect {
        self.generation += 1;
        Effect::Timer(which, self.generation, ms)
    }

    // The poll's own tick: a listing still out is not stacked with a second one.
    pub fn poll(&mut self) -> Vec<Effect> {
        if self.listing {
            return vec![];
        }
        self.listing = true;
        self.listings_started += 1;
        vec![Effect::List]
    }

    pub fn tick(&mut self) -> Vec<Effect> {
        let mut out = self.poll();
        out.push(Effect::Timer(Timer::Poll, 0, POLL_MS));
        out
    }

    // stdout None is a listing that failed or was timed out: the rows stand rather than empty, which
    // is what keeps Eject reachable while a device is misbehaving.
    pub fn listed(&mut self, stdout: Option<String>) -> Vec<Effect> {
        self.listing = false;
        let Some(body) = stdout else { return vec![] };
        let mut out = self.judge_eject(&body);
        self.entries = parse(&body, self.show_unmounted, &self.host);
        out.extend(self.open_when_mounted());
        out
    }

    fn open_when_mounted(&mut self) -> Vec<Effect> {
        let Some((device, _)) = self.pending_open.clone() else { return vec![] };
        match self.entries.iter().find(|e| e.device == device && e.mounted) {
            Some(e) => {
                self.pending_open = None;
                vec![Effect::Opened(e.path.clone())]
            }
            None => vec![],
        }
    }

    pub fn activate(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self.entries.get(index).cloned() else { return vec![] };
        if e.mounted {
            return vec![Effect::Opened(e.path)];
        }
        self.mount(&e)
    }

    fn mount(&mut self, e: &Device) -> Vec<Effect> {
        if self.mounting {
            return vec![];
        }
        self.mounting = true;
        self.pending_open = Some((e.device.clone(), e.label.clone()));
        let t = self.timer(Timer::Mount, MOUNT_TIMEOUT_MS);
        self.mount_timer = self.generation;
        vec![Effect::Run(Job::Mount, vec!["gio".into(), "mount".into(), "-d".into(), e.device.clone()]), t]
    }

    pub fn mounted(&mut self, code: Option<i32>) -> Vec<Effect> {
        self.mounting = false;
        if code == Some(0) {
            return self.poll();
        }
        let Some((_, label)) = self.pending_open.take() else { return vec![] };
        vec![Effect::Message(format!("{label} could not be mounted; unplug it and plug it back in."), true)]
    }

    pub fn unmount(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self.entries.get(index).cloned().filter(|e| e.kind == "volume" && e.mounted) else { return vec![] };
        if self.unmounting {
            return vec![];
        }
        self.unmounting = true;
        self.unmount_label = e.label.clone();
        vec![Effect::Run(Job::Unmount, vec!["gio".into(), "mount".into(), "-u".into(), e.path])]
    }

    pub fn unmounted(&mut self, code: Option<i32>) -> Vec<Effect> {
        self.unmounting = false;
        let mut out = vec![];
        if code != Some(0) {
            out.push(Effect::Message(format!("{} could not be unmounted; something is still using it.", self.unmount_label), true));
        }
        out.extend(self.poll());
        out
    }

    // gio mount -e, never mount -d: glib dispatches -d before it reads --eject. And never -f.
    pub fn eject(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self.entries.get(index).cloned().filter(|e| e.kind == "volume" && e.mounted) else { return vec![] };
        if let Some((_, label)) = &self.eject {
            return vec![Effect::Message(format!("Still ejecting {label}; wait for its result."), false)];
        }
        self.eject = Some((e.device.clone(), e.label.clone()));
        self.verdict_from = 0;
        vec![
            Effect::Run(Job::Eject, vec!["gio".into(), "mount".into(), "-e".into(), e.path]),
            Effect::Message(format!("Ejecting {}, do not unplug it yet.", e.label), false),
        ]
    }

    // The exit code is deliberately unread: only a listing started after this may judge it.
    pub fn ejected(&mut self, _code: Option<i32>) -> Vec<Effect> {
        self.verdict_from = self.listings_started + 1;
        let t = self.timer(Timer::EjectVerdict, EJECT_VERDICT_MS);
        self.verdict_timer = self.generation;
        let mut out = vec![t];
        out.extend(self.poll());
        out
    }

    fn judge_eject(&mut self, body: &str) -> Vec<Effect> {
        let Some((device, _)) = self.eject.clone() else { return vec![] };
        if self.verdict_from == 0 || self.listings_started < self.verdict_from {
            return vec![];
        }
        match verdict(body, &device) {
            Verdict::Unknown => vec![],
            v => self.report(v, blockers(body, &device)),
        }
    }

    fn report(&mut self, v: Verdict, others: Vec<String>) -> Vec<Effect> {
        let Some((_, label)) = self.eject.take() else { return vec![] };
        let (text, alert) = sentence(v, &label, &others);
        vec![Effect::Message(text, alert)]
    }

    pub fn timer_fired(&mut self, which: Timer, generation: u64) -> Vec<Effect> {
        match which {
            Timer::Poll => self.tick(),
            Timer::Mount if generation == self.mount_timer => match self.pending_open.take() {
                Some((_, label)) => vec![Effect::Message(format!("{label} mounted but never reported a folder to open."), true)],
                None => vec![],
            },
            // Fifteen seconds of listings that never settled it: say so rather than say nothing.
            Timer::EjectVerdict if generation == self.verdict_timer => self.report(Verdict::Unknown, vec![]),
            _ => vec![],
        }
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;

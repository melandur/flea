use super::*;

// The shape lsblk --json answers on this box: a system NVMe and a USB stick with one partition.
fn listing(stick_mounted: bool) -> String {
    let point = if stick_mounted { r#"["/run/media/me/STICK"]"# } else { "[null]" };
    format!(
        r#"{{"blockdevices":[
        {{"name":"nvme0n1","path":"/dev/nvme0n1","size":512110190592,"type":"disk","rm":false,"tran":"nvme","mountpoints":[null],
          "children":[{{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","type":"part","mountpoints":["/boot"]}},
                      {{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","type":"part","mountpoints":["/","/home"]}}]}},
        {{"name":"zram0","type":"disk","mountpoints":["[SWAP]"]}},
        {{"name":"sda","path":"/dev/sda","model":"SanDisk","size":32000000000,"type":"disk","rm":true,"tran":"usb","mountpoints":[null],
          "children":[{{"name":"sda1","path":"/dev/sda1","label":"STICK","type":"part","fstype":"vfat","mountpoints":{point}}}]}}
        ]}}"#
    )
}

#[test]
fn the_system_disk_is_the_host_and_a_stick_is_a_removable_volume() {
    let d = parse(&listing(true), false, "melandur");
    assert_eq!(d.len(), 2);
    assert_eq!((d[0].kind, d[0].label.as_str(), d[0].path.as_str()), ("disk", "melandur", "/"));
    assert_eq!((d[1].label.as_str(), d[1].mounted, d[1].removable), ("STICK", true, true));
    assert_eq!(d[1].path, "/run/media/me/STICK");
}

#[test]
fn an_eject_is_judged_by_the_next_listing_and_never_by_its_exit_code() {
    let mut dv = Devices::new("h".into(), false);
    dv.poll();
    dv.listed(Some(listing(true)));
    let out = dv.eject(1);
    assert_eq!(out[0], Effect::Run(Job::Eject, vec!["gio".into(), "mount".into(), "-e".into(), "/run/media/me/STICK".into()]));
    // gio exits 0, but the listing still shows it mounted: refused, not "safe".
    let out = dv.ejected(Some(0));
    assert!(out.contains(&Effect::List));
    let out = dv.listed(Some(listing(true)));
    assert_eq!(out, vec![Effect::Message("STICK is still mounted; close what is using it.".into(), true)]);
    // And once a later listing has dropped the mountpoint, the safe sentence.
    dv.eject(1);
    dv.ejected(Some(0));
    let out = dv.listed(Some(listing(false)));
    assert_eq!(out[0], Effect::Message("Ejected STICK, it is safe to unplug.".into(), false));
}

#[test]
fn a_listing_that_failed_keeps_the_rows_and_a_listing_in_flight_is_not_stacked() {
    let mut dv = Devices::new("h".into(), false);
    dv.poll();
    dv.listed(Some(listing(true)));
    assert_eq!(dv.poll(), vec![Effect::List]);
    assert!(dv.poll().is_empty());
    dv.listed(None);
    assert_eq!(dv.entries.len(), 2);
}

#[test]
fn activating_an_unmounted_volume_mounts_it_and_opens_it_once_listed() {
    let mut dv = Devices::new("h".into(), true);
    dv.poll();
    dv.listed(Some(listing(false)));
    assert!(dv.entries[1].volume_menu && !dv.entries[1].mounted);
    let out = dv.activate(1);
    assert!(matches!(&out[0], Effect::Run(Job::Mount, argv) if argv[3] == "/dev/sda1"));
    dv.mounted(Some(0));
    let out = dv.listed(Some(listing(true)));
    assert_eq!(out, vec![Effect::Opened("/run/media/me/STICK".into())]);
}

#[test]
fn an_eject_no_listing_settles_says_it_could_not_confirm_and_a_stale_timer_says_nothing() {
    let mut dv = Devices::new("h".into(), false);
    dv.poll();
    dv.listed(Some(listing(true)));
    dv.eject(1);
    let out = dv.ejected(Some(0));
    let Some(Effect::Timer(Timer::EjectVerdict, generation, _)) = out.first().cloned() else { panic!("no verdict timer") };
    assert!(dv.timer_fired(Timer::EjectVerdict, generation - 1).is_empty());
    assert_eq!(dv.timer_fired(Timer::EjectVerdict, generation),
               vec![Effect::Message("Could not confirm STICK was ejected; do not unplug it yet.".into(), true)]);
}

#[test]
fn a_volume_on_the_same_drive_names_itself_as_the_blocker() {
    let body = r#"{"blockdevices":[{"name":"sdb","children":[
        {"name":"sdb1","label":"A","mountpoints":["/a"]},{"name":"sdb2","label":"B","mountpoints":["/b"]}]}]}"#;
    assert_eq!(blockers(body, "/dev/sdb1"), Vec::<String>::new());
    let body2 = r#"{"blockdevices":[{"name":"sdb","children":[
        {"name":"sdb1","label":"A","mountpoints":[null]},{"name":"sdb2","label":"B","mountpoints":["/b"]}]}]}"#;
    assert_eq!(verdict(body2, "/dev/sdb1"), Verdict::Mounted);
    assert_eq!(sentence(Verdict::Mounted, "A", &blockers(body2, "/dev/sdb1")).0,
               "A could not be ejected; B on the same drive is still mounted, eject that instead.");
}

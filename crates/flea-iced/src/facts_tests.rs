use super::*;
use crate::pane::tests::row;

fn file(n: &str, icon: &str) -> Row {
    Row { i: icon.into(), s: 2_400, m: 1_700_000_000, p: 0o100644, ..row(n, false) }
}

#[test]
fn the_three_that_outrank_the_row_then_the_name_then_the_icon() {
    let r = file("a.rs", "text-x-generic");
    assert_eq!(state(Some(&r), 2, false, "", "Rust source"), State::Multi);
    assert_eq!(state(None, 1, false, "", ""), State::Unsupported);
    assert_eq!(state(Some(&r), 1, true, "x", ""), State::Error);
    assert_eq!(state(Some(&r), 1, true, "", ""), State::Loading);
    assert_eq!(state(Some(&r), 1, false, "", "Rust source"), State::Code);
    assert_eq!(state(Some(&Row { p: 0o120777, ..r.clone() }), 1, false, "", ""), State::Symlink);
    assert_eq!(state(Some(&file("core.dump", "text-x-generic")), 1, false, "", UNKNOWN_KIND), State::Unsupported);
    assert_eq!(state(Some(&file("a.PDF", "x-office-document")), 1, false, "", ""), State::Pdf);
    assert_eq!(state(Some(&file("s.xlsx", "x-office-spreadsheet")), 1, false, "", ""), State::Text);
    assert_eq!(state(Some(&file("notes.md", "x-office-document")), 1, false, "", ""), State::Text);
    // Only a row the icon called text is promoted: an image named .css stays an image.
    assert_eq!(state(Some(&file("a.css", "image-x-generic")), 1, false, "", ""), State::Image);
    assert_eq!(state(Some(&file("run.sh", "text-x-script")), 1, false, "", ""), State::Code);
    assert_eq!(state(Some(&file("a.zip", "package-x-generic")), 1, false, "", ""), State::Archive);
}

#[test]
fn each_state_states_its_own_rows_and_a_label_with_no_value_is_not_drawn() {
    let r = file("a.png", "image-x-generic");
    let labels = |f: Vec<Fact>| f.into_iter().map(|(l, _)| l).collect::<Vec<_>>();
    assert_eq!(labels(facts(State::Image, &r, None, "PNG image", 0)), ["Kind", "Size", "Modified"]);
    let m = Meta { w: 640, h: 480, ms: 3_723_000, rate: 44_100, lines: 12, partial: true, entries: 3, unpacked: 240_800, ..Default::default() };
    assert_eq!(facts(State::Image, &r, Some(&m), "PNG image", 0)[3], ("Pixels", "640 × 480".into()));
    assert_eq!(facts(State::Video, &r, Some(&m), "v", 0)[3], ("Duration", "1:02:03".into()));
    assert_eq!(facts(State::Audio, &r, Some(&m), "a", 0)[4], ("Rate", "44.1 kHz".into()));
    assert_eq!(facts(State::Text, &r, Some(&m), "t", 0)[3], ("Lines", "> 12".into()));
    assert_eq!(facts(State::Code, &r, Some(&Meta { lines_failed: true, ..m.clone() }), "c", 0)[3], ("Mode", "rw-r--r--".into()));
    assert_eq!(facts(State::Archive, &r, Some(&m), "z", 0)[3], ("Entries", "3, 240.8 kB out".into()));
    assert_eq!(facts(State::Pdf, &r, None, "p", 7)[3], ("Pages", "7".into()));
    let link = facts(State::Symlink, &r, Some(&Meta { target: "/x".into(), target_dir: true, ..Default::default() }), "", 0);
    assert_eq!(labels(link.clone()), ["Kind", "Target", "Points at", "Mode"]);
    assert_eq!(link[2].1, "Folder");
    assert_eq!(labels(facts(State::Loading, &r, None, "k", 0)), ["Kind", "Size", "State"]);
    assert_eq!(facts(State::Error, &r, Some(&Meta { owner: "gm".into(), ..Default::default() }), "k", 0)[3], ("Owner", "gm".into()));
    assert_eq!(labels(facts(State::Unsupported, &r, None, "Data", 0)), ["Kind", "Size", "Modified", "Mode"]);
}

#[test]
fn a_selection_is_a_summary_and_a_wider_one_than_the_window_is_a_floor() {
    let rows = [file("a.png", "image-x-generic"), file("b.png", "image-x-generic"), file("c.mp4", "video-x-generic"), file("d.txt", "text-x-generic")];
    let refs: Vec<&Row> = rows.iter().collect();
    let f = multi_facts(&refs, 4);
    assert_eq!(f[0], ("Kinds", "2 images, 1 video, 1 text".into()));
    assert_eq!(f[1], ("Combined", "9.6 kB".into()));
    assert!(multi_facts(&refs, 9)[1].1.starts_with("> "));
    assert_eq!(multi_marks(&refs), ["image", "film", "file-text"]);
    assert_eq!(sample_rate(48_000), "48 kHz");
    assert_eq!(duration(61_000), "1:01");
    assert_eq!(permissions(0o100755), "rwxr-xr-x");
}

use super::*;

fn open(p: &mut Preview, path: &str, icon: &str, size: u64) -> Vec<Effect> {
    p.open(path.into(), icon.into(), size, String::new(), 3)
}

#[test]
fn each_kind_asks_for_what_it_draws() {
    let mut p = Preview::default();
    assert_eq!(open(&mut p, "/d/a.rs", "text-x-script", 10), vec![Effect::LoadText("/d/a.rs".into())]);
    assert_eq!(open(&mut p, "/d/s.ods", "x-office-spreadsheet", 10), vec![Effect::LoadSheet("/d/s.ods".into())]);
    assert_eq!(open(&mut p, "/d/a.png", "image-x-generic", 10), vec![Effect::RenderImage("/d/a.png".into())]);
    assert_eq!(open(&mut p, "/d/a.zip", "package-x-generic", 10), vec![Effect::AskArchive(3)]);
    assert!(open(&mut p, "/d/a.out", "application-x-executable", 10).is_empty());
    assert_eq!(open(&mut p, "/d/b.pdf", "x-office-document", 10), vec![Effect::LoadPdf("/d/b.pdf".into())]);
    assert_eq!(open(&mut p, "/d/c.mkv", "video-x-generic", 10), vec![Effect::OpenMedia("/d/c.mkv".into())]);
    assert!(open(&mut p, "/d/a.out", "application-x-executable", 10).is_empty());
    assert_eq!(p.status(false), Some(("This file cannot be previewed.", false)));
    // Over the gate nothing is read at all.
    assert!(open(&mut p, "/d/huge.log", "text-x-generic", TEXT_MAX_BYTES + 1).is_empty());
    assert_eq!(p.status(false), Some(("This file is too large to preview.", false)));
}

#[test]
fn a_held_key_loads_the_first_and_the_last_and_nothing_between() {
    let mut p = Preview::default();
    open(&mut p, "/d/0.txt", "text-x-generic", 1);
    let out = p.follow("/d/1.txt".into(), "text-x-generic".into(), 1, String::new(), 1);
    assert!(out.contains(&Effect::LoadText("/d/1.txt".into())));
    let Some(Effect::Settle(first)) = out.last().cloned() else { panic!() };
    let out = p.follow("/d/2.txt".into(), "text-x-generic".into(), 1, String::new(), 2);
    assert_eq!(out.len(), 1, "a move inside the window loads nothing");
    let Effect::Settle(second) = out[0] else { panic!() };
    p.follow("/d/3.txt".into(), "text-x-generic".into(), 1, String::new(), 3);
    assert!(p.settled(first).is_empty());
    assert!(p.settled(second).is_empty(), "a superseded settle is stale");
    let last = p.settle_gen;
    assert_eq!(p.settled(last), vec![Effect::LoadText("/d/3.txt".into())]);
}

#[test]
fn inset_keys_move_the_listing_and_expanded_keys_scroll_the_surface() {
    let mut p = Preview::default();
    open(&mut p, "/d/a.txt", "text-x-generic", 1);
    assert_eq!(p.key(Action::Down), vec![Effect::MoveCursor(1)]);
    assert!(p.key(Action::Forward).is_empty());
    assert!(p.expanded);
    assert_eq!(p.key(Action::Down), vec![Effect::Scroll(1)]);
    // Expanded, Left is the content's own axis, which a text file does not have: it stays open.
    assert!(p.key(Action::Back).is_empty() && p.active);
    assert_eq!(p.key(Action::Escape), vec![Effect::Closed]);
    assert!(!p.active && !p.expanded);
    open(&mut p, "/d/a.txt", "text-x-generic", 1);
    assert_eq!(p.key(Action::Back), vec![Effect::Closed]);
}

#[test]
fn an_archive_answer_counts_only_for_the_row_still_open_and_scrolls_within_its_names() {
    let mut p = Preview::default();
    open(&mut p, "/d/a.zip", "package-x-generic", 10);
    let names: Vec<(String, bool)> = (0..30).map(|i| (format!("n{i}"), false)).collect();
    p.archive_answered(9, Archive { entries: 30, names: names.clone(), failed: false });
    assert!(p.archive.is_none());
    p.archive_answered(3, Archive { entries: 30, names, failed: false });
    p.scroll_archive(100, 10);
    assert_eq!(p.archive_offset, 20);
    p.scroll_archive(-100, 10);
    assert_eq!(p.archive_offset, 0);
    p.archive.as_mut().unwrap().failed = true;
    assert_eq!(p.status(false), Some(("This archive could not be read.", true)));
}

#[test]
fn expanded_left_and_right_turn_a_page_or_seek_and_m_mutes_only_media() {
    let mut p = Preview::default();
    open(&mut p, "/d/b.pdf", "x-office-document", 1);
    p.key(Action::Forward);
    assert_eq!(p.key(Action::Forward), vec![Effect::Turn(1)]);
    assert_eq!(p.key(Action::Back), vec![Effect::Turn(-1)]);
    assert!(p.key(Action::Mute).is_empty());
    open(&mut p, "/d/c.ogg", "audio-x-generic", 1);
    p.key(Action::Expand);
    assert_eq!(p.key(Action::Back), vec![Effect::Seek(-5000)]);
    assert_eq!(p.key(Action::Mute), vec![Effect::Mute]);
    assert_eq!(p.status(true), Some(("This file could not be played.", true)));
}

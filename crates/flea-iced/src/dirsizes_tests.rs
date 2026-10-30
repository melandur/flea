use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::Reply;

fn pane(names: &[(&str, bool)]) -> Pane {
    let mut p = Pane::new(false, 10);
    p.dirs.wanted = true;
    p.open("/d".into(), None);
    p.receive(Reply::Listed { n: names.len(), v: 0 });
    p.receive(Reply::Rows { start: 0, rows: names.iter().map(|(n, d)| row(n, *d)).collect(), kinds: vec![] });
    p
}

fn dirsize_asks(out: &[Effect]) -> Vec<Vec<usize>> {
    sends(out).into_iter().filter_map(|r| if let Request::DirSize { rows } = r { Some(rows) } else { None }).collect()
}

#[test]
fn the_visible_folders_are_asked_once_on_settle_and_only_while_wanted() {
    let mut p = pane(&[("a", true), ("f", false), ("b", true)]);
    let gen = p.settle_gen;
    p.viewport(0, 10, 2);
    let out = p.settled(p.settle_gen);
    assert_ne!(gen, p.settle_gen);
    assert_eq!(dirsize_asks(&out), [vec![0, 2]]);
    assert!(dirsize_asks(&p.settled(p.settle_gen)).is_empty(), "asked once");
    p.on_dirsized(0, 4000, true, 3);
    p.on_dirsized(2, 0, false, -1);
    assert_eq!(size_text(p.dirs.get(0), true), ">4.0 kB");
    assert_eq!(size_text(p.dirs.get(0), false), "·");
    assert_eq!((items_text(true, false, p.dirs.get(0)), items_text(true, false, p.dirs.get(2)), items_text(false, false, None)), ("3".into(), "·".into(), String::new()));
    let mut q = pane(&[("a", true)]);
    q.dirs.wanted = false;
    q.viewport(0, 10, 0);
    assert!(dirsize_asks(&q.settled(q.settle_gen)).is_empty());
}

#[test]
fn a_scroll_cancels_what_still_waits_and_an_answer_survives_an_attrib_only_reread() {
    let mut p = pane(&[("a", true), ("b", true)]);
    p.viewport(0, 10, 1);
    p.settled(p.settle_gen);
    p.on_dirsized(0, 10, false, 1);
    let out = p.viewport(1, 10, 1);
    assert!(sends(&out).contains(&Request::DirSizeCancel));
    assert!(sends(&p.viewport(1, 10, 1)).iter().all(|r| *r != Request::DirSizeCancel), "nothing moved, nothing cancelled");
    // An attrib-only burst keeps the answer; a structural one, or another folder, spends it.
    p.receive(Reply::Changed { path: "/d".into(), sizes: false });
    p.reread();
    listed(&mut p, "/d", 2);
    assert_eq!(p.dirs.get(0).map(|d| d.bytes), Some(10));
    p.receive(Reply::Changed { path: "/d".into(), sizes: true });
    p.reread();
    listed(&mut p, "/d", 2);
    assert!(p.dirs.get(0).is_none());
}

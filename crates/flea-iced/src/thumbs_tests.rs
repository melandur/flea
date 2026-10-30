use super::*;
use crate::pane::tests::{listed, row, sends};
use crate::wire::Reply;

fn trow(n: &str, t: bool, icon: &str) -> Row {
    Row { t, i: icon.into(), ..row(n, false) }
}

fn settle(p: &mut Pane) -> Vec<Effect> {
    let generation = p.settle_gen;
    p.settled(generation)
}

#[test]
fn a_settle_asks_only_visible_thumbnailable_rows_and_never_asks_twice() {
    let mut p = Pane::new(false, 3).with_view("media", false);
    p.open("/m".into(), None);
    p.receive(Reply::Listed { n: 6, v: 0 });
    let rows = vec![trow("a.jpg", true, "image-x-generic"), trow("b.txt", false, "text-x-generic"),
                    trow("c.mp4", true, "video-x-generic"), trow("d.jpg", true, "image-x-generic"),
                    trow("e.jpg", true, "image-x-generic"), trow("f.jpg", true, "image-x-generic")];
    let out = p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
    assert!(out.contains(&Effect::Settle(p.settle_gen, FIRST_SETTLE_MS)));
    p.viewport(0, 3, 3);
    assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 2, 3] }]);
    // The next settle is the fling debounce, and a row already asked is not asked again.
    assert_eq!(p.restart_settle(), Effect::Settle(p.settle_gen, SETTLE_MS));
    assert!(settle(&mut p).is_empty());
}

#[test]
fn a_settle_before_any_viewport_report_names_the_first_screen() {
    let mut p = Pane::new(false, 3).with_view("media", false);
    p.open("/m".into(), None);
    p.receive(Reply::Listed { n: 5, v: 0 });
    let rows = (0..5).map(|i| trow(&format!("{i}.jpg"), true, "image-x-generic")).collect();
    p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
    assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 1, 2] }]);
}

#[test]
fn a_stale_settle_does_nothing_and_a_row_left_waiting_is_cancelled_then_asked_again() {
    let mut p = Pane::new(false, 2).with_view("media", false);
    p.open("/m".into(), None);
    p.receive(Reply::Listed { n: 4, v: 0 });
    let rows = (0..4).map(|i| trow(&format!("{i}.jpg"), true, "image-x-generic")).collect();
    p.receive(Reply::Rows { start: 0, rows, kinds: vec![] });
    p.viewport(0, 2, 1);
    let stale = p.settle_gen;
    p.viewport(0, 2, 1);
    assert!(p.settled(stale).is_empty());
    assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![0, 1] }]);
    p.receive(Reply::Thumbed { row: 0, file: "/c/0.png".into() });
    p.viewport(2, 2, 3);
    assert_eq!(sends(&settle(&mut p)), vec![Request::ThumbCancel { rows: vec![1] }, Request::Thumb { rows: vec![2, 3] }]);
    p.viewport(0, 2, 1);
    // 0 is answered and terminal; 1 was cancelled back to absent, so it is asked again.
    let out = sends(&settle(&mut p));
    assert_eq!(out, vec![Request::ThumbCancel { rows: vec![2, 3] }, Request::Thumb { rows: vec![1] }]);
}

#[test]
fn images_mode_skips_video_and_off_asks_nothing() {
    let mut p = Pane::new(false, 2).with_view("images", false);
    p.open("/m".into(), None);
    p.receive(Reply::Listed { n: 2, v: 0 });
    p.receive(Reply::Rows { start: 0, rows: vec![trow("a.mp4", true, "video-x-generic"), trow("b.png", true, "image-x-generic")], kinds: vec![] });
    p.viewport(0, 2, 1);
    assert_eq!(sends(&settle(&mut p)), vec![Request::Thumb { rows: vec![1] }]);
    p.thumb_mode = "off".into();
    p.thumbs.clear();
    assert!(settle(&mut p).is_empty());
}

#[test]
fn an_answer_decodes_and_a_late_one_for_a_superseded_listing_is_dropped() {
    let mut p = Pane::new(false, 2).with_view("media", false);
    p.open("/m".into(), None);
    listed(&mut p, "/m", 3);
    assert_eq!(p.receive(Reply::Thumbed { row: 1, file: "/c/x.png".into() }), vec![Effect::DecodeThumb("/c/x.png".into())]);
    assert_eq!(p.thumb_file(1), Some("/c/x.png"));
    p.open("/n".into(), None);
    assert!(p.receive(Reply::Thumbed { row: 2, file: "/c/y.png".into() }).is_empty());
    listed(&mut p, "/n", 3);
    assert_eq!(p.thumb_file(1), None);
}

#[test]
fn the_map_is_capped_in_answer_order() {
    let mut p = Pane::new(false, 2).with_view("media", false);
    p.open("/m".into(), None);
    listed(&mut p, "/m", 400);
    for i in 0..THUMB_CAP + 5 {
        p.receive(Reply::Thumbed { row: i, file: String::new() });
    }
    assert_eq!(p.thumbs.len(), THUMB_CAP);
    assert!(!p.thumbs.contains_key(&4) && p.thumbs.contains_key(&5));
}

use super::*;
use crate::pane::tests::row;
use crate::wire::Reply;

fn file(n: &str, icon: &str, t: bool) -> Row {
    Row { i: icon.into(), t, s: 10, m: 1_700_000_000, ..row(n, false) }
}

fn pane(rows: Vec<Row>) -> Pane {
    let mut p = Pane::new(false, 10);
    p.open("/d".into(), None);
    p.receive(Reply::Listed { n: rows.len() });
    p.receive(Reply::Rows { start: 0, rows, kinds: vec!["Plain text document".into()] });
    p
}

fn settle(c: &mut ColPreview, p: &mut Pane) -> Vec<Out> {
    let out = c.follow(p, true, true);
    let Some(Out::Settle(g)) = out.first().cloned() else { panic!("no settle: {out:?}") };
    c.settled(g, p, true)
}

#[test]
fn the_cursor_row_loads_once_the_cursor_settles_with_the_hints_its_kind_names() {
    let mut p = pane(vec![file("a.rs", "text-x-generic", false), file("b.mkv", "video-x-generic", true), file("c.zip", "package-x-generic", false)]);
    let mut c = ColPreview::default();
    let out = settle(&mut c, &mut p);
    assert!(matches!(&out[..], [Out::Send(Request::Meta { row: 0, text: true, media: false, archive: false, token: 1 }), Out::Read(path, false)] if path == "/d/a.rs"), "{out:?}");
    assert_eq!(c.row_state(), Some(State::Code));
    assert!(c.busy());
    assert!(c.follow(&p, true, true).is_empty(), "nothing moved, so nothing restarts");
    // A video asks for the probe, and for its thumbnail when the pane has not.
    p.step(1);
    let out = settle(&mut c, &mut p);
    assert!(matches!(&out[..], [Out::Send(Request::Meta { row: 1, media: true, token: 2, .. }), Out::Send(Request::Thumb { rows })] if rows == &[1]), "{out:?}");
    assert_eq!(p.thumbs.get(&1), Some(&Thumb::Asked));
    p.step(1);
    assert!(matches!(&settle(&mut c, &mut p)[..], [Out::Send(Request::Meta { archive: true, text: false, .. })]));
}

#[test]
fn only_the_newest_token_lands_and_only_while_its_row_stands() {
    let mut p = pane(vec![file("a.txt", "text-x-generic", false), file("b.txt", "text-x-generic", false)]);
    let mut c = ColPreview::default();
    settle(&mut c, &mut p);
    p.step(1);
    settle(&mut c, &mut p);
    assert!(!c.meta_answered(Meta { row: 0, token: 1, ..Default::default() }, &p), "a superseded token");
    assert!(!c.meta_answered(Meta { row: 0, token: 0, ..Default::default() }, &p), "the overlay's");
    assert!(c.meta_answered(Meta { row: 1, token: 2, lines: 4, ..Default::default() }, &p));
    assert!(!c.busy());
    // The same row renamed under the cursor is another file: the column lets it go.
    p.rows[1].n = "c.txt".into();
    assert!(matches!(c.follow(&p, true, true)[..], [Out::Settle(_)]));
    assert!(c.loaded.is_none() && c.meta.is_none());
}

#[test]
fn a_selection_or_a_hidden_column_clears_and_manual_mode_never_settles() {
    let mut p = pane(vec![file("a.txt", "text-x-generic", false), file("b.png", "image-x-generic", false)]);
    let mut c = ColPreview::default();
    settle(&mut c, &mut p);
    assert!(c.loaded.is_some());
    p.selection = [0, 1].into();
    let out = settle(&mut c, &mut p);
    assert!(out.is_empty(), "a multi-selection asks for no meta");
    let l = c.loaded.as_ref().unwrap();
    assert_eq!((l.selection, l.selected.len()), (2, 2));
    assert!(c.follow(&p, false, true).is_empty());
    assert!(c.loaded.is_none(), "a column off screen holds nothing");
    assert!(c.follow(&p, true, false).is_empty(), "loadOn manual waits for a load nothing binds");
}

#[test]
fn a_settle_from_an_older_follow_is_spent() {
    let mut p = pane(vec![file("a.txt", "text-x-generic", false), file("b.txt", "text-x-generic", false)]);
    let mut c = ColPreview::default();
    let Some(Out::Settle(old)) = c.follow(&p, true, true).first().cloned() else { panic!() };
    p.step(1);
    c.follow(&p, true, true);
    assert!(c.settled(old, &mut p, true).is_empty());
    assert!(c.loaded.is_none());
}

#[test]
fn a_refused_thumbnail_and_a_row_with_none_are_no_thumbnail_coming() {
    let mut p = pane(vec![file("a.png", "image-x-generic", true), file("b.png", "image-x-generic", false)]);
    let mut c = ColPreview::default();
    settle(&mut c, &mut p);
    assert!(!c.no_thumb_coming(&p));
    p.thumbs.insert(0, Thumb::None);
    assert!(c.no_thumb_coming(&p));
    p.step(1);
    settle(&mut c, &mut p);
    assert!(c.no_thumb_coming(&p));
}

use super::*;

#[test]
fn the_rows_sample_from_the_protocol_parses() {
    let line = r#"{"t":"rows","start":0,"rows":[{"n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0},{"n":"photos","d":true,"s":4096,"m":1787790424,"p":16877,"i":"folder","t":false,"k":1,"v":56}],"kinds":["Plain text document","Folder"],"ms":1.250}"#;
    let Some(Reply::Rows { start, rows, kinds }) = parse(line) else { panic!("not rows") };
    assert_eq!(start, 0);
    assert_eq!(rows[0].n, "say \"hi\".txt");
    assert!(rows[1].d);
    assert_eq!(kinds, vec!["Plain text document", "Folder"]);
}

#[test]
fn an_unknown_line_is_other_and_junk_is_none() {
    assert_eq!(parse(r#"{"t":"dirsized","row":4,"bytes":1048576,"partial":false,"entries":12,"ms":12.500}"#), Some(Reply::Other));
    assert_eq!(parse(r#"{"t":"thumbed","row":2,"file":"/x.png","ms":1.0}"#), Some(Reply::Thumbed { row: 2, file: "/x.png".into() }));
    assert_eq!(parse("not json"), None);
}

#[test]
fn the_write_replies_from_the_protocol_parse() {
    assert_eq!(
        parse(r#"{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}"#),
        Some(Reply::TransferItem { id: 12, index: 0, name: "a.txt".into(), ok: true, err: String::new() })
    );
    assert_eq!(
        parse(r#"{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}"#),
        Some(Reply::TransferDone { id: 12, ok: 1, failed: 1, skipped: 0, cancelled: false })
    );
    assert_eq!(
        parse(r#"{"t":"transferqueued","id":13,"n":1,"moving":false,"ahead":1}"#),
        Some(Reply::TransferQueued { id: 13, n: 1, moving: false, ahead: 1 })
    );
    assert_eq!(parse(r#"{"t":"made","ok":true,"path":"/home/gm/New Folder"}"#), Some(Reply::Made { path: "/home/gm/New Folder".into() }));
    assert_eq!(parse(r#"{"t":"duplicated","ok":true,"path":"/home/gm/photo copy.jpg"}"#), Some(Reply::Duplicated { path: "/home/gm/photo copy.jpg".into() }));
    assert_eq!(Request::Duplicate { path: "/home/gm/photo.jpg".into() }.line(), "{\"c\":\"duplicate\",\"path\":\"/home/gm/photo.jpg\"}\n");
    assert_eq!(parse(r#"{"t":"paths","paths":["/a","/b"]}"#), Some(Reply::Paths { paths: vec!["/a".into(), "/b".into()] }));
}

#[test]
fn a_transfer_says_move_only_when_it_moves() {
    let l = Request::Transfer { moving: false, paths: vec!["/a".into()], dest: "/d".into() }.line();
    assert!(l.contains(r#""op":"copy""#));
    let l = Request::Transfer { moving: true, paths: vec!["/a".into()], dest: "/d".into() }.line();
    assert!(l.contains(r#""op":"move""#));
}

#[test]
fn a_request_is_one_line_with_its_newline() {
    let l = Request::List { path: "/a \"b\"".into(), first: 350, hidden: false, order: Order::default() }.line();
    assert!(l.ends_with('\n') && l.matches('\n').count() == 1);
    let v: Value = serde_json::from_str(l.trim_end()).unwrap();
    assert_eq!(v["path"], "/a \"b\"");
    assert_eq!(v["first"], 350);
}

#[test]
fn the_meta_sample_from_the_backend_parses_its_archive_half() {
    let line = r#"{"t":"meta","row":4,"w":0,"h":0,"ms":0,"rate":0,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true},{"n":"say \"hi\".txt","d":false}],"lines":0,"partial":false,"lfailed":false,"target":"","targetdir":false,"owner":"gm"}"#;
    assert_eq!(parse(line), Some(Reply::Meta { row: 4, entries: 214, names: vec![("ui".into(), true), ("say \"hi\".txt".into(), false)], archive_failed: false }));
}

#[test]
fn the_installed_catalogue_parses_and_a_refusal_carries_its_reason() {
    let ok = r#"{"t":"menuaction","id":7,"op":"installed","ok":true,"installed":[{"id":"gimp.desktop","label":"GIMP","icon":"gimp","default":false}]}"#;
    assert_eq!(parse(ok), Some(Reply::Installed { id: 7, apps: Ok(vec![("gimp.desktop".into(), "GIMP".into())]) }));
    let no = r#"{"t":"menuaction","id":7,"op":"installed","ok":false,"error":"no registry"}"#;
    assert_eq!(parse(no), Some(Reply::Installed { id: 7, apps: Err("no registry".into()) }));
}

#[test]
fn a_picker_check_answers_with_its_marks_or_its_review_or_why_not() {
    let marks = r#"{"t":"picker","id":3,"op":"validate","ok":true,"marks":[{"path":"/x/a.png","bytes":5}],"removed":1}"#;
    let Some(Reply::Picker(m)) = parse(marks) else { panic!("not a picker answer") };
    assert_eq!((m.id, m.ok, m.marks, m.removed), (3, true, vec![("/x/a.png".to_string(), 5)], 1));
    let save = r#"{"t":"picker","id":4,"op":"save","ok":true,"review":4,"path":"/x/b.txt","collision":true}"#;
    let Some(Reply::Picker(s)) = parse(save) else { panic!("not a picker answer") };
    assert_eq!((s.review, s.path.as_str(), s.collision), (4, "/x/b.txt", true));
    let no = r#"{"t":"picker","id":5,"op":"mark","ok":false,"error":"This item is not the requested file type."}"#;
    let Some(Reply::Picker(n)) = parse(no) else { panic!("not a picker answer") };
    assert_eq!((n.ok, n.error.as_str()), (false, "This item is not the requested file type."));
    let l = Request::PickerList { path: "/x".into(), first: 70, hidden: false, filter: (vec!["*.png".into()], vec![]) }.line();
    let v: Value = serde_json::from_str(l.trim_end()).unwrap();
    assert_eq!((v["c"].as_str(), v["pickerGlobs"][0].as_str()), (Some("list"), Some("*.png")));
}

use super::*;

#[test]
fn the_name_says_whether_it_converts_or_strips_and_never_overwrites_its_source() {
    assert_eq!(dest_name("photo.png", "jpg"), "photo (converted).jpg");
    assert_eq!(dest_name("photo.PNG", "png"), "photo (stripped).png");
    assert_eq!(FORMATS[default_format("photo.jpg")], "png");
    assert_eq!(FORMATS[default_format("photo.png")], "jpg");
    assert_eq!((label("webp"), label("jxl")), ("WebP".to_string(), "JXL".to_string()));
}

#[test]
fn it_checks_the_destination_first_and_converts_only_once_it_is_clear() {
    let (mut c, out) = Convert::open("/d/photo.png", "photo.png", 4);
    assert_eq!(out, vec![json!({"c": "convert", "path": "/d/photo.png", "dest": "/d/photo (converted).jpg", "strip": false, "menuId": 4, "requestId": 1, "check": true})]);
    assert!(!c.can_convert());
    // A second choice while a check is out waits for it, then checks the name now in force.
    assert!(c.choose(2).is_empty());
    let again = c.checked(&json!({"requestId": 1, "source": "/d/photo.png", "path": "/d/photo (converted).jpg", "ok": true, "collision": false}));
    assert_eq!(again[0]["dest"], "/d/photo (converted).webp");
    c.checked(&json!({"requestId": 2, "source": "/d/photo.png", "path": "/d/photo (converted).webp", "ok": false, "collision": true, "error": "photo (converted).webp already exists."}));
    assert!(!c.can_convert() && c.collision);
    assert_eq!(c.error, "photo (converted).webp already exists.");
    c.choose(0);
    c.checked(&json!({"requestId": 3, "source": "/d/photo.png", "path": "/d/photo (converted).jpg", "ok": true, "collision": false}));
    c.toggle_strip();
    let out = c.commit();
    assert_eq!(out[0]["check"], false);
    assert_eq!(out[0]["strip"], true);
    c.started(&json!({"id": 30, "requestId": 4, "source": "/d/photo.png"}));
    assert_eq!(c.done(&json!({"id": 29, "requestId": 4, "source": "/d/photo.png", "ok": true})), None);
    assert_eq!(c.done(&json!({"id": 30, "requestId": 4, "source": "/d/photo.png", "ok": true, "err": ""})), Some(true));
}

#[test]
fn a_refusal_before_anything_started_keeps_the_popup_with_its_reason() {
    let (mut c, _) = Convert::open("/d/a.png", "a.png", 0);
    c.checked(&json!({"requestId": 1, "source": "/d/a.png", "path": "/d/a (converted).jpg", "ok": true}));
    c.commit();
    assert_eq!(c.done(&json!({"id": 7, "requestId": 2, "source": "/d/a.png", "ok": true})), None);
    assert_eq!(c.done(&json!({"id": 7, "requestId": 2, "source": "/d/a.png", "ok": false, "err": "convert failed", "collision": false})), Some(false));
    assert_eq!((c.busy, c.error.as_str()), (false, "convert failed"));
}

#[test]
fn tab_reaches_convert_only_when_it_can_run() {
    let (mut c, _) = Convert::open("/d/a.png", "a.png", 0);
    for _ in 0..3 {
        c.step_focus(false);
    }
    assert_eq!(c.focus, 0);
    c.checked(&json!({"requestId": 1, "source": "/d/a.png", "path": "/d/a (converted).jpg", "ok": true}));
    c.step_focus(true);
    assert_eq!(c.focus, 3);
}

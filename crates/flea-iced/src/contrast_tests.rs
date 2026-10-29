use super::*;

fn hex(c: Color) -> String {
    let [r, g, b, _] = c.into_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[test]
fn a_colour_already_clear_of_the_ratio_is_untouched_and_a_dim_one_is_lifted_to_it() {
    let bg = Color::from_rgb8(0x1a, 0x1b, 0x26);
    let fine = Color::from_rgb8(0xa9, 0xb1, 0xd6);
    assert_eq!(ensure_ratio(fine, bg, 3.0), fine);
    let dim = Color::from_rgb8(0x41, 0x48, 0x68);
    let lifted = ensure_ratio(dim, bg, 3.0);
    assert!(ratio_of(rgb(lifted), rgb(bg)) >= 3.0);
    // And barely: one step back toward the original falls under the ratio again.
    let back = quantised(rgb(lifted).map(|v| v - 1.0 / 255.0));
    assert!(ratio_of(back, rgb(bg)) < 3.0 || hex(lifted) == "#000000");
}

#[test]
fn darker_divides_the_value_and_keeps_the_hue() {
    // Both measured with Qt.darker(c, 1.4) under /usr/lib/qt6/bin/qml on this box.
    assert_eq!(hex(darker(Color::from_rgb8(0xa9, 0xb1, 0xd6), 1.4)), "#797e99");
    assert_eq!(hex(darker(Color::from_rgb8(0x12, 0x34, 0x56), 1.4)), "#0d253d");
    assert_eq!(darker(Color::BLACK, 1.4), Color::BLACK);
}

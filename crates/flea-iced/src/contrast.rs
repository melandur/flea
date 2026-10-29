// ui/js/Contrast.js, one for one: a colour is lifted toward the far end of the background's
// luminance until it reaches a WCAG ratio, by bisection and then by single steps of 1/255.
// Plus QColor::darker, which Theme.qml and Row.qml use for muted and the row mark's dim ink.
use iced::Color;

fn channel(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
}

fn ratio_of(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn rgb(c: Color) -> [f32; 3] {
    [c.r, c.g, c.b]
}

// Contrast.js hexOf: every comparison it makes is between rounded 8-bit colours.
fn quantised(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn ensure_ratio(fg: Color, bg: Color, need: f32) -> Color {
    let (f, b) = (rgb(fg), rgb(bg));
    if ratio_of(f, b) >= need {
        return fg;
    }
    let toward = if luminance(b) > 0.179 { [0.0; 3] } else { [1.0; 3] };
    let (mut lo, mut hi, mut best) = (0.0f32, 1.0f32, toward);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        let cand = mix(f, toward, mid);
        if ratio_of(cand, b) >= need {
            best = cand;
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if ratio_of(best, b) < need {
        best = toward;
    }
    // Rounding to 8 bits can land just under the ratio, so step until it holds or nothing moves.
    let mut c = quantised(best);
    let step = if toward[0] > 0.5 { 1.0 / 255.0 } else { -1.0 / 255.0 };
    for _ in 0..255 {
        if ratio_of(c, b) >= need {
            break;
        }
        let next = quantised(c.map(|v| (v + step).clamp(0.0, 1.0)));
        if next == c {
            break;
        }
        c = next;
    }
    Color::from_rgb(c[0], c[1], c[2])
}

// QColor::darker, Qt 6's own arithmetic: 16-bit channels through HSV, the value scaled by an
// integer percentage, and back. Float HSV disagrees with it in the last bit (#797f99 against Qt's
// #797e99 for #a9b1d6), so the rounding steps below are Qt's, not a textbook conversion.
fn round(x: f32) -> u32 {
    (x + 0.5).floor() as u32
}

fn div_257(x: u32) -> u8 {
    ((x - (x >> 8) + 0x80) >> 8) as u8
}

pub fn darker(c: Color, factor: f32) -> Color {
    const MAX: f32 = u16::MAX as f32;
    let [r8, g8, b8, a8] = c.into_rgba8();
    let (r, g, b) = ((r8 as u32 * 0x101) as f32 / MAX, (g8 as u32 * 0x101) as f32 / MAX, (b8 as u32 * 0x101) as f32 / MAX);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let value = round(max * MAX);
    let (hue, sat) = if delta == 0.0 {
        (None, 0u32)
    } else {
        let mut h = if r == max {
            (g - b) / delta
        } else if g == max {
            2.0 + (b - r) / delta
        } else {
            4.0 + (r - g) / delta
        };
        h *= 60.0;
        if h < 0.0 {
            h += 360.0;
        }
        (Some(round(h * 100.0)), round(delta / max * MAX))
    };
    // Qt.darker(c, 1.4) is QColor::darker(140), an integer division of the value.
    let percent = (factor * 100.0).round() as u32;
    let v = (value * 100 / percent) as f32 / MAX;
    let s = sat as f32 / MAX;
    let h = hue.map(|h| h as f32 / 6000.0).unwrap_or(0.0);
    let i = h as u32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let (r, g, b) = if i & 1 == 1 {
        let q = v * (1.0 - s * f);
        match i {
            1 => (q, v, p),
            3 => (p, q, v),
            _ => (v, p, q),
        }
    } else {
        let t = v * (1.0 - s * (1.0 - f));
        match i {
            0 => (v, t, p),
            2 => (p, v, t),
            _ => (t, p, v),
        }
    };
    let to8 = |x: f32| div_257(round(x * MAX));
    Color::from_rgba8(to8(r), to8(g), to8(b), a8 as f32 / 255.0)
}

#[cfg(test)]
#[path = "contrast_tests.rs"]
mod tests;

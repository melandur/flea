// The reproduced marks the send rows draw instead of a lucide glyph, as ui/TailscaleMark.qml,
// ui/DropboxMark.qml, ui/LocalSendMark.qml and ui/FleaMark.qml draw them: monochrome in one palette
// role, never a brand colour, and outside the 24 unit grid Icons.js holds (AGENTS.md "The Network row
// marks"). Each answers a whole SVG document in the given colour, or None for a mark that is not one.

pub fn svg(mark: &str, hex: &str) -> Option<String> {
    let body = match mark {
        // Nine dots on the official 169 unit box; the middle row and the bottom centre are solid,
        // the other five 0.4, because the toolkit says the grey dots adapt in opacity.
        "tailscale" => {
            let centres = [30.5, 84.5, 138.5];
            let mut dots = String::new();
            for row in 0..3 {
                for col in 0..3 {
                    let solid = row == 1 || (row == 2 && col == 1);
                    dots += &format!(
                        r#"<circle cx="{}" cy="{}" r="18" fill="{hex}" fill-opacity="{}"/>"#,
                        centres[col],
                        centres[row],
                        if solid { "1" } else { "0.4" }
                    );
                }
            }
            return Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 169 169">{dots}</svg>"#));
        }
        // Five diamond tiles on a box 1.18 times as wide as it is tall, centred in a square.
        "dropbox" => {
            let (w, h) = (118.0_f64, 100.0_f64);
            let xs = [0.25, 0.75, 0.25, 0.75, 0.50];
            let ys = [0.188, 0.188, 0.564, 0.564, 0.812];
            let (hw, hh) = (0.50 * w / 2.0, 0.376 * h / 2.0);
            let mut d = String::new();
            for i in 0..5 {
                let (cx, cy) = (xs[i] * w, ys[i] * h + 9.0);
                d += &format!("M{cx} {} L{} {cy} L{cx} {} L{} {cy} Z ", cy - hh, cx + hw, cy + hh, cx - hw);
            }
            return Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 118 118"><path d="{d}" fill="{hex}"/></svg>"#));
        }
        // A disc inside a ring of eight round-capped dashes, on a unit box scaled to 100.
        "localsend" => {
            let (c, radius, half) = (50.0_f64, 46.6_f64, 22.5_f64 / 2.0);
            let mut d = String::new();
            for i in 0..8 {
                let from = (i as f64 * 45.0 - half).to_radians();
                let to = (i as f64 * 45.0 + half).to_radians();
                d += &format!(
                    "M{:.3} {:.3} A{radius} {radius} 0 0 1 {:.3} {:.3} ",
                    c + radius * from.cos(),
                    c + radius * from.sin(),
                    c + radius * to.cos(),
                    c + radius * to.sin()
                );
            }
            format!(r#"<circle cx="50" cy="50" r="27.6" fill="{hex}"/><path d="{d}" fill="none" stroke="{hex}" stroke-width="6.9" stroke-linecap="round"/>"#)
        }
        // FleaMark's settled frame: the square spiral at the lucide weight.
        "flea" => {
            return Some(format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{hex}" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M21 21H3V3h18v14H7V7h10v6h-6"/></svg>"#
            ))
        }
        _ => return None,
    };
    Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"#))
}

#[cfg(test)]
mod tests {
    use super::svg;

    #[test]
    fn each_mark_is_one_document_in_the_colour_asked_and_nothing_else_is_a_brand() {
        for mark in ["tailscale", "dropbox", "localsend", "flea"] {
            let doc = svg(mark, "#123456").unwrap();
            assert!(doc.starts_with("<svg") && doc.ends_with("</svg>") && doc.contains("#123456"), "{mark}");
            assert!(roxmltree::Document::parse(&doc).is_ok(), "{mark} is not well formed");
        }
        assert_eq!(svg("tailscale", "#000").unwrap().matches("<circle").count(), 9);
        assert_eq!(svg("tailscale", "#000").unwrap().matches(r#"fill-opacity="1""#).count(), 4);
        assert_eq!(svg("dropbox", "#000").unwrap().matches('Z').count(), 5);
        assert_eq!(svg("localsend", "#000").unwrap().matches('A').count(), 8);
        assert!(svg("file", "#000").is_none());
    }
}

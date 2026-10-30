// Which columns a list row of a given width draws, ui/js/Columns.js, and the row height a density
// stop gives it. The name is the one column a file manager cannot do without, so it never loses:
// each metadata column drops, widest first, at the width where keeping it would push the name under
// its floor, and the header and every row resolve their set from the same width. Toolkit free.
use crate::{DATE_W, ICON_SLOT, KIND_W, MODE_W, NAME_MIN, PAD_X, SIZE_W};

// Theme.spacing.gap, the space between two cells.
pub const GAP: f32 = 8.0;
// The optional columns in the order they drop: Kind first, the most redundant and the widest; Mode
// last, the permanent column. Items sits with them in QML and arrives with the folder walk.
const DROP_ORDER: [&str; 4] = ["kind", "date", "size", "mode"];
// src/uischema.rs's optional column keys, Items among them, in the header menu's own order.
pub const OPTIONAL: [&str; 5] = ["mode", "size", "items", "date", "kind"];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cols {
    pub mode: bool,
    pub size: bool,
    pub date: bool,
    pub kind: bool,
}

fn width_of(key: &str) -> f32 {
    match key {
        "mode" => MODE_W,
        "size" => SIZE_W,
        "date" => DATE_W,
        _ => KIND_W,
    }
}

// What the name's slot needs at its floor: the padding either side, the mark and the gap after it.
fn name_floor() -> f32 {
    2.0 * PAD_X + ICON_SLOT + GAP + NAME_MIN
}

// ViewState.hiddenCols: the optional columns `columns` does not name.
pub fn hidden(columns: &[String]) -> Vec<&'static str> {
    OPTIONAL.iter().copied().filter(|k| !columns.iter().any(|c| c == k)).collect()
}

// Columns.set: a floor includes every column that outlives it, so the floors nest; a hidden column
// never draws, and width still wins over one the user wants back.
pub fn set(width: f32, hidden: &[&str]) -> Cols {
    let mut running = name_floor();
    let mut out = Cols::default();
    for key in DROP_ORDER.iter().rev() {
        running += width_of(key) + GAP;
        let on = width >= running && !hidden.contains(key);
        match *key {
            "mode" => out.mode = on,
            "size" => out.size = on,
            "date" => out.date = on,
            _ => out.kind = on,
        }
    }
    out
}

// Columns.dualSet: the split's half-width panes carry Size and Modified and no more.
pub fn dual_set(width: f32, hidden: &[&str]) -> Cols {
    let base = name_floor();
    let size = !hidden.contains(&"size") && width >= base + SIZE_W;
    let date = !hidden.contains(&"date") && width >= base + if size { SIZE_W } else { 0.0 } + DATE_W;
    Cols { size, date, ..Cols::default() }
}

// ViewState.toggleColumn: the column leaves the stored set, or joins it at the end.
pub fn toggled(columns: &[String], key: &str) -> Vec<String> {
    let mut out: Vec<String> = columns.to_vec();
    match out.iter().position(|c| c == key) {
        Some(at) => {
            out.remove(at);
        }
        None => out.push(key.to_string()),
    }
    out
}

// Theme.fileRowHeight: the body's line box and the row padding, which the density stop scales.
pub fn row_height(density: &str) -> f32 {
    let ratio = match density {
        "compact" => 0.5,
        "comfortable" => 1.5,
        _ => 1.0,
    };
    // JavaScript's Math.round takes a half up, which f32::round does too for these positive values.
    ICON_SLOT + 2.0 * (7.0 * ratio as f32).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_keeps_its_floor_and_the_columns_drop_widest_first() {
        let all = set(2000.0, &[]);
        assert_eq!(all, Cols { mode: true, size: true, date: true, kind: true });
        let floor = name_floor();
        let at = |w: f32| set(w, &[]);
        assert_eq!(at(floor + MODE_W + GAP), Cols { mode: true, ..Default::default() });
        assert_eq!(at(floor + MODE_W + SIZE_W + DATE_W + 3.0 * GAP), Cols { mode: true, size: true, date: true, kind: false });
        assert_eq!(at(floor), Cols::default());
        assert_eq!(set(2000.0, &["mode", "kind"]), Cols { size: true, date: true, ..Default::default() });
    }

    #[test]
    fn the_stored_set_names_what_shows_and_a_toggle_adds_at_the_end() {
        let shipped: Vec<String> = ["name", "size", "date"].map(String::from).to_vec();
        assert_eq!(hidden(&shipped), ["mode", "items", "kind"]);
        assert_eq!(toggled(&shipped, "size"), ["name", "date"]);
        assert_eq!(toggled(&shipped, "kind"), ["name", "size", "date", "kind"]);
        assert_eq!(dual_set(2000.0, &["date"]), Cols { size: true, ..Default::default() });
    }

    #[test]
    fn density_scales_only_the_padding() {
        assert_eq!((row_height("compact"), row_height("normal"), row_height("comfortable")), (31.0, 37.0, 45.0));
        assert_eq!(row_height("anything"), 37.0);
    }
}

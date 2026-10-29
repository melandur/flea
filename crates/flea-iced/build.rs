// Generates the row-mark tables from the files that already own them, so nothing is copied by hand:
//   ui/js/Icons.js PATHS        the lucide marks, Omarchy cut (AGENTS.md "Lucide path data")
//   ui/js/FileTypes.js          the mark families and exact names (AGENTS.md "The filetype tier")
//   vendor/yazi-icons.toml      the colours, with tools/flea-filetypes-gen's own precedence
//   vendor/strata-themes.toml   the theme catalog, with tools/flea-themes-gen's own eight tokens
// While ui/ ships, those two JS files are the source; the port's last stage moves their tables into
// data files beside vendor/ and points this at them.
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn read(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    println!("cargo:rerun-if-changed={}", path.display());
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("build.rs: cannot read {}: {e}", path.display()))
}

// The quoted strings of one line, in order.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

// `var NAME = {` ... `}`, as (key, value) pairs in source order, with `+ "..."` continuation lines
// appended to the value before them.
fn js_table(body: &str, name: &str) -> Vec<(String, String)> {
    let start = body.find(&format!("var {name} = {{")).unwrap_or_else(|| panic!("build.rs: no table {name}"));
    let mut out: Vec<(String, String)> = vec![];
    // A one-line table, `var X = { "a": "b" }`.
    let first = body[start..].lines().next().unwrap();
    if first.trim_end().ends_with('}') {
        let q = quoted(first);
        out.extend(q.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())));
        assert!(!out.is_empty(), "build.rs: {name} came back empty");
        return out;
    }
    for line in body[start..].lines().skip(1) {
        let t = line.trim();
        if t.starts_with('}') {
            break;
        }
        if t.starts_with("//") || t.is_empty() {
            continue;
        }
        let q = quoted(t);
        if t.starts_with('+') {
            let last = out.last_mut().unwrap_or_else(|| panic!("build.rs: {name} opens with a continuation"));
            last.1.push_str(q.first().copied().unwrap_or(""));
        } else if q.len() >= 2 {
            out.push((q[0].to_string(), q[1].to_string()));
        }
    }
    assert!(!out.is_empty(), "build.rs: {name} came back empty, so the table would ship empty");
    out
}

// FileTypes.js fill(): the first family to claim a key keeps it.
fn marks(table: &[(String, String)], rename: &HashMap<&str, &str>) -> Vec<(String, String)> {
    let mut seen: HashMap<String, String> = HashMap::new();
    for (family, words) in table {
        let mark = rename.get(family.as_str()).copied().unwrap_or(family);
        for w in words.split(' ').filter(|w| !w.is_empty()) {
            seen.entry(w.to_string()).or_insert_with(|| mark.to_string());
        }
    }
    let mut v: Vec<_> = seen.into_iter().collect();
    v.sort();
    v
}

// tools/flea-filetypes-gen packs one line per colour in sorted hex order, and FileTypeColors.js
// assigns in that order, so for a key two colours claim the higher hex wins. Same here.
fn colours(toml: &str) -> [Vec<(String, String)>; 3] {
    let mut rules: Vec<(String, usize, String)> = vec![];
    for (kind, key) in ["exts", "files", "dirs"].iter().enumerate() {
        let open = toml
            .lines()
            .position(|l| l.trim_start().starts_with(key) && l.contains('['))
            .unwrap_or_else(|| panic!("build.rs: no {key} section in vendor/yazi-icons.toml"));
        let before = rules.len();
        for line in toml.lines().skip(open + 1) {
            if line.trim() == "]" {
                break;
            }
            let (Some(n), Some(f)) = (line.find("name = \""), line.find("fg = \"")) else { continue };
            let name = &line[n + 8..];
            let name = &name[..name.find('"').unwrap()];
            let fg = &line[f + 6..f + 13];
            rules.push((fg.to_lowercase(), kind, name.to_lowercase()));
        }
        assert!(rules.len() > before, "build.rs: no {key} rules, so the table would ship empty");
    }
    rules.sort_by(|a, b| a.0.cmp(&b.0));
    let mut maps: [HashMap<String, String>; 3] = Default::default();
    for (fg, kind, name) in rules {
        maps[kind].insert(name, fg);
    }
    maps.map(|m| {
        let mut v: Vec<_> = m.into_iter().collect();
        v.sort();
        v
    })
}

// tools/flea-themes-gen MAPPING: the Strata token each colors.toml key is read from, in KEYS order.
const THEME_MAPPING: [(&str, &str); 8] = [
    ("background", "background"),
    ("surface", "dark_background"),
    ("text", "foreground"),
    ("accent", "accent"),
    ("danger", "red"),
    ("dim_text", "muted"),
    ("syntax_string", "green"),
    ("syntax_preprocessor", "cyan"),
];

// One theme per [[themes]] block, in source order, with the generator's own refusals: an id that is
// not [a-z0-9-]+, a name a packed line cannot hold, any of the eight colours missing, a repeated id.
fn themes(toml: &str) -> String {
    let mut out = String::from("pub static THEME_KEYS: [&str; 8] = [");
    out += &THEME_MAPPING.iter().map(|(_, k)| format!("{k:?}")).collect::<Vec<_>>().join(", ");
    out += "];\npub static THEMES: &[(&str, &str, [&str; 8])] = &[\n";
    let mut seen = std::collections::HashSet::new();
    for block in toml.split("[[themes]]").skip(1) {
        let mut entry: HashMap<String, String> = HashMap::new();
        for line in block.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            if !k.chars().all(|c| c.is_ascii_lowercase() || c == '_') || !v.starts_with('"') {
                continue;
            }
            entry.insert(k.to_string(), v.trim_matches('"').to_string());
        }
        let id = entry.get("id").cloned().unwrap_or_default();
        assert!(!id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "build.rs: theme id {id:?}");
        let name = entry.get("name").cloned().unwrap_or_default();
        assert!(!name.is_empty() && !name.contains('|'), "build.rs: theme {id} name {name:?}");
        assert!(seen.insert(id.clone()), "build.rs: theme id {id} appears twice");
        let colours: Vec<String> = THEME_MAPPING
            .iter()
            .map(|(token, _)| {
                let v = entry.get(*token).cloned().unwrap_or_default().to_lowercase();
                assert!(v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()), "build.rs: theme {id} has no usable {token}");
                format!("{v:?}")
            })
            .collect();
        writeln!(out, "    ({id:?}, {name:?}, [{}]),", colours.join(", ")).unwrap();
    }
    assert!(!seen.is_empty(), "build.rs: no [[themes]] entries");
    out + "];\n"
}

fn emit(out: &mut String, name: &str, rows: &[(String, String)]) {
    writeln!(out, "pub static {name}: &[(&str, &str)] = &[").unwrap();
    for (k, v) in rows {
        writeln!(out, "    ({k:?}, {v:?}),").unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let icons = read(&root, "ui/js/Icons.js");
    let filetypes = read(&root, "ui/js/FileTypes.js");
    let toml = read(&root, "vendor/yazi-icons.toml");
    let strata = read(&root, "vendor/strata-themes.toml");

    let mut paths = js_table(&icons, "PATHS");
    paths.sort();
    let rename = js_table(&filetypes, "FAMILY_MARK");
    let rename: HashMap<&str, &str> = rename.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let ext_marks = marks(&js_table(&filetypes, "FAMILIES"), &rename);
    let name_marks = marks(&js_table(&filetypes, "NAMED"), &rename);
    // A family whose mark the PATHS table does not carry would draw the file mark in silence,
    // which is the trap tests/js/filetypes.js guards in QML; here it fails the build.
    for (_, mark) in ext_marks.iter().chain(&name_marks) {
        assert!(paths.binary_search_by(|(k, _)| k.as_str().cmp(mark)).is_ok(), "build.rs: no path for mark {mark}");
    }
    let [ext_colours, name_colours, dir_colours] = colours(&toml);

    let mut out = String::from("// GENERATED by build.rs. Every table is sorted by key for a binary search.\n");
    emit(&mut out, "PATHS", &paths);
    emit(&mut out, "EXT_MARKS", &ext_marks);
    emit(&mut out, "NAME_MARKS", &name_marks);
    emit(&mut out, "EXT_COLOURS", &ext_colours);
    emit(&mut out, "NAME_COLOURS", &name_colours);
    emit(&mut out, "DIR_COLOURS", &dir_colours);
    out += &themes(&strata);
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tables.rs");
    std::fs::write(dest, out).unwrap();
}

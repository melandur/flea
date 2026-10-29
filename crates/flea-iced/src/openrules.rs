// Settings > File types, ui/js/SettingsOpen.js: which application an ENDING opens in, the operator's
// own table and the one thing the desktop database cannot hold (AGENTS.md "Which application an
// ENDING opens in"). src/openrules.rs is the same contract in Rust and the one an open goes through;
// this is the panel's half: the shape a rule must have, the table read off the document, and the
// whole table written back, because src/uistate.rs refuses a list that holds one bad rule.
use crate::settings::{group, hint, row, Kind, Row};
use serde_json::{json, Value};

const MAX_ENDING: usize = 32;
const MAX_APP: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub id: String,
    pub label: String,
}

// SettingsOpen.normalised: trimmed, lower-cased, and given its leading dot.
pub fn normalised(text: &str) -> String {
    let t = text.trim().to_lowercase();
    if t.is_empty() || t.starts_with('.') { t } else { format!(".{t}") }
}

pub fn is_ending(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('.') else { return false };
    if value.len() < 2 || value.len() > MAX_ENDING || rest.starts_with('.') || rest.ends_with('.') || rest.contains("..") {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-+_".contains(c))
}

pub fn is_app(id: &str) -> bool {
    id.len() > ".desktop".len() && id.len() <= MAX_APP && id.ends_with(".desktop") && !id.contains('/') && !id.starts_with('.')
}

// The stored table, every rule that is still the shape a rule must have.
pub fn rules(doc: &Value) -> Vec<(String, String)> {
    doc.pointer("/open/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r.get("ends")?.as_str()?.to_string(), r.get("app")?.as_str()?.to_string())))
        .filter(|(e, a)| is_ending(e) && is_app(a))
        .collect()
}

fn encode(table: &[(String, String)]) -> Value {
    json!({ "open": { "rules": table.iter().map(|(e, a)| json!({ "ends": e, "app": a })).collect::<Vec<_>>() } })
}

// SettingsOpen.withRule: one rule per ending, the new one replacing an old one for the same ending,
// the table kept in ending order. None for an ending or an application that is not the shape.
pub fn with_rule(doc: &Value, ends: &str, app: &str) -> Option<Value> {
    let ending = normalised(ends);
    if !is_ending(&ending) || !is_app(app) {
        return None;
    }
    let mut table: Vec<(String, String)> = rules(doc).into_iter().filter(|(e, _)| *e != ending).collect();
    table.push((ending, app.to_string()));
    table.sort_by(|a, b| a.0.cmp(&b.0));
    Some(encode(&table))
}

pub fn without(doc: &Value, index: usize) -> Value {
    let table: Vec<(String, String)> = rules(doc).into_iter().enumerate().filter(|(i, _)| *i != index).map(|(_, r)| r).collect();
    encode(&table)
}

// The application's own Name=, or its id without .desktop while the catalogue has not answered.
pub fn label_for(apps: &[App], id: &str) -> String {
    apps.iter().find(|a| a.id == id).map(|a| a.label.clone()).unwrap_or_else(|| id.trim_end_matches(".desktop").to_string())
}

// OpenWith.matching, as written: each label is lower-cased and the typed text is not, so a search
// typed with a capital letter matches nothing there, and nothing here either.
pub fn matching<'a>(apps: &'a [App], needle: &str) -> Vec<&'a App> {
    apps.iter().filter(|a| needle.is_empty() || a.label.to_lowercase().contains(needle)).collect()
}

pub fn rows(doc: &Value, apps: &[App]) -> Vec<Row> {
    let table = rules(doc);
    let mut out = vec![Row { id: "addOpenRule".into(), action: Some("addOpenRule"), value: "Add rule".into(), ..group("Endings") }];
    for (i, (ends, app)) in table.iter().enumerate() {
        out.push(Row { value: label_for(apps, app), glyph: Some("app-window"), caption: app.clone(), ..row(Kind::OpenRule, &format!("openrule:{i}"), ends) });
    }
    out.push(hint(if table.is_empty() {
        "No endings yet. A rule sends one ending to one application, whatever the desktop's own default for that file type is."
    } else {
        "The longest ending wins, so a rule for .nii.gz opens before one for .gz. Everything with no rule opens the way the desktop says."
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ending_is_normalised_and_held_to_its_shape() {
        assert_eq!(normalised(" NII.GZ "), ".nii.gz");
        assert_eq!(normalised(".Mha"), ".mha");
        assert!(is_ending(".nii.gz") && is_ending(".c++"));
        for bad in [".", "..gz", ".gz.", ".a..b", "gz", ".a b", ".UPPER"] {
            assert!(!is_ending(bad), "{bad}");
        }
        assert!(is_app("itksnap.desktop") && !is_app(".desktop") && !is_app("a/b.desktop") && !is_app("x.app"));
    }

    #[test]
    fn a_rule_replaces_its_endings_old_one_and_the_table_stays_in_ending_order() {
        let doc = json!({ "open": { "rules": [{ "ends": ".nii", "app": "a.desktop" }, { "ends": "bad", "app": "b.desktop" }] } });
        assert_eq!(rules(&doc), vec![(".nii".to_string(), "a.desktop".to_string())]);
        let next = with_rule(&doc, "MHA", "itksnap.desktop").unwrap();
        assert_eq!(next, json!({ "open": { "rules": [{ "ends": ".mha", "app": "itksnap.desktop" }, { "ends": ".nii", "app": "a.desktop" }] } }));
        let replaced = with_rule(&doc, ".nii", "b.desktop").unwrap();
        assert_eq!(replaced["open"]["rules"].as_array().unwrap().len(), 1);
        assert!(with_rule(&doc, "..", "a.desktop").is_none());
        assert_eq!(without(&doc, 0), json!({ "open": { "rules": [] } }));
    }

    #[test]
    fn the_search_keeps_the_original_case_quirk() {
        let apps = vec![App { id: "gimp.desktop".into(), label: "GIMP".into() }];
        assert_eq!(matching(&apps, "gim").len(), 1);
        assert_eq!(matching(&apps, "GIM").len(), 0);
        assert_eq!(label_for(&apps, "gimp.desktop"), "GIMP");
        assert_eq!(label_for(&apps, "x.desktop"), "x");
    }
}

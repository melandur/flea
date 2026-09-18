.pragma library

// Settings > File types: the rows the section draws and the shape a rule must have before the panel
// writes one. src/openrules.rs is the same contract in Rust, and it is the one that decides at open
// time; this half exists so the dialog can refuse a bad ending while the caret is still in it rather
// than writing a key the backend will then drop on the next read.
//
// The section exists because the desktop database keys on a sniffed type and an ending is not a
// type: brain.nii and brain.nii.gz are one NIfTI volume and one gzip stream to `gio open`, so the
// Open with dialog's own "always" box cannot send the pair to two different applications, or send
// the compressed one to a viewer without claiming every other .gz on the box with it.

// The same two ceilings src/openrules.rs holds, for the same reasons.
var MAX_ENDING = 32
var MAX_APP = 128

// What the operator typed, as it will be stored: folded, trimmed, and wearing the dot it may have
// left out, because "nii.gz" and ".nii.gz" are one ending and only one of them can be the stored form.
function normalised(text) {
    var trimmed = String(text === undefined || text === null ? "" : text).trim().toLowerCase()
    if (trimmed.length === 0)
        return ""
    return trimmed.charAt(0) === "." ? trimmed : "." + trimmed
}

// A leading dot, then lowercase name parts with single dots between them; never a path, never a
// space, never a trailing dot. is_ending in src/openrules.rs, character for character.
function isEnding(text) {
    var value = String(text || "")
    if (value.charAt(0) !== "." || value.length < 2 || value.length > MAX_ENDING)
        return false
    var rest = value.substring(1)
    // No empty part anywhere: "..nii" is the parent directory's own spelling, not an ending.
    if (rest.charAt(0) === "." || rest.charAt(rest.length - 1) === "." || rest.indexOf("..") >= 0)
        return false
    return /^[a-z0-9.\-+_]+$/.test(rest)
}

// A desktop entry id and nothing that could be read as a path; is_app in src/openrules.rs.
function isApp(id) {
    var value = String(id || "")
    return value.length > ".desktop".length && value.length <= MAX_APP
        && value.indexOf(".desktop", value.length - ".desktop".length) >= 0
        && value.indexOf("/") < 0 && value.charAt(0) !== "."
}

// The stored table, with anything that is not a whole rule dropped: a hand-edited file or one from a
// newer Flea draws the rules this build understands instead of an empty section or a broken row.
function rules(state) {
    var stored = ((state || {}).open || {}).rules || []
    var out = []
    for (var i = 0; i < stored.length; i++) {
        var rule = stored[i] || {}
        if (isEnding(rule.ends) && isApp(rule.app))
            out.push({ ends: rule.ends, app: rule.app })
    }
    return out
}

// The application's own name where the catalogue has been read, and its id until then: a row that
// said nothing while the walk was running would read as a rule pointing at nothing.
function labelFor(apps, id) {
    for (var i = 0; i < (apps || []).length; i++) {
        if (apps[i].id === id)
            return apps[i].label
    }
    return String(id || "").replace(/\.desktop$/, "")
}

// One ending holds one application, so writing a rule for an ending already in the table replaces
// it rather than adding a second row the match would then have to break a tie between. Sorted by
// ending so the list does not reorder itself as rules are added.
function withRule(current, ends, app) {
    var ending = normalised(ends)
    if (!isEnding(ending) || !isApp(app))
        return null
    var out = []
    for (var i = 0; i < (current || []).length; i++) {
        if (current[i].ends !== ending)
            out.push({ ends: current[i].ends, app: current[i].app })
    }
    out.push({ ends: ending, app: String(app) })
    out.sort(function (left, right) { return left.ends < right.ends ? -1 : left.ends > right.ends ? 1 : 0 })
    return out
}

function without(current, index) {
    var out = []
    for (var i = 0; i < (current || []).length; i++) {
        if (i !== index)
            out.push({ ends: current[i].ends, app: current[i].app })
    }
    return out
}

// The application the table names for one file NAME, or "" when it names none: longest ending first,
// so .nii.gz answers before .gz however the two were entered, and folded, so a volume a scanner
// wrote out as VOLUME.NII.GZ opens where its lowercase twin does. chosen() in src/openrules.rs is
// the same function, and it is the one the open itself goes through; this one is here so ui/js/Nav.js
// can tell an ending the operator has claimed from one the archive route may still take.
function chosen(state, name) {
    var folded = String(name || "").toLowerCase()
    var table = rules(state)
    var best = ""
    var longest = 0
    for (var i = 0; i < table.length; i++) {
        var ends = table[i].ends
        // A name that IS the ending is a dotfile, ".nii" itself, and not a volume called nothing.
        if (folded.length <= ends.length || folded.indexOf(ends, folded.length - ends.length) < 0)
            continue
        if (ends.length > longest) {
            longest = ends.length
            best = table[i].app
        }
    }
    return best
}

// The section's rows. The heading carries the one action that governs the whole group, SettingsRest
// rule 3's slot, and the hint under an empty table is what says what the section is for at all.
function rows(state, apps) {
    var table = rules(state)
    var out = [{ kind: "group", label: "Endings", id: "addOpenRule", action: "addOpenRule", value: "Add rule" }]
    for (var i = 0; i < table.length; i++) {
        out.push({ kind: "openrule", id: "openrule:" + i, label: table[i].ends,
                   value: labelFor(apps, table[i].app), glyph: "app-window", ruleIndex: i,
                   app: table[i].app })
    }
    if (table.length === 0)
        out.push({ kind: "hint", label: "No endings yet. A rule sends one ending to one application, whatever the desktop's own default for that file type is." })
    else
        out.push({ kind: "hint", label: "The longest ending wins, so a rule for .nii.gz opens before one for .gz. Everything with no rule opens the way the desktop says." })
    return out
}

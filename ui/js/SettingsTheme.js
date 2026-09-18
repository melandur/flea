.pragma library
.import "Themes.js" as Themes

// The Settings panel's Theme section: Strata's 95 vendored palettes plus the one row that is not a
// palette at all, Follow Omarchy, which is the default and what Flea has always done.
//
// It lives beside Settings.js the way SettingsShelf.js does, and for the same reason: the section
// is one job and Settings.js is at its file budget. The catalog itself is ui/js/Themes.js's, which
// tools/flea-themes-gen writes; nothing here knows a colour.
//
// Every row is a plain `check`, not a new row kind. A check writes through ViewState.changeSetting,
// and a theme row's id carries the palette it selects ("theme:<id>"), which that writer answers
// with a leaf write rather than a toggle: choosing a row is choosing that theme, and there is no
// off. That keeps 95 rows off ui/SettingsRow.qml and ui/SettingsPanel.qml entirely, both of which
// are over their own caps already.
function rows(themeId) {
    var chosen = Themes.resolve(themeId)
    var out = [
        { kind: "group", label: "Theme" },
        { kind: "check", id: "theme:" + Themes.FOLLOW, label: "Follow Omarchy", glyph: "sliders",
          on: chosen === Themes.FOLLOW,
          value: chosen === Themes.FOLLOW ? "the desktop's own" : "" },
        // HANDOFF rule 8: one short line. The switch's own label says the rest.
        { kind: "hint", label: "Every window follows omarchy-theme-set until you pick one below." },
        { kind: "group", label: "Bundled palettes", value: Themes.entries().length + " themes" }
    ]
    var entries = Themes.entries()
    for (var i = 0; i < entries.length; i++) {
        out.push({ kind: "check", id: "theme:" + entries[i].id, label: entries[i].name,
                   glyph: "palette", on: chosen === entries[i].id })
    }
    // The catalog is Strata's and the row says so, because a shipped palette named after someone
    // else's project should name where it came from on the surface that offers it.
    out.push({ kind: "hint", label: "Vendored from Strata, MIT. Most are Tinted Base16 palettes." })
    return out
}

// What the rail row shows beside the section name: the palette in force, or the desktop.
function summary(themeId) {
    var chosen = Themes.resolve(themeId)
    return chosen === Themes.FOLLOW ? "Omarchy" : Themes.nameOf(chosen)
}

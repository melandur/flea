.import "../../ui/js/Themes.js" as Themes
.import "../../ui/js/SettingsTheme.js" as SettingsTheme
.import "../../ui/js/Palette.js" as Palette
.import "../../ui/js/Contrast.js" as Contrast

// Strata's vendored palette catalog. The suite beside it, tests/js/themes.js, reads the installed
// Omarchy themes off disk; this one reads nothing, because the catalog ships in the tree.

function run(check) {
    runCatalog(check)
    runBody(check)
    runRoles(check)
    runSection(check)
}

function runCatalog(check) {
    var entries = Themes.entries()
    check("the catalog is Strata's 95", entries.length, 95)
    check("every entry has an id and a name",
          entries.filter(function (e) { return !e.id || !e.name }).length, 0)
    // src/uistate.rs is_theme_id is what lets one of these through the state file, so every id the
    // generator emits has to be a shape that rule accepts, or a theme could be chosen and not saved.
    check("every id is the shape the state file accepts",
          entries.filter(function (e) { return !/^[a-z0-9-]+$/.test(e.id) || e.id.length > 64 }).length, 0)
    check("a known one is there", Themes.has("catppuccin-mocha"), true)
    check("and carries its display name", Themes.nameOf("catppuccin-mocha"), "Catppuccin Mocha")
    check("an id the catalog does not carry answers nothing", Themes.nameOf("nope"), "")

    // "omarchy" is the absence of a choice, not an entry, and resolve is the one place that decides.
    check("following is not a catalog entry", Themes.has(Themes.FOLLOW), false)
    check("so an unknown id resolves to following", Themes.resolve("nope"), "omarchy")
    check("as does an empty one", Themes.resolve(""), "omarchy")
    check("and following resolves to itself", Themes.resolve("omarchy"), "omarchy")
    check("a real id resolves to itself", Themes.resolve("tokyo-night"), "tokyo-night")
    check("following() agrees with resolve()", Themes.following("nope"), true)
    check("and says no for a palette it carries", Themes.following("catppuccin-mocha"), false)
}

// paletteBody is the whole seam: ui/Theme.qml hands it to the same applyColors an Omarchy
// colors.toml goes through, so what it emits has to parse as one.
function runBody(check) {
    var body = Themes.paletteBody("catppuccin-mocha")
    var found = Palette.parse(body)
    check("the body parses as a palette", Palette.isPalette(found), true)
    check("and carries all eight roles", Object.keys(found).length, 8)
    check("background is Strata's background", found.background, "#1e1e2e")
    check("surface lands on the rung Palette.js reads first", found.dark_background, "#181825")
    check("text becomes the foreground", found.foreground, "#cdd6f4")
    check("danger becomes red, which is what Color.urgent reads", found.red, "#f38ba8")
    // The load-bearing two: Flea's symlink and executable roles read cyan and green, and a Strata
    // theme has no ANSI ring. Its own themes.md records base0B as strings and base0C as
    // preprocessor, so those two syntax tokens ARE that ring's green and cyan under other names.
    check("syntax_string arrives as green", found.green, "#a6e3a1")
    check("syntax_preprocessor arrives as cyan", found.cyan, "#94e2d5")
    check("an id the catalog lacks emits no body at all", Themes.paletteBody("nope"), "")
    // The first rung of Palette.js's surface ladder is what a body must carry, or every catalog
    // theme would silently fall through to background and lose its chrome plane.
    check("the surface ladder's first rung is the key emitted",
          Palette.pick(found, Palette.SURFACE_KEYS, "#000000"), "#181825")
}

// A vendored palette goes through the same WCAG lifts an Omarchy one does, so the question worth
// asking is whether any of the 95 is so low-contrast that a lift cannot save it. It cannot: the
// lift walks toward black or white and always reaches the ratio. What IS worth asserting is that
// the catalog's own colours are real colours, because a "#000000" from a bad parse would lift to
// something legible and hide the breakage.
function runRoles(check) {
    var entries = Themes.entries()
    var bad = []
    var flat = []
    for (var i = 0; i < entries.length; i++) {
        var colors = Themes.colorsOf(entries[i].id)
        for (var key in colors) {
            if (!/^#[0-9a-f]{6}$/.test(colors[key]))
                bad.push(entries[i].id + "." + key)
        }
        // A theme whose ink is its own ground is one no lift can rescue into a readable row.
        if (colors.background === colors.foreground)
            flat.push(entries[i].id)
    }
    check("every colour in the catalog is a six-digit hex", bad.join(","), "")
    check("and no theme paints its ink in its own ground", flat.join(","), "")
    // The lift ui/Theme.qml runs on every role, checked on the one case it exists for: a colour
    // that starts below the floor comes back at or above it, on a catalog ground.
    var mocha = Themes.colorsOf("catppuccin-mocha")
    var lifted = Contrast.ensureRatio("#333333", mocha.background, 3)
    check("a colour under the floor is lifted to it",
          Contrast.ratio(lifted, mocha.background) >= 3, true)
    check("and one already over it is left alone",
          Contrast.ensureRatio(mocha.foreground, mocha.background, 3), mocha.foreground)
}

function runSection(check) {
    var rows = SettingsTheme.rows("omarchy")
    // Two headings, the follow row, two hints and the 95.
    check("the section is the catalog plus Follow Omarchy", rows.length, 100)
    check("it opens on the follow row", rows[1].id + "|" + rows[1].on, "theme:omarchy|true")
    check("which names what it does", rows[1].value, "the desktop's own")
    check("the catalog heading counts them", rows[3].value, "95 themes")
    // Every catalog row is a plain check whose id carries the palette, which is what lets
    // ui/ViewState.qml changeSetting write it with no new row kind on any panel surface.
    var checks = rows.filter(function (r) { return r.kind === "check" })
    check("every row is a check", checks.length, 96)
    check("and every id names its palette",
          checks.filter(function (r) { return r.id.indexOf("theme:") !== 0 }).length, 0)
    check("exactly one is on", checks.filter(function (r) { return r.on }).length, 1)

    var chosen = SettingsTheme.rows("catppuccin-mocha")
    var on = chosen.filter(function (r) { return r.kind === "check" && r.on })
    check("choosing a palette moves the one that is on", on.length + "|" + on[0].id,
          "1|theme:catppuccin-mocha")
    check("and the follow row is no longer it",
          chosen[1].on + "|" + chosen[1].value, "false|")
    // A stored id this build's catalog does not carry follows the desktop, and the section has to
    // agree with ui/Theme.qml about that or the panel would show nothing selected at all.
    var stale = SettingsTheme.rows("a-theme-from-a-newer-build")
    check("a stale id lands back on Follow Omarchy", stale[1].on, true)
    check("the rail summary names the palette in force",
          SettingsTheme.summary("tokyo-night"), "Tokyo Night")
    check("and names the desktop while following", SettingsTheme.summary("omarchy"), "Omarchy")
    check("a stale id summarises as following too",
          SettingsTheme.summary("a-theme-from-a-newer-build"), "Omarchy")
}

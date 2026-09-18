.pragma library
.import "TextSize.js" as TextSize
.import "SettingsTheme.js" as SettingsTheme

// The Settings panel's Display section, moved out of Settings.js when the 0.3.1 Appearance rows
// would have pushed that file past its 300-line hard cap. One job: the text size, the compositor's
// scale, and Appearance. The Theme catalog is its own section beside this one, SettingsTheme.js.

// The sentence beside Effective has a job only while the ruler cannot state the running size: in Follow the two differ whenever Omarchy's size is not one of the seven, and an override is always a stop.
function effectiveNote(follows, baseSize) {
    var nearest = TextSize.nearest(baseSize)
    return !follows ? "Your override." : nearest === baseSize ? "Omarchy's own size."
        : "Omarchy's own size. The ruler marks " + nearest + ", the nearest stop."
}

// The SettingsScale board's own division: Flea owns its text override and Omarchy owns the rest. The size follows the desktop until one of TextSize's seven stops is pinned, and the monitor scale and the corner rounding are the compositor's, drawn as the read-only facts they are.
function rows(state, marks) {
    var follows = TextSize.following(state.textSize)
    var out = [
        { kind: "group", label: "Text size" },
        { kind: "choice", id: "textMode", label: "Text size", glyph: "type",
          labels: ["Follow Omarchy", "Override"],
          value: follows ? "Follow Omarchy" : "Override" },
        // The board's seven-stop ruler, the override's own control, now carrying the numbers it stands for; a size Omarchy invented that is not a stop marks the nearest one.
        { kind: "ruler", id: "textStop", stops: TextSize.STOPS, on: !follows,
          index: TextSize.STOPS.indexOf(TextSize.nearest(state.baseSize)) }
    ]
    // SettingsRest rule 2 keeps only the chords, and HANDOFF rule 8 keeps them to the one line the panel can draw: the board's own sentence wrapped onto two at this width.
    out.push({ kind: "hint", label: "Ctrl+Shift +/- walks them, Ctrl+Shift+0 follows." })
    // And Effective stays: it is state.baseSize, the size actually running, where the ruler marks TextSize.nearest() and a tie takes the smaller stop, so a base of 13 marks 12.
    out.push({ kind: "fact", id: "textEffective", label: "Effective", role: "live",
               value: state.baseSize + " px", caption: effectiveNote(follows, state.baseSize) })
    out.push({ kind: "group", label: "Scale" })
    out.push({ kind: "fact", label: "Scale", glyph: "maximize",
               value: scaleLabel(state.monitorScale) })
    out.push({ kind: "hint",
               label: "Flea follows the compositor value and does not step or cycle it." })
    out.push({ kind: "group", label: "Appearance" })
    out.push({ kind: "check", id: "display.hyprlandIcons", label: "Hyprland-aware icons",
               mark: marks["display.hyprlandIcons"],
               on: ((state.data || {}).display || {}).hyprlandIcons === true })
    // 0.3.1. Off draws every row mark in the theme's own ink, which is what Flea did before; on
    // gives a row the mark and the colour its NAME earns, which no freedesktop icon name can say.
    // See AGENTS.md "The filetype tier"; ui/js/FileTypes.js is what answers both.
    out.push({ kind: "check", id: "display.fileTypeColors", label: "File type marks and colours",
               glyph: "palette",
               on: ((state.data || {}).display || {}).fileTypeColors === true })
    out.push({ kind: "hint", label: "Marks are Flea's own; the colours are yazi's table." })
    out.push({ kind: "group", label: "Palette" })
    // A fact and not a control: the palette is the Theme section's, and two surfaces writing one
    // setting is how they come to disagree. SettingsGrammar rule 5.
    out.push({ kind: "fact", id: "themeSection", label: "Theme", glyph: "palette", role: "live",
               value: SettingsTheme.summary(state.themeId) })
    out.push({ kind: "hint", label: "The Theme section carries the catalog." })
    return out
}

// The compositor's own number, as Hyprland writes it: 1.00 and 1.25. An unanswered query says so rather than reading as 1x, because a wrong number here looks exactly like a right one. The row carries no control, which is what says it cannot be changed; SettingsGrammar rule 5.
function scaleLabel(scale) {
    if (!(scale > 0))
        return "not reported"
    return (Math.round(scale * 100) / 100) + "x"
}

.pragma library

// The metrics contract as the app resolves it, one key=value per line in the Blueprint board's
// order. ui/Ipc.qml serves it as tokens() and tools/flea-metrics-gate diffs it.
//
// It came out of ui/Theme.qml when that file reached 399 of its 400-line hard cap and the 0.3.1
// palette work needed room, and it is one job: report the resolved tokens, as against resolve
// them. It reads a Theme and a Style rather than importing either, because a .pragma library
// cannot import a QML singleton, and because a pure function of two objects is one a test can
// drive with a literal.
//
// family is the resolved face, never the "monospace" alias, so the gate cannot pass on a box
// without the font. Adding a key here changes what the gate diffs: add it to the board too.
function lines(theme, style) {
    var t = {
        family: style.font.resolvedFamily,
        baseSize: theme.baseSize,
        body: theme.font.body,
        bodySmall: theme.font.bodySmall,
        caption: theme.font.caption,
        lineBoxRatio: theme.lineBoxRatio,
        rowPaddingX: theme.spacing.rowPaddingX,
        rowPaddingY: theme.spacing.rowPaddingY,
        gap: theme.spacing.gap,
        hairline: theme.spacing.hairline,
        rowHeight: theme.rowHeight,
        iconSize: theme.iconSize,
        markSize: theme.markSize,
        stateMarkSize: theme.stateMarkSize,
        heroMarkSize: theme.heroMarkSize,
        strokeWidth: theme.strokeWidth,
        railRowHeight: theme.railRowHeight,
        railIconSize: theme.railIconSize,
        chromeHeight: theme.chromeHeight,
        chromeMarkSize: theme.chromeMarkSize,
        columnMode: theme.column.mode,
        columnSize: theme.column.size,
        columnDate: theme.column.date,
        columnPickerDate: theme.column.pickerDate,
        columnKind: theme.column.kind,
        menuWidth: theme.menuWidth,
        cornerRadius: style.cornerRadius,
        previewFraction: theme.preview.fraction,
        gridIconSize: theme.grid.iconSize,
        gridMinCellWidth: theme.grid.minCellWidth,
        settingsPanelWidth: theme.settings.panelWidth,
        settingsRailWidth: theme.settings.railWidth,
        settingsPaneWidth: theme.settings.paneWidth,
        settingsIndent: theme.settings.indent,
        settingsRailPaddingY: theme.settings.railPaddingY
    }
    var out = []
    for (var key in t)
        out.push(key + "=" + t[key])
    return out.join("\n")
}

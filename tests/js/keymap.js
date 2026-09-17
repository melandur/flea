.import "../../ui/js/Keymap.js" as Keymap

// Default is the one preset, and its listing table is the arrows, the Ctrl chords and a handful of
// named keys. Everything this file asserts is either in that set or is asserted to be gone: a key
// that quietly stops working is the failure mode a keymap has, and nothing else notices it.
function run(check) {
    var none = Qt.NoModifier, ctrl = Qt.ControlModifier, shift = Qt.ShiftModifier
    var alt = Qt.AltModifier, meta = Qt.MetaModifier
    function key(name, text, mods, expected, context, frontend) {
        check("default " + (context || "listing") + " " + (frontend || "gui") + " " + name + " " + text + " " + mods,
              Keymap.lookupFor("default", Qt["Key_" + name] || 0, text, mods, context, frontend), expected)
    }

    check("default is the only preset", Keymap.PRESETS.join(","), "default")

    // The whole listing table, one row per key it binds.
    key("Down", "", none, "cursorDown")
    key("Up", "", none, "cursorUp")
    key("Left", "", none, "parent")
    key("Right", "", none, "pageForward")
    key("Return", "", none, "open")
    key("Enter", "", none, "open")
    key("Space", " ", none, "preview")
    key("Tab", "", none, "focusNext")
    key("Escape", "", none, "escape")
    key("Delete", "", none, "trash")
    key("F2", "", none, "rename")
    key("C", "", ctrl, "copy")
    key("V", "", ctrl, "paste")
    key("X", "", ctrl, "cut")
    key("Z", "", ctrl, "undo")
    key("F", "", ctrl, "search")
    key("B", "", ctrl, "copypath")
    key("G", "", ctrl, "sidebar")
    key("N", "", ctrl | shift, "newFolder")
    key("M", "m", none, "menu")
    key("Menu", "", none, "menu")
    key("F10", "", shift, "menu")
    key("Question", "?", shift, "keymapSheet")
    key("Comma", ",", none, "settings")

    // And the keys that left with the vim spellings, the view chords and the tab chords. A sample
    // wide enough that a row creeping back in is caught rather than a spot check of three.
    var goneLetters = [["H", "h"], ["J", "j"], ["K", "k"], ["L", "l"], ["Y", "y"], ["X", "x"],
                       ["P", "p"], ["R", "r"], ["Z", "z"], ["V", "v"], ["S", "s"], ["F", "f"],
                       ["O", "o"], ["G", "g"], ["T", "t"], ["W", "w"], ["A", "a"], ["E", "e"],
                       ["D", "d"], ["Period", "."], ["Slash", "/"], ["Minus", "-"]]
    for (var g = 0; g < goneLetters.length; g++)
        key(goneLetters[g][0], goneLetters[g][1], none, "")
    key("H", "H", shift, "")
    key("L", "L", shift, "")
    key("G", "G", shift, "")
    key("S", "S", shift, "")
    key("Colon", ":", shift, "")
    key("Plus", "+", shift, "")
    var goneChords = [["1", ctrl], ["2", ctrl], ["3", ctrl], ["A", ctrl], ["D", ctrl], ["U", ctrl],
                      ["E", ctrl], ["L", ctrl], ["T", ctrl], ["N", ctrl], ["W", ctrl],
                      ["Tab", ctrl], ["Space", ctrl], ["PageUp", ctrl], ["PageDown", ctrl],
                      ["P", alt], ["Down", shift], ["Up", shift], ["Delete", shift],
                      ["Backspace", none], ["Home", none], ["End", none],
                      ["PageUp", none], ["PageDown", none], ["Q", alt], ["J", meta]]
    for (var c = 0; c < goneChords.length; c++)
        key(goneChords[c][0], "", goneChords[c][1], "")

    // The TUI keeps Insert for copy and paste, which is the one pair a terminal cannot deliver as
    // Ctrl+C and Ctrl+V, and the digits that pick a tab. Neither reaches the window.
    key("Insert", "", ctrl, "copy", "listing", "tui")
    key("Insert", "", shift, "paste", "listing", "tui")
    key("Insert", "", ctrl, "", "listing", "gui")
    key("1", "1", none, "tab1", "listing", "tui")
    key("9", "9", none, "tab9", "listing", "tui")
    key("1", "1", none, "", "listing", "gui")

    // The rail, the menus, the panels and the three preview kinds, which the listing table does not
    // reach at all: a shared row answers only the listing and the rail, so the rest is preset rows.
    var overlays = ["menu", "panel", "preview", "pdf", "media"]
    for (var o = 0; o < overlays.length; o++) {
        var context = overlays[o]
        key("Down", "", none, "cursorDown", context)
        key("Up", "", none, "cursorUp", context)
        key("Return", "", none, "open", context)
        key("Enter", "", none, "open", context)
        key("Space", " ", none, "preview", context)
        key("Escape", "", none, "escape", context)
        key("Tab", "", shift, "focusPrevious", context)
        // No letter reaches any of them now, and neither does a chord the listing lost.
        key("J", "j", none, "", context)
        key("K", "k", none, "", context)
        key("D", "d", none, "", context)
        key("X", "", ctrl, "", context)
        key("N", "", ctrl, "", context)
        key("Tab", "", ctrl, "", context)
    }
    key("Menu", "", none, "menu", "rail")
    key("Down", "", none, "cursorDown", "rail")
    key("Escape", "", none, "escape", "editor")
    key("Return", "", none, "", "editor")

    // The context menu steps on the arrows, and Right walks into a submenu where Left walks out.
    key("Right", "", none, "menuRight", "menu")
    key("Left", "", none, "parent", "menu")
    // Left leaves a preview of the plain kinds, where it named an action only the seeking kinds
    // answered and so did nothing at all; the two that seek keep it, having no other key for it.
    key("Left", "", none, "escape", "preview")
    key("Left", "", none, "seekBack", "pdf")
    key("Left", "", none, "seekBack", "media")
    key("Right", "", none, "seekForward", "preview")
    key("Right", "", none, "seekForward", "pdf")
    // MediaMute: m is the menu in the listing and the mute in a media preview, its two meanings.
    key("M", "m", none, "mute", "media")
    // The PDF's zoom and expand went with the bare letters; the strip's own controls are the route.
    key("Minus", "-", none, "", "pdf")
    key("Plus", "+", shift, "", "pdf")
    key("E", "e", none, "", "pdf")

    // The sheet advertises what is bound and nothing else, so it is read back off the same table.
    function capFor(action) {
        var listed = Keymap.sheetFor("default", "gui")
        for (var s = 0; s < listed.length; s++)
            if (listed[s].action === action) return listed[s].keys
        return ""
    }
    check("the sheet spells move with the arrow", capFor("cursorDown"), "down")
    check("the sheet spells parent with the arrow", capFor("parent"), "left")
    check("the sheet spells browse in with the arrow", capFor("pageForward"), "right")
    check("the sheet spells copy with its chord", capFor("copy"), "ctrl-c")
    check("the sheet spells the file path with its chord", capFor("copypath"), "ctrl-b")
    check("the sheet spells the sidebar with its chord", capFor("sidebar"), "ctrl-g")
    check("the sheet spells trash with the bare key", capFor("trash"), "delete")
    check("the sheet draws no row for a key that left", capFor("filter"), "")
    check("nor for the tab keys", capFor("tabNew"), "")

    Keymap.setPreset("unknown")
    check("an unknown stored preset resolves to Default", Keymap.preset, "default")
    // A menu row draws the key the operator presses. Every one of these is a chord now, and before
    // the hint rule took chords they all drew nothing while their rows went on claiming a key.
    check("Copy hints its chord", Keymap.hintFor("copy"), "ctrl-c")
    check("Paste hints its chord", Keymap.hintFor("paste"), "ctrl-v")
    check("Trash hints the bare key it answers to", Keymap.hintFor("trash"), "delete")
    check("Copy path hints its chord", Keymap.hintFor("copypath"), "ctrl-b")
    check("Rename hints its own key", Keymap.hintFor("rename"), "f2")
    check("menu-only actions invent no shortcut", Keymap.hintFor("emptyTrash"), "")
    check("and neither do the actions that lost their key", Keymap.hintFor("openTerminal"), "")

    check("the sheet is populated from effective current bindings", Keymap.SHEET.length > 15, true)
    var widestCap = 0, identifierLabel = ""
    var listed = Keymap.sheetFor("default", "gui")
    for (var r = 0; r < listed.length; r++) {
        if (listed[r].keys.length > widestCap) widestCap = listed[r].keys.length
        if (/[a-z][A-Z]/.test(listed[r].label)) identifierLabel = listed[r].label
        if (listed[r].keys.split(" / ").length > 2) identifierLabel = "too many spellings: " + listed[r].keys
    }
    check("no cap outgrows its half of the card", widestCap <= 18, true)
    check("no row prints an action id where its wording belongs", identifierLabel, "")

    // MediaMute rule 4: the preview's own mute key is listed once, under Look, while the listing
    // goes on advertising m as its menu key.
    var muteRows = 0, muteCap = ""
    for (var q = 0; q < listed.length; q++)
        if (listed[q].action === "mute") { muteRows++; muteCap = listed[q].keys }
    check("the sheet lists mute once", muteRows, 1)
    check("and lists it under its own key", muteCap, "m")
    check("while m still opens the menu in the listing", Keymap.lookupFor("default", 0, "m", 0, "listing", "gui"), "menu")
    check("the sheet group that claims it is Look", Keymap.SHEET_GROUPS.look.indexOf("mute") >= 0, true)

    check("pointer contract remains populated", Keymap.POINTER.length > 10, true)
    var effective = Keymap.bindingRows("default", "gui")
    check("every effective binding resolves to the action it advertises", effective.every(function (row) {
        return Keymap.lookupFor("default", row.keycode, row.text, row.mask, "listing", "gui") === row.action
    }), true)
    check("and the table is the size the strip left it", effective.length, 23)
}

.import "../../ui/js/Marks.js" as Marks
.import "filterfixture.js" as Fixture

// Marking rows over whatever the filter left drawn: ctrl+A, the shift range in both forms, and
// ctrl+click. Split out of tests/js/filter.js with ui/js/Marks.js; the stub pane is shared.

// A key event as ui/Pane.qml's two seams hand one over; the space row is what keys.toml binds
// toggleSelect to, so a release is resolved through the same table the press was.
function key(code, text, repeat) {
    return { key: code, text: text === undefined ? "" : text, modifiers: Qt.NoModifier,
             isAutoRepeat: repeat === true }
}

function run(check) {
    // The operator's 2026-09-18 ruling, "allow me to keep space pressed": the press paints the row
    // under the cursor and every row the cursor reaches before the release takes the same state.
    var sweep = Fixture.pane()
    sweep.cursorIndex = 1
    Marks.paintPress(sweep)
    check("the press marks the row it landed on", Fixture.picks(sweep), "1")
    for (var down = 2; down <= 4; down++) { sweep.cursorIndex = down; Marks.paintCursor(sweep) }
    check("and every row the cursor reaches while it is held", Fixture.picks(sweep), "1,2,3,4")
    Marks.released(key(Qt.Key_Space, " "))
    sweep.cursorIndex = 5
    Marks.paintCursor(sweep)
    check("the release ends it, so the next move marks nothing", Fixture.picks(sweep), "1,2,3,4")

    // The same gesture over marked rows clears them: the first press decides which way the sweep
    // paints, which is what makes a held key able to undo a run as well as make one.
    Marks.paintPress(sweep)
    check("a press on an unmarked row still marks", Fixture.picks(sweep), "1,2,3,4,5")
    sweep.cursorIndex = 4
    Marks.paintPress(sweep)
    check("a press on a marked row unmarks it", Fixture.picks(sweep), "1,2,3,5")
    for (var up = 3; up >= 1; up--) { sweep.cursorIndex = up; Marks.paintCursor(sweep) }
    check("and the sweep clears the run it moves over", Fixture.picks(sweep), "5")
    Marks.released(key(Qt.Key_Space, " "))

    // An arrow is lifted and pressed again all through a sweep, so only the painting key's own
    // release may end one; ending on any release painted the first row reached and then nothing.
    var held = Fixture.pane()
    Marks.paintPress(held)
    Marks.released(key(Qt.Key_Down))
    held.cursorIndex = 1
    Marks.paintCursor(held)
    check("an arrow's release does not end the sweep", Fixture.picks(held), "0,1")
    Marks.released(key(Qt.Key_Space, " ", true))
    held.cursorIndex = 2
    Marks.paintCursor(held)
    check("nor does the auto-repeat a held key produces", Fixture.picks(held), "0,1,2")
    Marks.released(key(Qt.Key_Space, " "))
    held.cursorIndex = 3
    Marks.paintCursor(held)
    check("the real release does", Fixture.picks(held), "0,1,2")

    // A cursor that has not moved is not a row to repaint: the press already answered for it, and
    // ui/Pane.qml calls paintCursor on every cursor write, not only on the ones a key made.
    var still = Fixture.pane()
    Marks.paintPress(still)
    Marks.paintCursor(still)
    Marks.paintCursor(still)
    check("a cursor that stayed put is painted once, not flipped", Fixture.picks(still), "0")
    Marks.released(key(Qt.Key_Space, " "))

    var all = Fixture.pane("scr")
    Marks.selectAll(all)
    check("select all takes what is drawn, never the rows the filter hid", Fixture.picks(all), "0,1,5,6")
    var allPlain = Fixture.pane()
    Marks.selectAll(allPlain)
    check("and with no filter it still takes the whole listing", Fixture.picks(allPlain), "0,1,2,3,4,5,6")

    var span = Fixture.pane("scr")
    span.cursorIndex = 6
    Marks.extendTo(span, 1)
    check("extending over a filtered view skips the rows it hid", Fixture.picks(span), "1,5,6")
    var spanPlain = Fixture.pane()
    spanPlain.cursorIndex = 3
    Marks.extendTo(spanPlain, 1)
    check("and with no filter it is still a plain range", Fixture.picks(spanPlain), "1,2,3")

    // Shift+J whole: the anchor latches, the cursor steps through what is drawn and the selection
    // follows. "o" keeps 2, 3, 5 and 6, so two steps from row 2 reach 5 and not 4.
    var chain = Fixture.pane("o")
    chain.cursorIndex = 2
    Marks.extend(chain, 1)
    Marks.extend(chain, 1)
    check("shift J walks the selection down the rows that are drawn", Fixture.picks(chain), "2,3,5")
    check("and the cursor is on the last of them", chain.cursorIndex, 5)
    check("and the anchor stayed where the chain started", chain.selectionAnchor, 2)
    check("and the version moved once per step", chain.selectionVersion, 2)
    var chainPlain = Fixture.pane()
    chainPlain.cursorIndex = 2
    Marks.extend(chainPlain, 1)
    check("with no filter it is still a plain one-row range", Fixture.picks(chainPlain), "2,3")

    // Ctrl+click whole: an empty set means the cursor row is the selection, so it joins first.
    var joined = Fixture.pane()
    joined.cursorIndex = 2
    Marks.toggleRow(joined, 4)
    check("ctrl click on another row keeps the cursor row in the selection", Fixture.picks(joined), "2,4")
    check("and the cursor moved to the clicked row", joined.cursorIndex, 4)
    check("and the anchor is the clicked row", joined.selectionAnchor, 4)
    var held = Fixture.pane()
    held.cursorIndex = 1
    held.selection.toggle(1)
    Marks.toggleRow(held, 4)
    check("a set already holding the cursor row keeps it, marked once", Fixture.picks(held), "1,4")
    var same = Fixture.pane()
    same.cursorIndex = 3
    Marks.toggleRow(same, 3)
    check("ctrl click on the cursor row with nothing selected marks it once", Fixture.picks(same), "3")
}

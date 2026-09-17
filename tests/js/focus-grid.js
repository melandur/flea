.import "../../ui/js/Grid.js" as Grid
.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "filterfixture.js" as Fixture

// The grid's own geometry: which cell a key means when the rows are tiles. Split out of
// tests/js/focus.js with ui/js/Grid.js, and driving the same stub pane the filter suites use.

function run(check) {
    var none = 0
    function key(code, text, modifiers) {
        return { key: code, text: text, modifiers: modifiers }
    }
    var gridPane = Fixture.pane()
    gridPane.viewMode = "grid"
    gridPane.cursorStride = 3
    gridPane.wrapAtEnds = true
    gridPane.cursorIndex = 2
    Focus.act("cursorDown", gridPane)
    check("grid j follows row-major order across a row boundary", gridPane.cursorIndex, 3)
    Focus.act("cursorUp", gridPane)
    check("grid k follows the previous item", gridPane.cursorIndex, 2)
    check("grid j is not a physical arrow", Grid.arrow(key(Qt.Key_J, "j", none), "cursorDown", gridPane), false)
    // No preset spells the arrows as letters any more, so Grid.spelled is false and the grid takes
    // neither letter; the four arrows below are the whole of its geometry. Issue 114 survives as
    // that one question, which a preset binding h again would answer for itself.
    check("the grid refuses h", Grid.arrow(key(Qt.Key_H, "h", none), "cursorLeft", gridPane), false)
    check("the grid refuses l", Grid.arrow(key(Qt.Key_L, "l", none), "cursorRight", gridPane), false)
    check("and Grid.spelled says why", Grid.spelled(), false)
    for (var move of [
        [Qt.Key_Right, "cursorRight", 2, 2], [Qt.Key_Left, "cursorLeft", 3, 3],
        [Qt.Key_Down, "cursorDown", 2, 5], [Qt.Key_Down, "cursorDown", 5, 5],
        [Qt.Key_Down, "cursorDown", 3, 6], [Qt.Key_Up, "cursorUp", 6, 3],
        [Qt.Key_Up, "cursorUp", 0, 0], [Qt.Key_Right, "cursorRight", 6, 6]
    ]) {
        gridPane.cursorIndex = move[2]
        Grid.arrow(key(move[0], "", none), move[1], gridPane)
        check("grid visual neighbour from " + move[2] + " with " + move[1], gridPane.cursorIndex, move[3])
    }
    // The arrows clamp at the row's edge, which is what issue 114 was about; a refusal moves
    // nothing, so both the answer and the cursor are asserted.
    for (var walk of [
        [Qt.Key_Left, "cursorLeft", 3, 3], [Qt.Key_Left, "cursorLeft", 4, 3],
        [Qt.Key_Right, "cursorRight", 5, 5], [Qt.Key_Right, "cursorRight", 4, 5]
    ]) {
        gridPane.cursorIndex = walk[2]
        check("the grid takes the arrow for " + walk[1] + " from " + walk[2],
              Grid.arrow(key(walk[0], "", none), walk[1], gridPane), true)
        check("and " + walk[1] + " from " + walk[2] + " leaves the cursor on " + walk[3],
              gridPane.cursorIndex, walk[3])
    }


    gridPane.cursorStride = 2
    gridPane.cursorIndex = 3
    Grid.arrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("grid arrows use the reflowed column count", gridPane.cursorIndex, 5)

    gridPane.filterQuery = "screen"
    gridPane.refresh()
    gridPane.cursorIndex = 0
    gridPane.cursorStride = 2
    Grid.arrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("filtered grid arrows address visible cells", gridPane.cursorIndex, 6)
    Grid.arrow(key(Qt.Key_Right, "", none), "cursorRight", gridPane)
    check("filtered final row has no right cell", gridPane.cursorIndex, 6)
}

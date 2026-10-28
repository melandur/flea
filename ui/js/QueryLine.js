.pragma library

// What the search's and the filter's query lines share, the operator's rulings of 2026-10-28. Both
// draw the query as a Text with no editor behind it, so neither gets Qt's own text handling: which
// key is a typed character, and the system clipboard, are decided here once for both.

// One character a line may take. Delete reports its text as DEL (U+007F) and a Ctrl or Meta chord
// carries a control character or a letter it does not mean, so neither is typed. Alt is left alone,
// because some layouts reach printable characters through it.
function printable(event) {
    return event.text.length === 1 && event.text >= " " && event.text !== "\u007f"
        && (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier)) === 0
}

// No name is longer than this, so nothing past it can match and a pasted blob stops here.
var PASTE_MAX = 255

// A query is one line: a trailing line break goes, an inner one or a tab is a space, any other
// control character goes. The spaces the clipboard holds are kept, a leading one included.
function oneLine(text) {
    return String(text || "").replace(/[\r\n]+$/, "").replace(/[\r\n\t]+/g, " ")
        .replace(/[\u0000-\u001f\u007f]/g, "").substring(0, PASTE_MAX)
}

// root.opener is ui/Opener.qml: pasteText reads the clipboard, copyText hands it to wl-copy the way
// the menu's Copy Path does. typed is the line's own append, so a paste schedules the live walk or
// narrows the rows exactly as typing the same characters would. Off the line the same chords stay
// Flea's file copy and paste, because only a line holding the caret calls this.
function clipKey(event, root, query, typed) {
    var ctrl = (event.modifiers & Qt.ControlModifier) !== 0
    var shift = (event.modifiers & Qt.ShiftModifier) !== 0
    if ((ctrl && event.key === Qt.Key_V) || (shift && event.key === Qt.Key_Insert)) {
        var text = oneLine(root.opener ? root.opener.pasteText() : "")
        if (text.length > 0) typed(text)
        return true
    }
    if (ctrl && (event.key === Qt.Key_C || event.key === Qt.Key_Insert)) {
        if (query.length > 0 && root.opener) root.opener.copyText(query)
        return true
    }
    return false
}

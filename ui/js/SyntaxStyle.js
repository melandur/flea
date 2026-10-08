.pragma library

.import "Contrast.js" as Contrast

// How ui/js/Syntax.js and ui/js/SyntaxMarkdown.js draw what they find: the Monokai colours, and the
// StyledText each role becomes. Every byte is escaped before a tag is built round it.

// Tabs are expanded to this stop, because StyledText collapses a tab the way it collapses spaces.
var TAB = 4

function expandTabs(text) {
    if (text.indexOf("\t") < 0) return text
    var out = "", col = 0
    for (var i = 0; i < text.length; i++) {
        var ch = text.charAt(i)
        if (ch === "\t") { var pad = TAB - col % TAB; out += "    ".substring(0, pad); col += pad }
        else { out += ch; col = ch === "\n" ? 0 : col + 1 }
    }
    return out
}

function escape(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }

// StyledText collapses runs of spaces and ignores a bare newline, so indentation becomes &nbsp;
// and a line break <br>. A single space between words stays a space, so wrapping still breaks there.
// The spaces are marked on the raw text, before any tag splits a run of them, with a private-use
// character no file is drawn with; layout turns the marks into the entity after escaping.
var HARD = "\uE000"
function hardSpaces(text) {
    return text.replace(/ {2,}/g, function (m) { return new Array(m.length + 1).join(HARD) })
               .replace(/(^|\n) /g, "$1" + HARD)
}

function layout(html) {
    return html.replace(/\uE000/g, "&nbsp;").replace(/\n/g, "<br>")
}

// PyCharm's Monokai, the operator's ruling of 2026-10-08, from its .icls and its sample picture.
// Fixed rather than the theme's, because the scheme is the point; keywords and parameters, self
// among them, draw italic as the picture shows them.
var MONOKAI = { keyword: "#66d9ef", builtin: "#66d9ef", string: "#e6db74", number: "#ae81ff",
                comment: "#75715e", operator: "#f92672", func: "#a6e22e", param: "#fd971f",
                heading: "#a6e22e", strong: "#fd971f" }
var ITALIC = { keyword: true, param: true }
var BOLD = { heading: true, strong: true }

// One stretch of one role, escaped before any tag is built round it.
function tag(role, s, palette) {
    s = escape(s)
    if (!role || !s.length) return s
    if (ITALIC[role]) s = "<i>" + s + "</i>"
    if (BOLD[role]) s = "<b>" + s + "</b>"
    return '<font color="' + palette[role] + '">' + s + "</font>"
}

// One line from [role, text] pieces, for ui/js/SyntaxMarkdown.js. The spaces are marked over the
// whole line, as highlight does, and the marking keeps every length, so the pieces cut it back up.
function line(pieces, palette) {
    var all = "", html = "", at = 0, k
    for (k = 0; k < pieces.length; k++) all += pieces[k][1]
    all = hardSpaces(all)
    for (k = 0; k < pieces.length; k++) {
        html += tag(pieces[k][0], all.substring(at, at + pieces[k][1].length), palette)
        at += pieces[k][1].length
    }
    return layout(html)
}

// The colours, each lifted to 4.5:1 on the ground it is drawn over, comments to 3:1. A dark theme
// keeps Monokai's own hexes; a light one darkens them, hue kept, until they read.
function colours(ground) {
    var out = {}
    for (var role in MONOKAI)
        out[role] = Contrast.ensureRatio(MONOKAI[role], ground, role === "comment" ? 3 : 4.5)
    return out
}

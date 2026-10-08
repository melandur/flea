.pragma library

.import "Contrast.js" as Contrast
.import "SyntaxLanguages.js" as Languages

// Syntax colour for the text previews, in PyCharm's Monokai: keywords, builtins, strings and their
// escapes, numbers and constants, comments, operators, the name a function is defined under, its
// parameters and self, and decorators. A lexer, not a parser, so each role is read off the tokens
// around it. Pure, so tests/js/syntax.js runs it under qml6 with no window. The output is
// StyledText, and every byte of the file is escaped before it reaches a tag, so a file can never
// inject one: StyledText would otherwise fetch an <img src> the way MarkdownText does.

// Larger files preview as plain text: the colour of a run depends on every run above it, so the
// first paint far down a big file would scan everything before it.
var MAX_BYTES = 2097152
// Tabs are expanded to this stop, because StyledText collapses a tab the way it collapses spaces.
var TAB = 4

var byExtension = null, specs = {}

// The family a file name previews in, or null for one that stays plain text.
function language(path) {
    var name = String(path || "").substring(String(path || "").lastIndexOf("/") + 1)
    if (Languages.NAMES[name]) return spec(Languages.NAMES[name])
    var dot = name.lastIndexOf(".")
    if (dot <= 0) return null
    if (!byExtension) {
        byExtension = {}
        for (var key in Languages.EXTENSIONS) {
            var exts = Languages.EXTENSIONS[key].split(" ")
            for (var i = 0; i < exts.length; i++) byExtension[exts[i]] = key
        }
    }
    var key2 = byExtension[name.substring(dot + 1).toLowerCase()]
    return key2 ? spec(key2) : null
}

function quote(s) {
    return s.replace(/[.*+?^${}()|[\]\\\/-]/g, "\\$&")
}

// One regex per family finds the next token worth colouring; everything between two of its
// matches is drawn as it is. Built once and kept, because a scroll asks for the same family often.
function spec(key) {
    if (specs[key]) return specs[key]
    var f = Languages.FAMILIES[key]
    var parts = [], kinds = []
    function add(kind, alternatives) {
        if (!alternatives.length) return
        alternatives.sort(function (a, b) { return b.length - a.length })
        parts.push("(" + alternatives.map(quote).join("|") + ")")
        kinds.push(kind)
    }
    // Block openers first, so lua's --[[ wins over its -- line comment.
    add("block", f.block.map(function (b) { return b[0] }))
    add("line", f.line)
    add("string", f.strings.map(function (s) { return s[0] }))
    if (f.tags) { parts.push("(<\\/?[A-Za-z][\\w:.-]*)"); kinds.push("tag") }
    if (f.decorators) { parts.push("(@[A-Za-z_][\\w.]*)"); kinds.push("decorator") }
    parts.push("(\\b\\d[\\w.]*)"); kinds.push("number")
    parts.push("([" + quote(f.idStart) + "A-Za-z_$][\\w$]*)"); kinds.push("word")
    // One sign a token, so a run of them never swallows the // or /* a comment opens with.
    if (f.ops) { parts.push("([" + quote(f.ops) + "])"); kinds.push("op") }
    var words = set(Languages.WORDS[f.words] + " " + Languages.CONSTANTS, f.ci)
    var constants = set(Languages.CONSTANTS, false)
    var closes = {}
    for (var b = 0; b < f.block.length; b++) closes[f.block[b][0]] = [f.block[b][1], false, true]
    for (var s = 0; s < f.strings.length; s++) closes[f.strings[s][0]] = [f.strings[s][1], true, f.strings[s][2]]
    specs[key] = { key: key, re: new RegExp(parts.join("|"), "g"), kinds: kinds, words: words,
                   constants: constants, closes: closes, ci: f.ci, hashWord: f.hashWord,
                   lifetimes: f.lifetimes, builtins: set(Languages.BUILTINS[f.words], false),
                   selfWords: set(f.self, false), definers: set(Languages.DEFINERS, false),
                   kwargs: f.kwargs, macros: f.macros, attrs: f.attrs }
    return specs[key]
}

function set(words, lower) {
    var out = {}, list = String(words || "").split(" ")
    for (var i = 0; i < list.length; i++) if (list[i]) out[lower ? list[i].toLowerCase() : list[i]] = 1
    return out
}

// The sign before a token, past any space or line break, which is how a parameter or a keyword
// argument is told from a name: it follows the ( or , of a list.
function signBefore(text, at) {
    var k = at - 1
    while (k >= 0 && /[ \uE000\n]/.test(text.charAt(k))) k--
    return k < 0 ? "" : text.charAt(k)
}

// Where the parameter list that opens just after a defined name closes, or -1 when no list follows
// it. A list still open at the run's end ends with the run.
function paramsEnd(text, from) {
    var k = from
    while (k < text.length && /[ \uE000]/.test(text.charAt(k))) k++
    if (text.charAt(k) !== "(") return -1
    for (var depth = 0; k < text.length; k++) {
        var ch = text.charAt(k)
        if (ch === "(") depth++
        else if (ch === ")" && --depth === 0) return k
    }
    return text.length
}

function escaped(text, at) {
    var n = 0
    while (at - n - 1 >= 0 && text.charAt(at - n - 1) === "\\") n++
    return n % 2 === 1
}

// Where an open comment or string ends: just past its closer, or at the line's end for a string
// that may not span lines, or at the run's end with the construct still open for the next run.
function closeAt(text, from, open) {
    var nl = open.multi ? -1 : text.indexOf("\n", from)
    var j = from
    for (;;) {
        var k = text.indexOf(open.close, j)
        if (nl >= 0 && (k < 0 || nl < k)) return { at: nl, open: false }
        if (k < 0) return { at: text.length, open: open.multi }
        if (open.esc && escaped(text, k)) { j = k + 1; continue }
        return { at: k + open.close.length, open: false }
    }
}

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
                comment: "#75715e", operator: "#f92672", func: "#a6e22e", param: "#fd971f" }
var ITALIC = { keyword: true, param: true }

// The colours, each lifted to 4.5:1 on the ground it is drawn over, comments to 3:1. A dark theme
// keeps Monokai's own hexes; a light one darkens them, hue kept, until they read.
function colours(ground) {
    var out = {}
    for (var role in MONOKAI)
        out[role] = Contrast.ensureRatio(MONOKAI[role], ground, role === "comment" ? 3 : 4.5)
    return out
}

// One run of the file: { html, state }, where state is the comment or string still open at its end
// and is what the next run starts in. With paint false only the state is worked out, which is how
// a run far down the file learns what the runs above it left open.
function highlight(text, sp, state, paint, palette) {
    text = expandTabs(text)
    if (paint) text = hardSpaces(text)
    var out = [], i = 0, n = text.length
    // Neighbouring text of one role is one tag, so a row of signs is not a tag a character.
    var role = "", held = ""
    function flush() {
        if (!held.length) return
        var s = escape(held)
        if (ITALIC[role]) s = "<i>" + s + "</i>"
        out.push(role ? '<font color="' + palette[role] + '">' + s + "</font>" : s)
        held = ""
    }
    function put(r, s) {
        if (!paint || !s.length) return
        if (r !== role) { flush(); role = r }
        held += s
    }
    // A string's escapes draw as constants do, Monokai's \n in purple.
    function putString(s, esc) {
        var at = 0, e, escapes = /\\./g
        while (esc && (e = escapes.exec(s))) {
            put("string", s.substring(at, e.index))
            put("number", e[0])
            at = e.index + 2
        }
        put("string", s.substring(at))
    }
    function openUntil(start, from, open) {
        var r = closeAt(text, from, open)
        if (open.comment) put("comment", text.substring(start, r.at))
        else putString(text.substring(start, r.at), open.esc)
        state = r.open ? open : null
        return r.at
    }
    if (state) i = openUntil(0, 0, state)
    var re = sp.re
    // Just past a def, fn or function keyword, and where the parameter list after its name closes.
    var defEnd = -1, params = -1
    while (i < n) {
        re.lastIndex = i
        var m = re.exec(text)
        if (!m) { put("", text.substring(i)); break }
        put("", text.substring(i, m.index))
        var tok = m[0], g = 1
        while (m[g] === undefined) g++
        var kind = sp.kinds[g - 1]
        i = m.index + tok.length
        if (kind === "line") {
            if (sp.hashWord && m.index > 0 && !/[\s\uE000;]/.test(text.charAt(m.index - 1))) { put("", tok); continue }
            var eol = text.indexOf("\n", m.index)
            i = eol < 0 ? n : eol
            put("comment", text.substring(m.index, i))
        } else if (kind === "block" || kind === "string") {
            if (kind === "string" && sp.lifetimes && tok === "'" && !/^'(\\.|[^\\'])'/.test(text.substring(m.index, m.index + 12))) {
                put("", tok)
                continue
            }
            var c = sp.closes[tok]
            i = openUntil(m.index, i, { close: c[0], esc: c[1], multi: c[2], comment: kind === "block" })
        } else if (kind === "tag") {
            put("", tok.substring(0, tok.charAt(1) === "/" ? 2 : 1))
            put("operator", tok.substring(tok.charAt(1) === "/" ? 2 : 1))
        } else if (kind === "decorator") {
            put("func", tok)
        } else if (kind === "number") {
            put("number", tok)
        } else if (kind === "op") {
            put("operator", tok)
        } else {
            var w = sp.ci ? tok.toLowerCase() : tok, sign = signBefore(text, m.index)
            var next = text.charAt(i), after = text.charAt(i + 1), r = ""
            if (sp.constants[tok]) r = "number"
            else if (sp.selfWords[tok]) r = "param"
            else if (sp.words[w]) r = "keyword"
            else if (defEnd >= 0 && /^[ \uE000]+$/.test(text.substring(defEnd, m.index))) r = "func"
            else if (m.index < params && /[(,*]/.test(sign)) r = "param"
            else if (sp.kwargs && /[(,]/.test(sign) && next === "=" && after !== "=") r = "param"
            else if (sp.attrs && next === "=") r = "func"
            else if (sp.builtins[tok] || sp.macros && next === "!" && after !== "=") r = "builtin"
            put(r, tok)
            if (r === "func") params = paramsEnd(text, i)
            defEnd = r === "keyword" && sp.definers[w] ? i : -1
        }
    }
    flush()
    return { html: paint ? layout(out.join("")) : "", state: state }
}

// A fresh cache for one body: the state each run starts in, worked out in order and kept, so
// scrolling back up costs nothing and scrolling down scans each run above the screen once.
function memo(runs, sp) { return { runs: runs, spec: sp, states: [null] } }

function run(cache, index, palette) {
    var states = cache.states
    for (var k = states.length - 1; k < index; k++)
        states.push(highlight(cache.runs[k], cache.spec, states[k], false, palette).state)
    return highlight(cache.runs[index], cache.spec, states[index], true, palette).html
}

// The column's first lines, one entry each, with what an earlier line left open carried down.
function lines(list, sp, palette) {
    var out = [], state = null
    for (var i = 0; i < list.length; i++) {
        var r = highlight(list[i], sp, state, true, palette)
        out.push(r.html)
        state = r.state
    }
    return out
}

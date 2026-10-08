.pragma library

.import "SyntaxStyle.js" as Style

// Markdown for ui/js/Syntax.js, which hands in its highlight and language as host, so the two never
// import each other.
// Markdown is read a line at a time rather than token by token: a heading, a rule, a fence, a list
// or quote marker is known only from where its line starts. A fenced block draws in the language its
// info string names, through the host's own highlighter, so ```python reads as a .py file would.

var FENCE = /^ {0,3}(`{3,}|~{3,}) *([\w+#.-]*)/
var HEADING = /^ {0,3}#{1,6}(?=[ \t]|$)/
var RULE = /^ {0,3}([-*_])( *\1){2,} *$/
var MARKER = /^ *(?:>|[-*+](?= )|\d{1,9}[.)](?= )) ?/
var INLINE = /`[^`\n]+`|\*\*[^*\n]+?\*\*|__[^_\n]+?__|\*[^*\s][^*\n]*?\*|_[^_\s][^_\n]*?_|!?\[[^\]\n]*\]\([^)\n]*\)|<https?:\/\/[^>\s]*>/g

// The info strings that name a language by something other than its extension.
var ALIASES = { python: "py", python3: "py", javascript: "js", typescript: "ts", rust: "rs",
                bash: "sh", shell: "sh", console: "sh", zsh: "sh", ruby: "rb", perl: "pl",
                golang: "go", "c++": "cpp", csharp: "cs", kotlin: "kt", yml: "yaml",
                dockerfile: "Dockerfile", docker: "Dockerfile", containerfile: "Dockerfile" }

function fenceSpec(info, host) {
    var name = String(info).toLowerCase()
    if (!name) return null
    var alias = ALIASES[name] || name
    var sp = host.language(alias === "Dockerfile" ? alias : "x." + alias)
    return sp && !sp.markdown ? sp : null
}

function wordAt(text, at) { return /\w/.test(text.charAt(at)) }

// The pieces of a line's prose: code spans, strong and emphasis, links and autolinks.
function inline(text, pieces) {
    var at = 0, m
    INLINE.lastIndex = 0
    while ((m = INLINE.exec(text))) {
        var tok = m[0], c = tok.charAt(0), end = m.index + tok.length
        // snake_case is a name, not emphasis: _ opens and closes only at a word's edge.
        if (c === "_" && (wordAt(text, m.index - 1) || wordAt(text, end))) { INLINE.lastIndex = m.index + 1; continue }
        pieces.push(["", text.substring(at, m.index)])
        if (c === "`") pieces.push(["string", tok])
        else if (tok.substring(0, 2) === "**" || tok.substring(0, 2) === "__") pieces.push(["strong", tok])
        else if (c === "*" || c === "_") pieces.push(["param", tok])
        else if (c === "<") pieces.push(["number", tok])
        else {
            var close = tok.indexOf("](")
            var open = c === "!" ? 2 : 1
            pieces.push(["", tok.substring(0, open)], ["builtin", tok.substring(open, close)],
                        ["", "]("], ["number", tok.substring(close + 2, tok.length - 1)], ["", ")"])
        }
        at = end
    }
    pieces.push(["", text.substring(at)])
}

// One line outside a fence: { pieces, state }, where state is the fence the line opens, if any.
function prose(line, host) {
    var fence = FENCE.exec(line)
    if (fence) return { pieces: [["comment", line]],
                        state: { mark: fence[1], sp: fenceSpec(fence[2], host), inner: null } }
    if (HEADING.test(line)) return { pieces: [["heading", line]], state: null }
    if (RULE.test(line)) return { pieces: [["comment", line]], state: null }
    var pieces = [], rest = line, m
    while (rest.length && (m = MARKER.exec(rest)) && m[0].length) {
        var lead = m[0].length - m[0].replace(/^ +/, "").length
        pieces.push(["", rest.substring(0, lead)], ["operator", m[0].substring(lead)])
        rest = rest.substring(m[0].length)
    }
    inline(rest, pieces)
    return { pieces: pieces, state: null }
}

// A fence closes on a line of its own character at least as long as the one that opened it.
function closes(line, mark) {
    var m = /^ {0,3}(`{3,}|~{3,}) *$/.exec(line)
    return m !== null && m[1].charAt(0) === mark.charAt(0) && m[1].length >= mark.length
}

// One run: { html, state }, the same contract as the host's own highlight. The state is a fence
// still open at the run's end, with the inner language's own state inside it; each line makes a new
// one rather than change the last, because the host's memo keeps every run's starting state.
function highlight(text, state, paint, palette, host) {
    var lines = Style.expandTabs(text).split("\n"), out = []
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i], html = ""
        if (state && closes(line, state.mark)) {
            html = paint ? Style.line([["comment", line]], palette) : ""
            state = null
        } else if (state && state.sp) {
            var r = host.highlight(line, state.sp, state.inner, paint, palette)
            html = r.html
            state = { mark: state.mark, sp: state.sp, inner: r.state }
        } else if (state) {
            html = paint ? Style.line([["string", line]], palette) : ""
        } else {
            var p = prose(line, host)
            html = paint ? Style.line(p.pieces, palette) : ""
            state = p.state
        }
        out.push(html)
    }
    return { html: paint ? out.join("<br>") : "", state: state }
}

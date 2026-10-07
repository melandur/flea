.import "../../ui/js/Syntax.js" as Syntax
.import "../../ui/js/TextChunks.js" as TextChunks

// Fixed colours with no lift, so a span names its role by its colour alone.
var P = { keyword: "K", string: "S", number: "N", comment: "C" }

function paint(path, text, state) {
    return Syntax.highlight(text, Syntax.language(path), state || null, true, P)
}
function html(path, text) { return paint(path, text).html }
function span(colour, text) { return '<font color="' + colour + '">' + text + "</font>" }

function run(check) {
    check("a file with no family stays plain", Syntax.language("/d/notes.txt"), null)
    check("an extension is matched in any case", Syntax.language("/d/MAIN.RS").key, "rust")
    check("a bare well-known name has a family", Syntax.language("/d/PKGBUILD").key, "sh")
    check("a dotfile with no extension of its own is plain", Syntax.language("/d/.gitignore"), null)

    check("keywords, numbers and plain names each draw as they are",
          html("a.py", "def f(x): return 42"),
          span("K", "def") + " f(x): " + span("K", "return") + " " + span("N", "42"))
    check("a constant draws as a number does", html("a.py", "x = None"), "x = " + span("N", "None"))
    check("a line comment runs to the line's end and no further",
          html("a.rs", "let a = 1; // one\nlet"),
          span("K", "let") + " a = " + span("N", "1") + "; " + span("C", "// one") + "<br>" + span("K", "let"))
    check("an escaped quote does not close a string",
          html("a.js", 'x = "a\\"b" + y'), 'x = ' + span("S", '"a\\"b"') + " + y")
    check("a keyword inside a string is only string", html("a.js", '"if"'), span("S", '"if"'))
    check("a name that merely holds a number is not one", html("a.c", "x1"), "x1")
    check("a C directive is a keyword", html("a.c", "#include"), span("K", "#include"))
    check("SQL's keywords match in any case", html("q.sql", "SELECT a"), span("K", "SELECT") + " a")

    // The file is arbitrary text: every byte is escaped before a tag can be built from it.
    check("markup in a file is escaped, never drawn",
          html("a.py", "x = '<img src=\"http://e/x\">' & y"),
          "x = " + span("S", "'&lt;img src=\"http://e/x\"&gt;'") + " &amp; y")
    check("an HTML tag name is coloured and its bracket escaped",
          html("a.html", "<p>hi</p>"), "&lt;" + span("K", "p") + "&gt;hi&lt;/" + span("K", "p") + "&gt;")

    // StyledText collapses spaces and drops a bare newline, so indentation and breaks are spelled out.
    check("indentation survives as hard spaces", html("a.py", "  x"), "&nbsp;&nbsp;x")
    check("a single leading space survives too, even inside a comment run",
          paint("a.c", " * b */", { close: "*/", esc: false, multi: true, comment: true }).html,
          span("C", "&nbsp;* b */"))
    check("a single space between words stays breakable", html("a.txt.py", "a b"), "a b")
    check("a tab expands to the next stop", html("a.py", "\tx\na\ty"), "&nbsp;&nbsp;&nbsp;&nbsp;x<br>a&nbsp;&nbsp;&nbsp;y")

    // What a run leaves open is what the next one starts in, which is how a block comment or a
    // triple-quoted string crosses the preview's 4 KB runs.
    var open = paint("a.c", "x /* one")
    check("a block comment left open is the run's end state", open.state !== null, true)
    check("and the next run starts inside it and closes it",
          paint("a.c", "two */ y", open.state).html, span("C", "two */") + " y")
    var doc = paint("a.py", 'x = """doc')
    check("a triple-quoted string spans lines", paint("a.py", 'more""" + y', doc.state).html,
          span("S", 'more"""') + " + y")
    check("an ordinary string never carries past its line", paint("a.py", "x = 'abc").state, null)

    check("a Rust lifetime is not a string", html("a.rs", "&'a str"), "&amp;'a str")
    check("but a char literal is", html("a.rs", "'x'"), span("S", "'x'"))
    check("shell's # inside a word is not a comment", html("a.sh", "echo $# x"), "echo $# x")
    check("but at a word's start it is", html("a.sh", "a # c"), "a " + span("C", "# c"))
    check("lua's long comment beats its line comment", html("a.lua", "--[[ x ]] y"), span("C", "--[[ x ]]") + " y")

    // Scrolling straight to a run far down must colour it exactly as reading down to it would.
    var lines = []
    for (var i = 0; i < 3000; i++) lines.push(i % 400 === 0 ? "/* open " + i : i % 400 === 7 ? "close */" : "int x" + i + " = " + i + ";")
    var runs = TextChunks.split(lines.join("\n"))
    var sp = Syntax.language("a.c")
    var walked = Syntax.memo(runs, sp), jumped = Syntax.memo(runs, sp)
    for (var r = 0; r < runs.length; r++) Syntax.run(walked, r, P)
    var last = runs.length - 1
    check("a jump to the last run colours it as reading down did",
          Syntax.run(jumped, last, P), Syntax.run(walked, last, P))
    check("there is more than one run to cross", runs.length > 3, true)

    // The column's first lines carry an open comment down from line to line.
    var first = Syntax.lines(["/* a", "b */ c"], Syntax.language("a.c"), P)
    check("a comment open on one column line colours the next", first[1], span("C", "b */") + " c")

    check("colours are lifted to the ground they draw on",
          Syntax.colours("#202020", "#00ff00", "#00ffff", "#808080", "#101010").keyword !== "#202020", true)
}

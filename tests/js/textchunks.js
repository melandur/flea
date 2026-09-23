.import "../../ui/js/TextChunks.js" as TextChunks

function line(n, c) { return new Array(n + 1).join(c) }

function run(check) {
    check("an empty body is no runs", TextChunks.split("").length, 0)
    check("a short body is one run, verbatim", JSON.stringify(TextChunks.split("a\n\nb\n")), JSON.stringify(["a\n\nb\n"]))

    // Joined back with the newline each boundary dropped, the runs are the body again.
    var lines = []
    for (var i = 0; i < 2000; i++) lines.push(i % 7 === 0 ? "" : "line " + i + " " + line(i % 50, "x"))
    var body = lines.join("\n")
    var runs = TextChunks.split(body)
    check("a long body is cut into several runs", runs.length > 1, true)
    check("the runs rejoin to the body", runs.join("\n"), body)
    check("every run holds at least the target", runs.slice(0, -1).every(function (r) { return r.length >= TextChunks.TARGET }), true)
    check("every cut falls on a line boundary", runs.slice(0, -1).every(function (r, k) { return body.indexOf(r + "\n") >= 0 }), true)

    // A minified file, one line and no newline: cut at the limit rather than one run of it all.
    var flat = line(3 * TextChunks.LIMIT + 5, "m")
    var flatRuns = TextChunks.split(flat)
    check("a line with no newline is cut at the limit", flatRuns.map(function (r) { return r.length }).join(","),
        [TextChunks.LIMIT, TextChunks.LIMIT, TextChunks.LIMIT, 5].join(","))
    check("the hard cuts drop nothing", flatRuns.join(""), flat)

    // A long line after short ones: the cut falls on the last break before the target instead.
    var mixed = "short\n" + line(TextChunks.TARGET, "s") + "\n" + line(2 * TextChunks.LIMIT, "L") + "\nend"
    var mixedRuns = TextChunks.split(mixed)
    check("a long line is cut at the break before it", mixedRuns[0], "short\n" + line(TextChunks.TARGET, "s"))
    check("the long line is cut at the limit and its newline closes the run", mixedRuns.slice(1).map(function (r) { return r.length }).join(","),
        [TextChunks.LIMIT, TextChunks.LIMIT, 3].join(","))
}

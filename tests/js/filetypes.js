.import "../../ui/js/FileTypes.js" as FileTypes
.import "../../ui/js/FileTypeColors.js" as Colors
.import "../../ui/js/Icons.js" as Icons

// The filetype tier: yazi's colour table (vendored, generated) under Flea's own lucide marks.
// Pure, so the whole of both halves is assertable with no window and no theme.

var LNK = 0o120777
var REG = 0o100644

function run(check) {
    runExtensions(check)
    runMarks(check)
    runColors(check)
    runRows(check)
    runGeometry(check)
}

// Yazi writes compound extensions as single keys, so the longest suffix has to be offered first
// or "spec.ts" can never beat "ts", and "d.ts" can never beat "ts" either.
function runExtensions(check) {
    check("a plain name offers its one extension", FileTypes.extensions("main.rs").join(","), "rs")
    check("a compound name offers the long one first",
          FileTypes.extensions("app.spec.ts").join(","), "spec.ts,ts")
    check("three dots, still longest first",
          FileTypes.extensions("a.b.c.d").join(","), "b.c.d,c.d,d")
    check("a dotfile with no second dot has no extension at all",
          FileTypes.extensions(".bashrc").join(","), "")
    check("but a dotfile with one does", FileTypes.extensions(".prettierrc.json").join(","), "json")
    check("case never reaches the table", FileTypes.extensions("PHOTO.JPG").join(","), "jpg")
    check("a name with no dot offers nothing", FileTypes.extensions("Makefile").join(","), "")
    // A trailing dot is a name ending in ".", not an empty extension the table could match.
    check("a trailing dot offers no empty key", FileTypes.extensions("odd.").join(","), "")
}

function runMarks(check) {
    check("rust source is code", FileTypes.markFor("main.rs", false), "code")
    check("a json file is braces", FileTypes.markFor("tsconfig.json", false), "braces")
    check("so is a toml", FileTypes.markFor("Cargo.toml", false), "braces")
    check("markup is code-xml, not code", FileTypes.markFor("index.html", false), "code-xml")
    check("an object file is binary", FileTypes.markFor("libflea.so", false), "binary")
    check("a database is a database", FileTypes.markFor("places.sqlite", false), "database")
    check("an image is a disc", FileTypes.markFor("arch.iso", false), "disc")
    check("subtitles are captions", FileTypes.markFor("ep01.srt", false), "captions")
    check("a checksum is a hash", FileTypes.markFor("archive.sha256", false), "hash")
    check("an ebook opens", FileTypes.markFor("novel.epub", false), "book-open")
    check("a notebook is a notebook", FileTypes.markFor("analysis.ipynb", false), "notebook")
    // The exact-name table beats the extension table, which is yazi's own order too.
    check("a lock file is a lock, not the json its name ends in",
          FileTypes.markFor("package-lock.json", false), "lock")
    check("a Dockerfile is a box", FileTypes.markFor("Dockerfile", false), "box")
    check("and the name matches whatever its case", FileTypes.markFor("DOCKERFILE", false), "box")
    check("a git config is a branch", FileTypes.markFor(".gitignore", false), "git-branch")
    // scroll is a family name and not a mark: prose is prose, and the set's prose mark is file-text.
    check("a licence is prose", FileTypes.markFor("LICENSE", false), "file-text")
    check("a name no family claims answers nothing, so the icon name keeps the row",
          FileTypes.markFor("notes.qqq", false), "")
    check("an empty name answers nothing", FileTypes.markFor("", false), "")
    // A directory is entered, and the folder mark is the whole of what says so. No extension on a
    // directory name may take that away, which is why isDir short-circuits before any lookup.
    check("a directory keeps the folder mark whatever it is called",
          FileTypes.markFor("build.rs", true), "")
    check("even one named like a docker file", FileTypes.markFor("Dockerfile", true), "")
}

function runColors(check) {
    check("rust takes devicons' rust orange", FileTypes.colorFor("main.rs", false), "#dea584")
    check("typescript its blue", FileTypes.colorFor("app.ts", false), "#519aba")
    check("a git file takes git's red", FileTypes.colorFor(".gitignore", false), "#f54d27")
    check("case never reaches the table", FileTypes.colorFor("MAIN.RS", false), "#dea584")
    check("the exact name wins over the extension",
          FileTypes.colorFor("package.json", false) === FileTypes.colorFor("random.json", false), false)
    check("a name with no rule answers nothing", FileTypes.colorFor("notes.qqq", false), "")
    check("an empty name answers nothing", FileTypes.colorFor("", false), "")
    // yazi's dirs table is 14 well-known names, and a directory reads that table and no other:
    // a folder called "Music" is not a .music file and must not be coloured as one.
    check("a well-known directory takes its own colour",
          FileTypes.colorFor("Downloads", true), "#00bcd4")
    check("an ordinary directory takes none", FileTypes.colorFor("src", true), "")
    check("a directory named like a file still reads the dirs table alone",
          FileTypes.colorFor("main.rs", true), "")

    // The generated table is the shipped artefact, so its own shape is worth one check: a rule
    // whose packing broke would answer "" for everything and nothing above would catch it.
    check("the colour table carries every kind of key",
          [Colors.extColor("rs"), Colors.nameColor("package.json"), Colors.dirColor(".config")].join(","),
          "#dea584,#e8274b,#ff9800")
    check("and a key it has never seen answers empty",
          Colors.extColor("qqq") + Colors.nameColor("qqq") + Colors.dirColor("qqq"), "")
}

// What a board actually asks for. A symlink is a link before it is anything else, on both halves.
function runRows(check) {
    var row = { n: "main.rs", i: "text-x-generic", p: REG, d: false }
    check("a row handed in reaches the filetype tier",
          Icons.glyphForRow(row.i, row.p, row), "code")
    check("and the same row with no third argument does not",
          Icons.glyphForRow(row.i, row.p), "file-text")
    check("which is the switch being off", Icons.glyphForRow(row.i, row.p, null), "file-text")
    check("the colour half comes with it", FileTypes.rowColor(row.n, row.p, row.d), "#dea584")

    var link = { n: "main.rs", i: "text-x-generic", p: LNK, d: false }
    check("a symlink keeps the link mark even inside the tier",
          Icons.glyphForRow(link.i, link.p, link), "symlink")
    check("and takes the theme's own symlink colour, not the filetype's",
          FileTypes.rowColor(link.n, link.p, link.d), "")
}

// Every mark a family can name has to resolve to real path data. A name missing from PATHS falls
// back to the file mark in silence, which on a listing reads as a row of blank documents.
function runGeometry(check) {
    var named = {}
    var tables = [FileTypes.FAMILIES, FileTypes.NAMED]
    for (var t = 0; t < tables.length; t++) {
        for (var family in tables[t])
            named[FileTypes.FAMILY_MARK[family] || family] = true
    }
    var missing = []
    var silent = []
    for (var mark in named) {
        if (Icons.PATHS[mark] === undefined)
            missing.push(mark)
        else if (mark !== "file" && Icons.pathFor(mark) === Icons.pathFor("file"))
            silent.push(mark)
    }
    check("every family names a mark PATHS carries", missing.join(","), "")
    check("and each is its own mark, not the silent file fallback", silent.join(","), "")
    // The fifteen this tier added. Each is checked by name so a deletion cannot pass by leaving
    // the loop above with a shorter list to walk.
    var added = ["braces", "code-xml", "binary", "database", "disc", "key", "book-open", "palette",
                 "box", "git-branch", "hash", "captions", "mail", "calendar", "notebook"]
    var absent = []
    for (var i = 0; i < added.length; i++) {
        if (!Icons.PATHS[added[i]])
            absent.push(added[i])
    }
    check("the fifteen recut marks are all in the table", absent.join(","), "")
    // The cut's own rule, spot-checked where it is easiest to get wrong: a genuine curve stays a
    // curve. disc is two circles and database's skirts are arcs, so both must still carry an A.
    check("disc keeps its circles", Icons.pathFor("disc").indexOf("A10 10") >= 0, true)
    check("database keeps its ellipse", Icons.pathFor("database").indexOf("A9 3") >= 0, true)
    check("and braces, which is nothing but corners, keeps none",
          Icons.pathFor("braces").toLowerCase().indexOf("a") >= 0, false)
}

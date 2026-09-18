.pragma library

.import "FileTypeColors.js" as Colors
.import "Format.js" as Format

// What a filename says about its row, beyond what the backend's freedesktop icon name can say.
//
// Two halves, from two places, on purpose. The COLOUR is yazi's, which is nvim-web-devicons'
// table, vendored at vendor/yazi-icons.toml and generated into FileTypeColors.js; 713 rules is
// an inventory no one in this tree would have written by hand and none of it is geometry. The
// MARK is Flea's own, the lucide set cut to the Omarchy edge in Icons.js, because yazi's marks
// are nerd-font codepoints and AGENTS.md "The row mark: Shape, not a generated font" rules a
// font glyph out of this tree. So a `.rs` row draws lucide's chevrons in devicons' rust orange,
// and never a brand logo: there are 282 distinct glyphs in that table, most of them logos, and
// reproducing a trademark is a different decision from importing a colour, see AGENTS.md
// "The Network row marks".
//
// FAMILIES is therefore deliberately coarser than the colour table. A family exists only where
// the shape itself carries information the generic file mark loses; everything else keeps the
// mark the backend's icon name already chose and takes only the colour.
var FAMILIES = {
    "braces": "json json5 jsonc yaml yml toml ini cfg conf config properties env plist webmanifest"
        + " tsconfig nswag csproj sln slnx dconf rasi qss material xcstrings strings prisma"
        // A stylesheet is a block language: its own braces are the mark, not a page of prose.
        + " css scss sass less styl",
    "code-xml": "html htm xml svg svgz vue svelte astro jsx tsx php blade.php erb ejs haml slim"
        + " liquid twig mustache hbs xaml xul xslt cshtml razor templ qml ui gql graphql",
    "code": "rs c h cc cp cpp cxx c++ cu cuh hh hpp hxx go py pyi pyw pyx pxd pxi rb js cjs mjs cts"
        + " mts ts d.ts lua luau pl pm swift kt kts scala clj cljc cljs cljd ex exs erl hrl hs lhs"
        + " ml mli fs fsi fsx cs dart zig nim v d elm gleam sol vala odin mojo r jl m mm java groovy"
        + " coffee el elc eln scm sc sbt res resi rlib gd godot tscn tres gleam adb ads ada asm"
        + " apl bqn fnl mint moon nix norg purs ps rkt tcl tbc templ vim vsh vhdl vhd vh sv svh"
        + " frag vert geom glsl huff ino ixx cppm ccm cxxm mo pp epp drl bicep bicepparam",
    "terminal": "sh bash zsh fish ksh csh bat cmd ps1 psm1 psd1 awk nu mk cmake bazel bzl ebuild"
        + " gnumakefile makefile rake gradle applescript",
    "binary": "o a so dll lib elf exe bin out wasm ko pyc pyo pyd luac jar class rlib apk vsix xpi"
        + " app pck hex",
    "database": "db sql sqlite sqlite3 dump",
    "disc": "iso img dmg toast cue",
    "key": "pub asc sig signature kdb kdbx kbx",
    "hash": "md5 sha1 sha224 sha256 sha384 sha512",
    "book-open": "epub mobi ebook",
    "palette": "psd psb xcf ai kra kpp blp skp",
    // Packages, containers and solids: a thing with an inside, which is what the cube says.
    "box": "3mf stl obj ply fbx glb brep step stp ste igs iges ige wrl wrz f3d sldasm sldprt scad"
        + " gemspec import",
    "git-branch": "git",
    "captions": "srt ssa ass sub vtt lrc",
    "mail": "msf eml",
    "calendar": "ics ical icalendar ifb",
    "notebook": "ipynb rmd org",
    "lock": "lock lck",
    "archive": "zip tar gz bz bz2 bz3 xz txz tgz zst 7z rar cab",
    "image": "png jpg jpeg gif webp bmp ico avif jxl tif tiff heic heif image",
    "film": "mp4 mkv webm mov avi m4v ogv ogx wmv mpp 3gp kdenlive kdenlivetitle",
    "music": "mp3 wav flac aac m4a ogg oga opus wma ape wv wvc aif aiff xm m3u m3u8 pls",
    "type": "ttf otf woff woff2 eot flf",
    "table": "csv xls xlsx ods fods",
    "presentation": "ppt pptx odp fodp",
    "file-text": "txt md markdown mdx rmd tex typ log nfo doc docx odt fodt odf pdf po pot diff"
        + " patch rss lrc info man",
    "globe": "torrent magnet http hurl rss webpack",
    "app-window": "desktop terminal"
}

// The exact filenames worth a mark of their own. Yazi carries 214 and most of them are one
// project's config file, which the extension already answers for; these are the ones whose whole
// name is the type, and the ones with no extension at all.
var NAMED = {
    "git-branch": ".gitignore .gitattributes .gitconfig .gitmodules .git-blame-ignore-revs .mailmap"
        + " commit_editmsg .gitlab-ci.yml",
    "box": "dockerfile containerfile compose.yaml compose.yml docker-compose.yaml docker-compose.yml"
        + " .dockerignore package.json go.mod go.sum go.work pom.xml build.zig.zon vercel.json"
        + " wrangler.toml wrangler.jsonc node_modules workspace build",
    "lock": "package-lock.json bun.lock bun.lockb pnpm-lock.yaml mix.lock cargo.lock yarn.lock"
        + " flake.lock poetry.lock gemfile.lock",
    "terminal": "makefile gnumakefile cmakelists.txt justfile .justfile procfile vagrantfile"
        + " rakefile brewfile jenkinsfile pkgbuild gradlew gulpfile.js gruntfile.js .bashrc"
        + " .bash_profile .zshrc .zshenv .zprofile .xinitrc .xsession bspwmrc sxhkdrc",
    "scroll": "license license.md copying copying.lesser unlicense authors authors.txt security"
        + " security.md code_of_conduct code_of_conduct.md readme readme.md",
    "key": ".xauthority py.typed",
    "braces": ".prettierrc .babelrc .eslintrc .npmrc .nvmrc .editorconfig .settings.json"
        + " tsconfig.json ionic.config.json robots.txt config"
}

// scroll has no mark of its own: a licence and a readme are prose, and the set's prose mark is
// file-text. Named here as its own family only so the table reads as what it is.
var FAMILY_MARK = { "scroll": "file-text" }

var MAPS = null

function maps() {
    if (MAPS)
        return MAPS
    MAPS = { e: {}, n: {} }
    fill(MAPS.e, FAMILIES)
    fill(MAPS.n, NAMED)
    return MAPS
}

function fill(into, table) {
    for (var family in table) {
        var mark = FAMILY_MARK[family] || family
        var keys = table[family].split(" ")
        for (var i = 0; i < keys.length; i++) {
            // First writer wins, so a key listed twice keeps the family that claimed it first.
            if (keys[i].length > 0 && into[keys[i]] === undefined)
                into[keys[i]] = mark
        }
    }
}

// Every suffix of a dotted name, longest first, with no leading dot: "app.spec.ts" offers
// "spec.ts" then "ts". Yazi writes compound extensions as single keys, so the longest has to be
// asked for first or "spec.ts" can never win over "ts". A dotfile with no further dot is a name
// and not an extension, which is why the run starts after the first character.
function extensions(name) {
    var out = []
    var lower = String(name).toLowerCase()
    for (var i = 1; i < lower.length - 1; i++) {
        if (lower.charAt(i) === ".")
            out.push(lower.substring(i + 1))
    }
    return out
}

function lowerName(name) {
    return String(name).toLowerCase()
}

// The mark a name earns, or "" for a name no family claims, which leaves the row on the mark the
// backend's own icon name already chose. A directory is never renamed by this: the folder mark is
// what says a row can be entered, and no extension may take that away.
function markFor(name, isDir) {
    if (isDir)
        return ""
    var table = maps()
    var exact = table.n[lowerName(name)]
    if (exact)
        return exact
    var exts = extensions(name)
    for (var i = 0; i < exts.length; i++) {
        if (table.e[exts[i]])
            return table.e[exts[i]]
    }
    return ""
}

// The colour a name earns, or "" for a name yazi's table has no rule for. A directory reads the
// dirs table alone, which is where yazi puts Downloads, .config and the other well-known names.
function colorFor(name, isDir) {
    var lower = lowerName(name)
    if (isDir)
        return Colors.dirColor(lower)
    var exact = Colors.nameColor(lower)
    if (exact)
        return exact
    var exts = extensions(name)
    for (var i = 0; i < exts.length; i++) {
        var hit = Colors.extColor(exts[i])
        if (hit)
            return hit
    }
    return ""
}

// The colour half as a board asks for it, off the row's own fields. A symlink keeps the theme's
// own symlink colour and takes none of this: a link is a link before it is a .rs file, which is
// the same rule ui/js/Icons.js glyphForRow applies to the mark. ui/Theme.qml markInk is the one
// caller, and it does the WCAG lift this deliberately does not.
function rowColor(name, mode, isDir) {
    if (Format.isSymlink(mode))
        return ""
    return colorFor(name, isDir)
}

.pragma library

// How a row is classified: which of the twelve preview states it is, and the extension families that
// decide the three the backend's icon cannot. Split out of Facts.js, which owns the fact tables.

var IMAGE = "image"
var VIDEO = "video"
var AUDIO = "audio"
var PDF = "pdf"
var TEXT = "text"
var CODE = "code"
var ARCHIVE = "archive"
var SYMLINK = "symlink"
var MULTI = "multi"
var LOADING = "loading"
var ERROR = "error"
var UNSUPPORTED = "unsupported"

// src/backend/rows.rs's own UNKNOWN_KIND: the Kind a row gets when nothing could describe its type.
var UNKNOWN_KIND = "Data"

var CODE_EXTENSIONS = [
    ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".c", ".h", ".cpp", ".hpp", ".cc", ".go", ".rb",
    ".java", ".kt", ".swift", ".lua", ".pl", ".php", ".sh", ".bash", ".zsh", ".fish", ".vim",
    ".qml", ".css", ".scss", ".html", ".xml", ".json", ".toml", ".yaml", ".yml", ".ini", ".conf",
    ".sql", ".mjs", ".cjs"
]

function isCode(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < CODE_EXTENSIONS.length; i++) {
        var e = CODE_EXTENSIONS[i]
        if (lower.length > e.length && lower.indexOf(e, lower.length - e.length) !== -1) {
            return true
        }
    }
    return false
}

var MARKDOWN_EXTENSIONS = [".md", ".markdown", ".mkd"]

function isMarkdown(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < MARKDOWN_EXTENSIONS.length; i++) {
        var e = MARKDOWN_EXTENSIONS[i]
        if (lower.length > e.length && lower.indexOf(e, lower.length - e.length) !== -1) {
            return true
        }
    }
    return false
}

function isPdf(name) {
    var lower = String(name).toLowerCase()
    return lower.length > 4 && lower.indexOf(".pdf", lower.length - 4) !== -1
}

// The icon name is the classification the backend already made, so the column never re-derives one.
// Its first segment is the freedesktop family, and the family is what classifies: this box's
// generic-icons table maps text/* to five different text- names, and a list of the names seen so
// far is exactly how the Quick Look refused every image for as long as it kept one.
function kindState(icon) {
    var name = String(icon)
    switch (name) {
    case "text-x-script":
    case "text-html":
    case "application-xml": return CODE
    case "package-x-generic": return ARCHIVE
    }
    if (name.indexOf("image-") === 0) return IMAGE
    if (name.indexOf("video-") === 0) return VIDEO
    if (name.indexOf("audio-") === 0) return AUDIO
    if (name.indexOf("text-") === 0) return TEXT
    // A .doc or .odt shares the office icon with a PDF, and a PDF was already told apart by its
    // extension. What is left is binary, so it is Unsupported rather than a newline count over a blob.
    return UNSUPPORTED
}

// A directory is not a preview subject: the column shows what is inside it instead.
function isPreviewable(row) {
    return row !== null && row !== undefined && row.d !== true
}

// The Quick Look's own answer, from the icon and the name alone: the three tests Facts.state makes
// before the ones only a row can answer (symlink mode, Kind name, selection count). Code renders as
// text there, because the overlay draws no gutter, and a markdown file renders verbatim like any text.
function quickLookKind(icon, name) {
    if (isPdf(name)) return PDF
    if (isMarkdown(name)) return TEXT
    var byIcon = kindState(icon)
    return byIcon === CODE ? TEXT : byIcon
}

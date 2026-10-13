.pragma library
.import "Format.js" as Format

// Which rows get an Extract, and what a new archive is called. The backend never sends an is-archive
// flag: the client is given the icon name and never the MIME type, so this is the whole mechanism.

// Longest form first, so ".tar.gz" is matched before ".gz" could be. Flea reads 7z and rar and writes
// neither, so both are here, where Extract is decided, and never in the compress submenu, which is
// the table src/backend/archive.rs probed.
var EXTENSIONS = [".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar", ".zip", ".7z", ".rar"]

function isArchive(name) {
    var lower = String(name).toLowerCase()
    for (var i = 0; i < EXTENSIONS.length; i++) {
        if (lower.length > EXTENSIONS[i].length && lower.indexOf(EXTENSIONS[i], lower.length - EXTENSIONS[i].length) !== -1) {
            return true
        }
    }
    return false
}

// The name an extract unpacks into: the archive's own, with every archive extension taken off.
function extractDir(name) {
    var text = String(name)
    var lower = text.toLowerCase()
    for (var i = 0; i < EXTENSIONS.length; i++) {
        if (lower.length > EXTENSIONS[i].length && lower.indexOf(EXTENSIONS[i], lower.length - EXTENSIONS[i].length) !== -1) {
            return text.substring(0, text.length - EXTENSIONS[i].length)
        }
    }
    return text
}

// One row compresses under its own name; several compress under the directory holding them.
function archiveStem(names, parentLeaf) {
    if (names.length === 1) {
        return stripExtension(names[0])
    }
    return parentLeaf.length > 0 ? parentLeaf : "archive"
}

function stripExtension(name) {
    var text = String(name)
    var cut = text.lastIndexOf(".")
    return cut > 0 ? text.substring(0, cut) : text
}

// The status line while an archive job runs: what it is doing, then how far through its input. The
// backend counts bytes the tool has read, and reading the last byte is not finishing, so this never
// says 100%; the done line is what says it finished.
// Sample: "Compressing 3 items to .zip, 42% (1.1 GB of 2.6 GB)"
function progressLine(work, done, total) {
    if (!(total > 0)) {
        return work
    }
    var percent = Math.min(99, Math.floor(done * 100 / total))
    return work + ", " + percent + "% (" + Format.size(done) + " of " + Format.size(total) + ")"
}

// The compress submenu is exactly the table the backend probed, never a fixed list, so a box with
// no bsdtar offers nothing at all.
function formatEntries(formats) {
    var out = []
    for (var i = 0; i < formats.length; i++) {
        out.push({ id: formats[i], label: "." + formats[i] })
    }
    return out
}

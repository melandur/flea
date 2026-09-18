.pragma library

.import "Format.js" as Format

// Errors reach the user as one sentence, never a raw path or errno. StatusBar board rule 4: the
// sentence names the input and drops advice nobody can act on. named is the failing directory's own
// leaf, empty when the breadcrumb is already standing on it and there is nothing to add.
function sentence(where, message, named) {
    if (where === "scan") {
        if (denied(where, message)) {
            return named ? "Permission denied on " + named : "Permission denied"
        }
        return named ? "That directory could not be read: " + named : "That directory could not be read."
    }
    if (where === "sort") {
        // Size and mtime are real orders, so the one refusal left is a key the wire never defined.
        return "Sorting by that column is not available."
    }
    if (where === "read") {
        return "The backend stopped responding; reopen Flea and try again."
    }
    // The write operations say what they were doing, because the operator is about to try it again.
    if (where === "undo" || where === "redo") {
        // The empty journal is the common case and the backend's own sentence is already the right one.
        return capitalised(message)
    }
    if (where === "rename") {
        return exists(message) ? "A file with that name is already here." : "That file could not be renamed."
    }
    // undo reverses a rename through the same call, so this sentence names no direction.
    if (where === "rename-kept") {
        return "Copied, but the old name was only partly removed. Check it."
    }
    // Deliberately not the capitalised branch: every other mkdir refusal reaches the UI through
    // src/error.rs from_io, which passes std::io::Error::to_string straight through, errno and all.
    if (where === "mkdir") {
        return exists(message) ? "A folder or file with that name is already here."
                               : "That folder could not be created."
    }
    // The state file: what was asked for is still on screen, so the sentence says what did not last.
    if (where === "state") {
        return "That setting could not be saved."
    }
    // And the other way round: main() left a ui.json it could not read alone, so none of it is used.
    if (where === "statefile") {
        return "Your saved settings could not be read, so these are the defaults."
    }
    if (where === "duplicate") {
        return "That file could not be duplicated."
    }
    if (where === "trash") {
        return "That could not be moved to Trash."
    }
    if (where === "transfer" || where === "archive" || where === "convert") {
        return capitalised(message)
    }
    return "That action could not be completed; try again."
}

// One condition, two spellings: rename gets the errno's "File exists", while src/backend/ops.rs
// words mkdir's own collision "a folder or file with that name already exists".
function exists(message) {
    var text = String(message).toLowerCase()
    return text.indexOf("file exists") >= 0 || text.indexOf("already exists") >= 0
}

// The backend already writes these as sentences; this only makes one read like one in the bar.
function capitalised(message) {
    var text = String(message)
    if (text.length === 0) {
        return "That action could not be completed; try again."
    }
    var out = text.charAt(0).toUpperCase() + text.substring(1)
    return out.charAt(out.length - 1) === "." ? out : out + "."
}

// The one place the backend's errno wording is read, so the sentence and the pane state can never
// disagree about which failure this is.
function denied(where, message) {
    return where === "scan" && String(message).toLowerCase().indexOf("permission denied") >= 0
}

// Which state the listing area reaches when a listing fails. A denial is the canvas's Locked tile on
// States.dc.html, which draws the lock mark over the directory's own mode string; the rest are Error.
function listingState(where, message) {
    return denied(where, message) ? "locked" : "error"
}

// That tile's one line, drawn verbatim as "rwx------ · not yours". src/backend/meta.rs answers mode 0
// when the stat itself failed, and a real st_mode always carries its file-type bits, so a zero here
// means "I could not look" and earns no line rather than a false "---------".
function lockedLine(mode) {
    if (!(mode > 0)) {
        return ""
    }
    return Format.permissions(mode) + notYours(mode)
}

// Entering a directory needs its execute bit and listing it needs its read bit, so an owner holding
// both would not have been denied: a denial on such a directory proves the operator is not the
// owner. Below that the owner is locked out too, and the line claims nothing it cannot know.
var OWNER_CAN_LIST = 0o500

function notYours(mode) {
    return (mode & OWNER_CAN_LIST) === OWNER_CAN_LIST ? " · not yours" : ""
}

// The one sentence a credentialed mount reaches the user as, lifted here in the 0.1.4 composition
// so ui/NetworkMounts.qml keeps its budget. "timeout" answers 124 for its own deadline and the shell
// answers 126 or 127 for a helper it could not run at all; every other code is the server refusing,
// which reads as the handshake for the schemes that negotiate one.
// The prefix ui/NetworkDialog.qml reads a question by, the way it reads a failure by "Connect
// failed:". A question is not a failure: the address was right, the password was never tried, and
// the only thing missing is a person saying yes to a key.
var VERIFY = "Verify: "

// What tools/flea-gio-auth printed when it refused GIO's identity question, as one sentence with
// the fingerprint in it. changed is exit 5, a key this box has seen before and that is not the same
// key today, which is the case that is not a first meeting and is worded so nobody skims past it.
function identityQuestion(text, changed, uri) {
    var body = String(text || "")
    var print = body.match(/((?:SHA256|SHA1|MD5):[A-Za-z0-9+\/=]+)/)
                || body.match(/\b([0-9a-f]{2}(?::[0-9a-f]{2}){5,})\b/i)
    var host = hostOf(uri)
    var key = print ? print[1] : "an unreadable key"
    if (changed) {
        return VERIFY + host + " answers with a DIFFERENT key than the one this computer accepted before. "
            + "It now offers " + key + ". A rebuilt server does this; so does something pretending to be it. "
            + "Connect anyway?"
    }
    return VERIFY + host + " has not been connected to from this computer before. It offers " + key
        + ". Connect anyway?"
}

function isQuestion(message) {
    return String(message || "").indexOf(VERIFY) === 0
}

// The host the uri names, for a sentence that would otherwise have to quote the whole address.
function hostOf(uri) {
    var match = String(uri || "").match(/^[a-z][a-z0-9+.-]*:\/\/(?:[^@\/]*@)?([^\/:]+)/i)
    return match ? match[1] : "that server"
}

function connectFailure(exitCode, uri) {
    if (exitCode === 124) return "Connect failed: host did not respond"
    if (exitCode === 126 || exitCode === 127)
        return "Connect failed: authentication helper is unavailable"
    // tools/flea-gio-auth's own two refusals. Neither is a wrong password, and saying so sent the
    // operator back to a password that was right: the server is the thing that was not vouched for.
    if (exitCode === 3)
        return "Connect failed: this server's identity is not known. Connect to it once in a terminal to check and accept its key, then retry."
    if (exitCode === 4)
        return "Connect failed: this server's certificate is not trusted."
    // 6: gio ended before it ever asked for a password, so the credential was never looked at.
    if (exitCode === 6)
        return "Connect failed: the server refused the connection before asking for a password"
    if (/^(ftp|ftps|dav|davs):/i.test(String(uri || "")))
        return "Connect failed: host refused the TLS handshake"
    return "Connect failed: authentication was refused"
}

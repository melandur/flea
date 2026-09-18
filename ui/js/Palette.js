.import "Contrast.js" as Contrast

// The Omarchy theme palette in colors.toml, parsed and queried. Pure, with no QML imports, so
// tests/js/palette.js can run it under qml6 with no window.

// Sample input: cyan = "#27a6a2"   # the ANSI ring
function parse(body) {
    var found = {};
    var lines = String(body || "").split("\n");
    for (var i = 0; i < lines.length; i++) {
        var kv = lines[i].match(/^\s*([A-Za-z0-9_-]+)\s*=\s*["']?(#[0-9A-Fa-f]{6})/);
        if (kv)
            found[kv[1]] = kv[2];
    }
    return found;
}

// ThemeRoles.html's surface ladder: the chrome plane, then background as the neutral fallback when
// it is absent, then selection for the alacritty-derived file that emits neither ladder key at all.
var SURFACE_KEYS = ["dark_background", "background", "selection"];

// The first key the theme actually set wins, and a role no theme models keeps Flea's own colour.
function pick(found, keys, fallback) {
    for (var i = 0; i < keys.length; i++) {
        if (found[keys[i]])
            return found[keys[i]];
    }
    return fallback;
}

// A theme need not set every role and pick() already falls back per role, so the only question a
// readiness flag can answer honestly is whether the file yielded any colour at all.
function isPalette(found) {
    return Object.keys(found).length > 0;
}

// The six roles ui/Theme.qml derives rather than binds, each with the WCAG floor it carries, out
// of one parse. It came out of that file when the 0.3.1 palette work took it to its 400-line hard
// cap, and it is one job: turn a colors.toml body into the roles no theme states outright.
//
// accent, urgent and mutedFallback are handed in because they are qs.Commons Color's, and a
// .pragma library cannot import a QML singleton (nor call Qt.darker, which is why the muted
// fallback arrives resolved rather than as a foreground to darken).
function roles(body, fallback, accent, urgent, urgentSaturation, foreground, mutedFallback) {
    var found = parse(body);
    var bg = pick(found, ["background"], fallback.background);
    var surface = pick(found, SURFACE_KEYS, fallback.surface);
    return {
        surface: surface,
        // Measured over the 22 stock palettes in tests/js/themes.js: 20 set a muted under the 3:1
        // a caption needs, rose-pine's at 1.48, so it is lifted the way the ladder colours are.
        muted: Contrast.ensureRatio(pick(found, ["muted"], mutedFallback), bg, 3),
        accentFrame: Contrast.ensureRatio(accent, surface, 3),
        symlink: Contrast.ensureRatio(pick(found, ["cyan", "color6"], fallback.symlink), bg, 4.5),
        executable: Contrast.ensureRatio(pick(found, ["green", "color2"], fallback.executable), bg, 4.5),
        // Urgent is the palette's own red: seven of the 23 installed themes leave it under 4.5:1 on
        // their own ground, so it is lifted the way symlink and executable are, and the three whose
        // red carries no chroma at all (solitude, white, vantablack) fall back to the foreground,
        // because a destructive row drawn in the same grey as an unavailable one reads as switched
        // off rather than as dangerous.
        error: urgentSaturation > 0.2 ? Contrast.ensureRatio(urgent, bg, 4.5) : String(foreground),
        // A body that parsed to nothing left every role on its fallback, so the flag says so rather
        // than reporting that the read happened: text() returns "" for a file that is not there.
        ready: isPalette(found)
    };
}

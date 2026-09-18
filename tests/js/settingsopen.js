.import "../../ui/js/SettingsOpen.js" as OpenSection
.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Nav.js" as Nav

// Settings > File types: the table's shape, its match and the rows the section draws. The same
// contract src/openrules.rs holds in Rust, and the tests there are deliberately the same cases:
// this half decides what the panel will write and that half decides what an open reads back, so a
// disagreement between them is a rule the operator can save and that never opens anything.
function state(rules) {
    return { open: { rules: rules } }
}

function run(check) {
    // An ending is a dot and lowercase name parts, and normalised() is what the card types into it.
    for (var good of [".nii", ".nii.gz", ".mha", ".tar.gz", ".c++", ".my-notes", ".f90", ".x_y"])
        check("an ending may be " + good, OpenSection.isEnding(good), true)
    for (var bad of ["", ".", "nii", ".NII", ".nii.", ".nii..gz", "..nii", ".nii/../etc", ".nii gz",
                     ".012345678901234567890123456789012"])
        check("an ending may not be “" + bad + "”", OpenSection.isEnding(bad), false)
    check("a typed ending wears the dot it was left without", OpenSection.normalised("nii.gz"), ".nii.gz")
    check("and is folded, because the match is", OpenSection.normalised(" .NII.GZ "), ".nii.gz")
    check("an empty field normalises to nothing rather than to a bare dot", OpenSection.normalised("  "), "")

    // An application is a desktop id and never a path: it becomes a path component in src/open.rs.
    check("an application is a desktop id", OpenSection.isApp("itksnap.desktop"), true)
    check("a reverse-dns id is one too", OpenSection.isApp("org.gnome.Loupe.desktop"), true)
    for (var wrong of ["itksnap", ".desktop", "/usr/share/applications/itksnap.desktop",
                       "../itksnap.desktop", "itksnap.desktop/x"])
        check("but not “" + wrong + "”", OpenSection.isApp(wrong), false)

    // The stored table drops whatever is not a whole rule, so a hand-edited file draws the rules
    // this build understands instead of a broken row or an empty section.
    var mixed = state([{ ends: ".nii", app: "itksnap.desktop" }, { ends: "nii", app: "x.desktop" },
                       { ends: ".mha" }, "itksnap.desktop", { ends: ".mha", app: "itksnap" }])
    check("only whole rules are read back", JSON.stringify(OpenSection.rules(mixed)),
          '[{"ends":".nii","app":"itksnap.desktop"}]')
    check("and nothing stored reads as an empty table", OpenSection.rules({}).length, 0)

    // The match: longest ending first, folded, and a dotfile is not an ending's own file.
    var table = state([{ ends: ".gz", app: "file-roller.desktop" },
                       { ends: ".nii.gz", app: "itksnap.desktop" },
                       { ends: ".nii", app: "itksnap.desktop" }])
    check("the longest ending answers", OpenSection.chosen(table, "brain.nii.gz"), "itksnap.desktop")
    check("whatever the case of the name", OpenSection.chosen(table, "BRAIN.NII.GZ"), "itksnap.desktop")
    check("a shorter ending still answers its own files", OpenSection.chosen(table, "photos.tar.gz"), "file-roller.desktop")
    check("an ending nothing claims answers nothing", OpenSection.chosen(table, "notes.txt"), "")
    check("and a dotfile named only for the ending is not one of its files", OpenSection.chosen(table, ".nii"), "")

    // One ending holds one application, so saving over one replaces it; the list stays in ending
    // order, which is not the order rules were added in.
    var written = OpenSection.withRule(OpenSection.rules(table), "NII.GZ", "org.gnome.Loupe.desktop")
    check("a saved rule replaces the ending's own", JSON.stringify(written),
          '[{"ends":".gz","app":"file-roller.desktop"},{"ends":".nii","app":"itksnap.desktop"},{"ends":".nii.gz","app":"org.gnome.Loupe.desktop"}]')
    check("a rule the card could not have built is refused rather than written",
          OpenSection.withRule([], ".nii", "itksnap"), null)
    check("and so is one with no ending at all", OpenSection.withRule([], "", "itksnap.desktop"), null)
    check("removing one takes that row and no other",
          JSON.stringify(OpenSection.without(OpenSection.rules(table), 1)),
          '[{"ends":".gz","app":"file-roller.desktop"},{"ends":".nii","app":"itksnap.desktop"}]')

    // The rows, which is what ui/SettingsPanel.qml operates and ui/SettingsRow.qml draws.
    var apps = [{ id: "itksnap.desktop", label: "ITK-SNAP" }]
    var rows = OpenSection.rows(state([{ ends: ".nii.gz", app: "itksnap.desktop" }]), apps)
    check("the heading carries the section's one action", rows[0].kind + "/" + rows[0].action, "group/addOpenRule")
    check("a rule row names its ending", rows[1].label, ".nii.gz")
    check("and the application's own name once the catalogue has landed", rows[1].value, "ITK-SNAP")
    check("the row carries the index the remove writes", rows[1].ruleIndex, 0)
    check("a rule row is a stop the cursor can reach", Settings.focusable(rows[1]), true)
    check("the section ends in its own hint", rows[rows.length - 1].kind, "hint")
    // Before the catalogue lands the id is what there is, and a row that said nothing would read as
    // a rule pointing at nothing at all.
    check("without a catalogue the row still names something", OpenSection.rows(state([{ ends: ".nii", app: "itksnap.desktop" }]), []) [1].value, "itksnap")
    // What Enter does with a claimed ending, which is the half of this contract that reaches the
    // listing. It is asserted here rather than in tests/js/nav.js because that suite is already at
    // its 300-line hard cap, and the rule is this section's own: the archive route would otherwise
    // take every .nii.gz before src/open.rs ever saw the table, since a gzip stream is what every
    // classifier on the box reads one as.
    function entered(row, data) {
        var went = { navigated: "", previewed: "", opened: "" }
        var pane = {
            listInFlight: false, path: "/scans", shown: null, cursorIndex: 0, kindNames: [],
            rowFor: function () { return row },
            join: function (base, name) { return base + "/" + name },
            open: function (target) { went.navigated = target },
            preview: { open: function (path) { went.previewed = path } }
        }
        Nav.openCursor(pane, { open: function (path) { went.opened = path } }, data)
        return went
    }

    var volume = { n: "brain.nii.gz", d: false, i: "package-x-generic", s: 12 }
    var claimed = entered(volume, state([{ ends: ".nii.gz", app: "itksnap.desktop" }]))
    check("Enter on a claimed ending hands the file to the opener", claimed.opened, "/scans/brain.nii.gz")
    check("and opens no archive view over it", claimed.previewed, "")
    var unclaimed = entered(volume, state([]))
    check("with no rule the same row is still Flea's own archive view", unclaimed.previewed, "/scans/brain.nii.gz")
    check("and nothing is handed on", unclaimed.opened, "")
    var text = entered({ n: "notes.txt", d: false, i: "text-plain", s: 4 }, state([{ ends: ".nii.gz", app: "itksnap.desktop" }]))
    check("a row no rule names opens the way it always did", text.opened, "/scans/notes.txt")

    var empty = OpenSection.rows(state([]), apps)
    check("an empty table draws the heading and one hint", empty.length, 2)
    check("and the hint says what a rule is for", empty[1].label.indexOf("one ending to one application") > 0, true)
}

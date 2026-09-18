.import "../../ui/js/DirSizes.js" as DirSizes

// A held window starting at 10, four rows, the third of which is a file, not a directory.
function heldRows() {
    return [{ n: "a", d: true }, { n: "b", d: true }, { n: "c", d: false }, { n: "d", d: true }]
}

function run(check) {
    var rows = heldRows()
    var s = DirSizes.empty()

    var ask = DirSizes.plan(s, rows, 10, 10, 13)
    check("only directory rows are asked for", ask.join(","), "10,11,13")

    check("a fresh listing has nothing pending to cancel", DirSizes.hasPending(s), false)
    s = DirSizes.applied(s, ask)
    check("a row already asked is not asked twice", DirSizes.plan(s, rows, 10, 10, 13).length, 0)
    check("a row asked and still waiting reports no size", DirSizes.sizeFor(s, 10), null)
    check("a row waiting on an answer is pending", DirSizes.hasPending(s), true)

    s = DirSizes.remember(s, 10, 4096, false, 12, 240)
    check("an answered row reports its bytes", DirSizes.sizeFor(s, 10).bytes, 4096)
    check("and reports whether it is partial", DirSizes.sizeFor(s, 10).partial, false)
    // The same answer carries the directory's own child count, which the Items column draws.
    check("and the count of the children the same walk saw", DirSizes.sizeFor(s, 10).entries, 12)
    check("and is never asked for again", DirSizes.plan(s, rows, 10, 10, 10).length, 0)
    check("rows 11 and 13 are still pending", DirSizes.hasPending(s), true)

    // A scroll cancels everything still pending, not a diff against the new viewport: the backend's
    // own dirsizecancel always means "everything queued", so the client's pending set must match.
    s = DirSizes.cancelled(s)
    check("a cancelled row can be asked for again", DirSizes.plan(s, rows, 10, 11, 11).join(","), "11")
    check("an answered row survives a cancel", DirSizes.sizeFor(s, 10).bytes, 4096)
    check("the cancelled pending row leaves the order too", s.order.length, 1)
    check("a cancel clears every pending row, so there is nothing left to cancel again", DirSizes.hasPending(s), false)

    check("an unknown row reports no size", DirSizes.sizeFor(DirSizes.empty(), 4), null)
    check("a file row is never asked for", DirSizes.plan(DirSizes.empty(), rows, 10, 12, 12).length, 0)
    check("a row outside the held window is not asked for", DirSizes.plan(DirSizes.empty(), rows, 10, 0, 3).length, 0)

    // The cap bounds a policy bug: normal use only ever records the rows a viewport held.
    var capped = DirSizes.empty()
    for (var i = 0; i < 5; i++) {
        capped = DirSizes.remember(capped, i, 1000 + i, false, -1, 3)
    }
    check("the cap evicts the oldest entry", DirSizes.sizeFor(capped, 0), null)
    check("and keeps the newest", DirSizes.sizeFor(capped, 4).bytes, 1004)
    check("and holds exactly the cap", capped.order.length, 3)

    capped = DirSizes.remember(capped, 4, 2004, true, 7, 3)
    check("answering one row twice does not grow the order", capped.order.length, 3)
    check("and the newer answer wins", DirSizes.sizeFor(capped, 4).bytes, 2004)
    check("including its partial flag", DirSizes.sizeFor(capped, 4).partial, true)
    check("and its count, which a partial walk still answers exactly", DirSizes.sizeFor(capped, 4).entries, 7)
    // -1 off the wire is a directory whose entries could not be read; null is what the cell reads
    // as "nothing to say", where 0 would claim a folder was measured and found empty.
    check("an unreadable directory records no count rather than a zero", DirSizes.sizeFor(capped, 3).entries, null)

    check("each empty state is its own", DirSizes.empty() === DirSizes.empty(), false)
}

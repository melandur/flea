// How bsdtar's listing is shaped: which whitespace field holds the size, where the name starts, and
// how a directory is marked, with the exact line it was derived from. bsdtar is the one tool that
// reads archives here, .7z and .rar included. What a parse produces is archivelist.rs's job.

// bsdtar's own verbose listing puts the size in the fifth column, after the mode, the link count,
// the owner and the group.
// Sample input: -rw-r--r--  0 gm     gm         10 Sep  1 09:14 a.txt
const BSDTAR_SIZE_COLUMN: usize = 4;
// mode, links, owner, group, size, month, day, time, then the name, which may hold spaces.
const BSDTAR_NAME_AFTER: usize = 8;

// How the listing is read: which whitespace field holds the size, and how many come before the name.
// A directory is a mode string in field 0 beginning with d.
pub struct ListSpec {
    pub size_column: usize,
    pub name_after_fields: usize,
}

pub fn tar_spec() -> ListSpec {
    ListSpec { size_column: BSDTAR_SIZE_COLUMN, name_after_fields: BSDTAR_NAME_AFTER }
}

// Sample input, bsdtar -tvf: "-rw-r--r--  0 gm     gm          2 Sep  1 12:10 ./name with spaces.txt"
// One line, read once, borrowing rather than allocating: the streaming path runs this for every line
// of a 200k-entry index while an owned Entry is only built for the first few that reach the wire.
pub struct Row<'a> {
    // Trailing slash already trimmed, so "./a/" reads as "./a".
    pub name: &'a str,
    pub is_dir: bool,
}

impl Row<'_> {
    // THE INVARIANT the extract check is built on: a member produces an entry in the destination
    // unless it IS the destination. Extracting the archive root creates nothing new; every other
    // member, directory or not, produces something. "./a/" is a destination entry and "./" is not,
    // which is the distinction three predicates missed in turn.
    pub fn produces_destination_entry(&self) -> bool {
        !is_archive_root(self.name)
    }
}

// The root has more than one spelling and a predicate that knew only one refused a legal archive:
// bsdtar writes "./" for `-C dir .` and "././" for `-C dir ./.`, both of which extract to nothing.
// A name is the root when it names no component at all, so every part of it is empty or ".".
fn is_archive_root(name: &str) -> bool {
    name.split('/').all(|part| part.is_empty() || part == ".")
}

// Sample input, bsdtar -tvf: "drwxr-xr-x  0 gm     gm          0 Sep  1 12:10 ./sub/"
pub fn row_of<'a>(line: &'a str, spec: &ListSpec) -> Option<Row<'a>> {
    let is_dir = line.trim_start().starts_with('d');
    let name = nth_field_rest(line, spec.name_after_fields)?;
    // Trimmed before the emptiness test, not after: a member named "/" survived the old order and
    // reached the wire as a preview row with no label on it.
    let name = name.trim_end_matches('/');
    if name.is_empty() {
        return None;
    }
    Some(Row { name, is_dir })
}


// Everything after the first `skip` whitespace-separated fields, with the name's own spaces kept.
fn nth_field_rest(line: &str, skip: usize) -> Option<&str> {
    let mut rest = line.trim_start();
    for _ in 0..skip {
        let end = rest.find(char::is_whitespace)?;
        rest = rest[end..].trim_start();
    }
    Some(rest)
}



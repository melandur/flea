// A path joined to a name, and a path split into its parent and leaf: the two string rules every
// navigation shares, out of pane.rs whole.

pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub(crate) fn parent(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let (dir, leaf) = trimmed.rsplit_once('/')?;
    Some((if dir.is_empty() { "/".into() } else { dir.into() }, leaf.into()))
}

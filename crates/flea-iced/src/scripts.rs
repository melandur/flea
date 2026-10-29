// MenuAdditions rule 2, as ui/Scripts.qml and ui/js/Scripts.js hold it: the executables in
// ~/.config/flea/scripts, one level down and nothing else, read when a menu opens rather than
// watched, and run with the selected paths as arguments in the first one's folder.
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn directory() -> PathBuf {
    let config = std::env::var("XDG_CONFIG_HOME").ok().filter(|v| !v.is_empty()).map(PathBuf::from);
    config.unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")).join("flea/scripts")
}

// find -maxdepth 1 -type f -perm -u+x: a regular file, not followed through a link, with the owner's
// execute bit. Sorted by name, each labelled with its name less the extension.
pub fn list() -> Vec<(String, String)> {
    let Ok(dir) = std::fs::read_dir(directory()) else { return vec![] };
    let mut names: Vec<String> = dir
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && e.metadata().is_ok_and(|m| m.permissions().mode() & 0o100 != 0))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names.into_iter().map(|n| (n.clone(), label(&n))).collect()
}

fn label(name: &str) -> String {
    match name.rfind('.') {
        Some(cut) if cut > 0 => name[..cut].to_string(),
        _ => name.to_string(),
    }
}

// The script's own report: its last stderr line, or the status it exited with when it said nothing.
pub fn failure(name: &str, stderr: &str, code: Option<i32>) -> String {
    match stderr.lines().map(str::trim).filter(|l| !l.is_empty()).last() {
        Some(line) => format!("{name} · {line}"),
        None => format!("{name} exited with status {}", code.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into())),
    }
}

// Runs `id` from the scripts folder over `paths`; Ok on a zero exit, else the sentence to say.
pub fn run(id: &str, paths: &[String]) -> Result<(), String> {
    let script = directory().join(id);
    if paths.is_empty() || id.contains('/') || !script.is_file() {
        return Err("That script is no longer there.".into());
    }
    let cwd = paths[0].rsplit_once('/').map(|(d, _)| if d.is_empty() { "/" } else { d }).unwrap_or("/").to_string();
    let out = std::process::Command::new(&script).args(paths).current_dir(cwd).stdin(std::process::Stdio::null()).output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(failure(id, &String::from_utf8_lossy(&o.stderr), o.status.code())),
        Err(_) => Err(format!("{id} could not be started.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_is_the_last_stderr_line_or_the_status() {
        assert_eq!(failure("ocr.sh", "one\ntwo\n\n", Some(1)), "ocr.sh · two");
        assert_eq!(failure("ocr.sh", "  \n", Some(3)), "ocr.sh exited with status 3");
        assert_eq!(label("ocr.sh"), "ocr");
        assert_eq!(label("convert-to-webp"), "convert-to-webp");
        assert_eq!(label(".hidden"), ".hidden");
    }
}

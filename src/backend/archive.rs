// Which archive formats this box can actually write and read, probed once at startup, and the argv
// each job needs. One tool does all of it: bsdtar, whose libarchive also reads .7z and .rar, so no
// second archiver is needed for either. The jobs themselves are archiveops.rs and reading an index is
// archivelist.rs.
use crate::backend::archivespec::{tar_spec, ListSpec};
use std::path::Path;

const BSDTAR: &str = "bsdtar";

// The formats bsdtar writes on this box, in the order the submenu offers them.
const TAR_FORMATS: &[&str] = &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"];

pub struct Formats {
    names: Vec<String>,
    have_bsdtar: bool,
}

fn on_path(prog: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(prog).is_file())
}

impl Formats {
    pub fn probe() -> Formats {
        Formats::from_tools(on_path(BSDTAR))
    }

    // Nothing writes rar or 7z here: rar needs WinRAR's nonfree tool, and 7z is not offered because
    // nobody asked for it. Reading either is libarchive's, see extract_argv below.
    pub fn from_tools(have_bsdtar: bool) -> Formats {
        let names = if have_bsdtar { TAR_FORMATS.iter().map(|s| s.to_string()).collect() } else { Vec::new() };
        Formats { names, have_bsdtar }
    }

    // Exactly the table, which is what the compress submenu draws; an empty one self-hides the entry.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn offers(&self, format: &str) -> bool {
        self.names.iter().any(|n| n == format)
    }

    // Whether anything can read an archive at all, which is what the Extract row asks.
    pub fn can_extract(&self) -> bool {
        self.have_bsdtar
    }

    // bsdtar's -a picks the compression from the destination's own extension, so one argv covers
    // every tar flavour and zip. -v names each member on stderr as it is added, which is how a
    // compress reports progress; see archivework.rs.
    // corner: bsdtar reads -C positionally, so it has to precede the names it applies to. Putting it
    // after them archives nothing, prints "Cannot stat" per name, and still leaves an empty archive.
    pub fn compress_argv(&self, format: &str, dest: &Path, parent: &Path, names: &[String]) -> Option<Vec<String>> {
        if !self.offers(format) {
            return None;
        }
        let mut a: Vec<String> = Vec::with_capacity(names.len() + 7);
        a.push(BSDTAR.to_string());
        a.push("-a".to_string());
        a.push("-c".to_string());
        a.push("-f".to_string());
        a.push(dest.to_string_lossy().to_string());
        a.push("-C".to_string());
        a.push(parent.to_string_lossy().to_string());
        // Members are relative names from the selection; one that begins with a dash would be read
        // as a bsdtar option (`--use-compress-program` runs an arbitrary program), so option parsing
        // is terminated first, the same way trash.rs does before its own paths.
        a.push("--".to_string());
        a.extend(names.iter().cloned());
        Some(a)
    }

    // Extraction is the same tool whatever the archive is: libarchive reads zip, every tar flavour,
    // 7z and rar, and recognises the format from the bytes rather than the name.
    pub fn extract_argv(&self, archive: &Path, dest: &Path) -> Option<Vec<String>> {
        if !self.have_bsdtar {
            return None;
        }
        // bsdtar's own secure-extraction default refuses .. components and absolute paths; the live
        // battery proves that on this build rather than trusting it.
        Some(vec![
            BSDTAR.to_string(),
            "-x".to_string(),
            "-f".to_string(),
            archive.to_string_lossy().to_string(),
            "-C".to_string(),
            dest.to_string_lossy().to_string(),
        ])
    }

    // The argv that lists an archive without extracting a byte of it, by the same tool that extracts
    // it, so a name in the preview is a name the extract will actually write.
    pub fn list_argv(&self, archive: &Path) -> Option<(Vec<String>, ListSpec)> {
        if !self.have_bsdtar {
            return None;
        }
        Some((
            vec![BSDTAR.to_string(), "-t".to_string(), "-v".to_string(), "-f".to_string(),
                 archive.to_string_lossy().to_string()],
            tar_spec(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_what_is_installed_and_never_a_fixed_list() {
        let tar = Formats::from_tools(true);
        assert_eq!(tar.names(), &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"]);
        assert!(!tar.offers("7z"), "7z is never offered as something to write");
        assert!(!tar.offers("rar"), "and neither is rar");
        let nothing = Formats::from_tools(false);
        assert!(nothing.names().is_empty(), "with no tool at all the whole entry self-hides");
        assert!(!nothing.can_extract());
    }

    // 7z and rar are read, never written, and by the one tool every other archive is read by.
    #[test]
    fn a_7z_and_a_rar_are_read_by_bsdtar() {
        let f = Formats::from_tools(true);
        for name in ["/x/holiday.rar", "/x/HOLIDAY.RAR", "/x/a.7z"] {
            assert_eq!(f.extract_argv(Path::new(name), Path::new("/d")).unwrap()[0], "bsdtar");
            assert_eq!(f.list_argv(Path::new(name)).unwrap().0[0], "bsdtar");
        }
        assert!(Formats::from_tools(false).extract_argv(Path::new("/x/a.rar"), Path::new("/d")).is_none());
    }

    #[test]
    fn a_format_the_table_does_not_offer_builds_no_argv_at_all() {
        let f = Formats::from_tools(true);
        assert!(f.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["a".to_string()]).is_none());
        assert!(f.compress_argv("rar", Path::new("/x/out.rar"), Path::new("/src"), &["a".to_string()]).is_none());
    }

    #[test]
    fn bsdtar_lets_the_destination_extension_choose_the_compression() {
        let f = Formats::from_tools(true);
        let a = f.compress_argv("tar.zst", Path::new("/x/out.tar.zst"), Path::new("/src"),
                                &["a.txt".to_string(), "b".to_string()]).unwrap();
        assert_eq!(a[0], "bsdtar");
        assert!(a.contains(&"-a".to_string()), "-a is what reads the extension");
        assert_eq!(a.last().unwrap(), "b");
        // The names are relative, so the archive holds "a.txt" and not "/src/a.txt".
        assert!(!a.iter().any(|s| s.starts_with("/src/")));
        // -C is positional: after the names it applies to nothing and the archive comes out empty.
        let dash_c = a.iter().position(|s| s == "-C").expect("a working directory");
        let first_name = a.iter().position(|s| s == "a.txt").expect("the first name");
        assert!(dash_c < first_name, "-C must precede the names it applies to");
    }

    #[test]
    fn a_member_name_that_looks_like_an_option_sits_after_the_terminator() {
        let f = Formats::from_tools(true);
        let a = f
            .compress_argv("tar", Path::new("/x/out.tar"), Path::new("/src"),
                           &["--use-compress-program=sh".to_string(), "ok.txt".to_string()])
            .unwrap();
        let term = a.iter().position(|s| s == "--").expect("a -- terminator before the names");
        let bad = a.iter().position(|s| s == "--use-compress-program=sh").unwrap();
        assert!(term < bad, "a dash-leading member must sit after the -- terminator");
    }

    #[test]
    fn the_listing_and_the_extract_are_the_same_tool() {
        let f = Formats::from_tools(true);
        let (tar, spec) = f.list_argv(Path::new("/x/a.tar.zst")).unwrap();
        assert_eq!(tar[0], "bsdtar");
        assert_eq!(spec.size_column, tar_spec().size_column);
        assert_eq!(spec.name_after_fields, tar_spec().name_after_fields);
        let x = f.extract_argv(Path::new("/x/a.tar.zst"), Path::new("/out")).unwrap();
        assert!(x.contains(&"-C".to_string()));
        assert!(Formats::from_tools(false).list_argv(Path::new("/x/a.zip")).is_none());
    }
}

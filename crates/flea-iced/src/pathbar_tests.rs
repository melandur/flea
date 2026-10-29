use super::*;

#[test]
fn a_typed_line_resolves_the_tilde_relative_names_and_dots() {
    assert_eq!(resolve("~/Downloads", "/x", "/home/u"), "/home/u/Downloads");
    assert_eq!(resolve("~", "/x", "/home/u"), "/home/u");
    assert_eq!(resolve("sub/../other/./x", "/home/u", "/home/u"), "/home/u/other/x");
    assert_eq!(resolve("/a//b/", "/x", ""), "/a/b");
    assert_eq!(resolve("/..", "/x", ""), "/");
    assert_eq!(resolve("   ", "/x", ""), "");
}

#[test]
fn a_file_uri_is_a_path_here_and_refused_on_another_host() {
    assert_eq!(resolve("file:///home/u/a%20b", "/x", ""), "/home/u/a b");
    assert_eq!(resolve("file://localhost/etc", "/x", ""), "/etc");
    assert_eq!(resolve("file://nas/etc", "/x", ""), "");
    assert!(refused("file://nas/etc") && !refused("") && !refused("/etc"));
}

#[test]
fn tab_grows_to_the_common_prefix_and_a_single_match_gains_its_slash() {
    let names: Vec<String> = ["Downloads", "Documents", "Desktop", "music"].iter().map(|s| s.to_string()).collect();
    assert_eq!(complete("/home/u/Do", &names), ("/home/u/Do".into(), 2));
    assert_eq!(complete("/home/u/Dow", &names), ("/home/u/Downloads/".into(), 1));
    // Case-sensitive first, folded only when that finds nothing.
    assert_eq!(complete("/home/u/MU", &names), ("/home/u/music/".into(), 1));
    assert_eq!(complete("/home/u/zz", &names), ("/home/u/zz".into(), 0));
    assert_eq!(completion_message("/home/u/Do", &complete("/home/u/Do", &names), "/home/u"), Some("2 names share that prefix.".into()));
    assert_eq!(completion_dir("~/Dow", "/x", "/home/u"), "/home/u");
    assert_eq!(completion_dir("Dow", "/x", "/home/u"), "/x");
    assert!(wants_hidden("/home/u/.co", false) && !wants_hidden("/home/u/co", false));
}

#[test]
fn crumbs_name_home_and_root_and_the_last_carries_no_separator() {
    let c = crumbs("/home/u/Pictures/2026", "/home/u");
    let texts: Vec<_> = c.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, vec!["~/", "Pictures/", "2026"]);
    assert_eq!(c[0].path, "/home/u");
    assert_eq!(c[2].path, "/home/u/Pictures/2026");
    assert!(c[2].last && !c[1].last);
    let r = crumbs("/etc/x", "/home/u");
    assert_eq!(r.iter().map(|c| (c.text.as_str(), c.path.as_str())).collect::<Vec<_>>(), vec![("/", "/"), ("etc/", "/etc"), ("x", "/etc/x")]);
    assert_eq!(crumbs("/", "/home/u"), vec![Crumb { text: "/".into(), path: "/".into(), last: true }]);
    assert_eq!(crumbs("/home/u", "/home/u")[0].text, "~");
}

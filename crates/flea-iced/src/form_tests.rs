use super::*;

fn form(protocol: &str, host: &str) -> Form {
    let mut f = Form::default();
    f.pick(protocol);
    f.host = host.into();
    f
}

#[test]
fn a_uri_writes_no_default_port_and_encodes_each_path_part() {
    let mut f = form("SMB", "nas");
    f.path = "my share/sub dir".into();
    assert_eq!(f.uri(), "smb://nas/my%20share/sub%20dir");
    f.port = "1445".into();
    assert_eq!(f.uri(), "smb://nas:1445/my%20share/sub%20dir");
    f.path.clear();
    f.port = "445".into();
    assert_eq!(f.uri(), "smb://nas/");
}

#[test]
fn credentials_carry_a_domain_only_for_smb_and_nfs_carries_none() {
    let mut f = form("SMB", "nas");
    f.user = "me@x".into();
    f.domain = "WORK".into();
    assert_eq!(f.uri(), "smb://WORK;me%40x@nas/");
    let mut s = form("SFTP", "h");
    s.user = "u".into();
    s.domain = "ignored".into();
    assert_eq!(s.uri(), "sftp://u@h/");
    let mut n = form("NFS", "h");
    n.user = "u".into();
    n.path = "/export".into();
    assert_eq!(n.uri(), "nfs://h/export");
}

#[test]
fn the_tls_box_moves_the_scheme_and_its_port_together() {
    // advloop round 2: unticking TLS on WebDAV used to leave 443 on a dav:// URI.
    let mut f = form("WebDAV", "h");
    assert_eq!((f.port.as_str(), f.uri().as_str()), ("443", "davs://h/"));
    f.set_tls(false);
    assert_eq!((f.port.as_str(), f.uri().as_str()), ("80", "dav://h/"));
    let mut t = form("FTPS", "h");
    t.set_tls(false);
    assert_eq!(t.uri(), "ftp://h/");
}

#[test]
fn an_invalid_host_or_port_builds_nothing() {
    assert_eq!(form("SMB", "bad host").uri(), "");
    let mut f = form("SMB", "nas");
    f.port = "70000".into();
    assert!(!f.complete() && f.uri().is_empty());
    assert!(valid_host("[fe80::1]") && !valid_host("[]") && !valid_host(""));
}

#[test]
fn a_saved_uri_reopens_the_form_with_its_own_port_and_user() {
    let f = values_for("dav://u@h:443/a%20b", "L", "");
    assert_eq!((f.protocol.as_str(), f.tls, f.port.as_str(), f.user.as_str(), f.path.as_str()), ("WebDAV", false, "443", "u", "a b"));
    let g = values_for("smb://WORK;me@nas/share", "", "");
    assert_eq!((g.domain.as_str(), g.user.as_str(), g.host.as_str(), g.port.as_str()), ("WORK", "me", "nas", "445"));
    let v6 = values_for("sftp://[fe80::1]:2222/x", "", "");
    assert_eq!((v6.host.as_str(), v6.port.as_str()), ("[fe80::1]", "2222"));
    // And it round-trips: the reopened form builds the URI it came from.
    assert_eq!(values_for("smb://WORK;me@nas/share", "", "").uri(), "smb://WORK;me@nas/share");
}

#[test]
fn the_label_falls_back_to_the_path_leaf_then_the_host() {
    let mut f = form("SMB", "nas");
    assert_eq!(f.label_text(), "nas");
    f.path = "data/".into();
    assert_eq!(f.label_text(), "data");
    f.label = " Mine ".into();
    assert_eq!(f.label_text(), "Mine");
}

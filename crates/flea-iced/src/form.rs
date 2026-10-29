// The connect form's model: ui/NetworkForm.qml's fields and ui/js/Protocols.js's rules for turning
// them into one URI, plus ui/NetworkDialog.qml valuesFor, which takes a saved URI back apart so Edit
// and a password retry reopen the form on the place they are about.
use crate::protocols::default_port;

pub const PROTOCOLS: [&str; 5] = ["SMB", "SFTP", "FTPS", "WebDAV", "NFS"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub path_label: &'static str,
    pub credentials: bool,
    pub domain: bool,
    pub tls: bool,
}

pub fn spec(protocol: &str) -> Spec {
    let s = |path_label, credentials, domain, tls| Spec { path_label, credentials, domain, tls };
    match protocol {
        "SFTP" => s("Path", true, false, false),
        "FTPS" => s("Path", true, false, true),
        "WebDAV" => s("Path", true, false, true),
        "NFS" => s("Export", false, false, false),
        _ => s("Share", true, true, false),
    }
}

pub fn scheme(protocol: &str, tls: bool) -> &'static str {
    match protocol {
        "SFTP" => "sftp",
        "FTPS" => if tls { "ftps" } else { "ftp" },
        "WebDAV" => if tls { "davs" } else { "dav" },
        "NFS" => "nfs",
        _ => "smb",
    }
}

// The port the chip and the box prefill: the scheme's own, from the one table gio's dedup reads.
pub fn port_for(protocol: &str, tls: bool) -> u16 {
    default_port(scheme(protocol, tls))
}

// encodeURIComponent: everything but A-Z a-z 0-9 - _ . ! ~ * ' ( ) is escaped, a UTF-8 byte at a time.
pub fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn valid_host(host: &str) -> bool {
    let bracketed = host.len() > 2
        && host.starts_with('[')
        && host.ends_with(']')
        && host[1..host.len() - 1].chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
    let plain = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    bracketed || plain
}

pub fn valid_port(port: &str) -> bool {
    port.is_empty() || (port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u32>().is_ok_and(|n| (1..=65535).contains(&n)))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub protocol: String,
    pub label: String,
    pub host: String,
    pub port: String,
    pub path: String,
    pub domain: String,
    pub user: String,
    pub password: String,
    pub tls: bool,
}

impl Default for Form {
    fn default() -> Self {
        Form {
            protocol: "SMB".into(),
            label: String::new(),
            host: String::new(),
            port: port_for("SMB", true).to_string(),
            path: String::new(),
            domain: String::new(),
            user: String::new(),
            password: String::new(),
            tls: true,
        }
    }
}

impl Form {
    pub fn spec(&self) -> Spec {
        spec(&self.protocol)
    }

    // NetworkForm.pick: the port follows the scheme, because it is that scheme's default.
    pub fn pick(&mut self, protocol: &str) {
        self.protocol = protocol.to_string();
        self.port = port_for(protocol, self.tls).to_string();
    }

    // onTlsChanged: the box picks half the scheme, so it picks half the port.
    pub fn set_tls(&mut self, tls: bool) {
        self.tls = tls;
        self.port = port_for(&self.protocol, tls).to_string();
    }

    pub fn complete(&self) -> bool {
        valid_host(self.host.trim()) && valid_port(self.port.trim())
    }

    // Protocols.uri: "" while the host or port is not valid; a default port is never written.
    pub fn uri(&self) -> String {
        let (host, port) = (self.host.trim(), self.port.trim());
        if !valid_host(host) || !valid_port(port) {
            return String::new();
        }
        let spec = self.spec();
        let mut head = format!("{}://", scheme(&self.protocol, self.tls));
        if spec.credentials {
            let user = self.user.trim();
            if !user.is_empty() {
                let domain = if spec.domain { self.domain.trim() } else { "" };
                if !domain.is_empty() {
                    head += &format!("{};", encode_component(domain));
                }
                head += &format!("{}@", encode_component(user));
            }
        }
        head += host;
        if !port.is_empty() && port.parse::<u16>().ok() != Some(port_for(&self.protocol, self.tls)) {
            head += &format!(":{port}");
        }
        let path = self.path.trim();
        if path.is_empty() {
            return head + "/";
        }
        let encoded = path.split('/').map(encode_component).collect::<Vec<_>>().join("/");
        if encoded.starts_with('/') { head + &encoded } else { format!("{head}/{encoded}") }
    }

    // Protocols.label: the typed label, else the path's last part, else the host.
    pub fn label_text(&self) -> String {
        let given = self.label.trim();
        if !given.is_empty() {
            return given.to_string();
        }
        let path = self.path.trim_end_matches('/');
        let leaf = path.rsplit('/').next().unwrap_or(path);
        if leaf.is_empty() { self.host.trim().to_string() } else { leaf.to_string() }
    }
}

fn decoded(s: &str) -> String {
    crate::rail::decode_path(s)
}

// NetworkDialog.valuesFor: a saved URI back into the form. The box is set before the port, because
// the port the place carries must win over the scheme's default the box would prefill.
pub fn values_for(uri: &str, label: &str, password: &str) -> Form {
    let mut f = Form { label: label.to_string(), password: password.to_string(), ..Form::default() };
    let Some(mark) = uri.find("://") else { return f };
    let scheme = uri[..mark].to_ascii_lowercase();
    let protocol = match scheme.as_str() {
        "sftp" => "SFTP",
        "ftp" | "ftps" => "FTPS",
        "dav" | "davs" => "WebDAV",
        "nfs" => "NFS",
        _ => "SMB",
    };
    let tls = scheme != "ftp" && scheme != "dav";
    f.protocol = protocol.to_string();
    f.tls = tls;
    let rest = &uri[mark + 3..];
    let (authority, path) = rest.split_once('/').map(|(a, p)| (a, decoded(p))).unwrap_or((rest, String::new()));
    f.path = path;
    let (user_info, host_port) = authority.rsplit_once('@').unwrap_or(("", authority));
    f.host = host_port.to_string();
    f.port = port_for(protocol, tls).to_string();
    if host_port.starts_with('[') {
        if let Some(b) = host_port.find(']').filter(|b| host_port[b + 1..].starts_with(':')) {
            f.host = host_port[..=b].to_string();
            f.port = host_port[b + 2..].to_string();
        }
    } else if let Some(c) = host_port.rfind(':') {
        f.host = host_port[..c].to_string();
        f.port = host_port[c + 1..].to_string();
    }
    f.user = decoded(user_info);
    if protocol == "SMB" {
        if let Some((d, u)) = user_info.split_once(';') {
            f.domain = decoded(d);
            f.user = decoded(u);
        }
    }
    f
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;

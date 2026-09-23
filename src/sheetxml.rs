// Just enough XML to read a spreadsheet's own parts: tags, their attributes and the text between
// them, with prefixes dropped, because one writer spells <row> and another <x:row> for the same
// element. No DTD, no namespaces resolved, no validation: a part that is not well formed reads as
// whatever its tags say, and the preview draws that rather than refusing the whole sheet.
use std::borrow::Cow;

pub enum Tok<'a> {
    // attrs is the raw run after the name, read with attr() only when the caller wants one.
    Open { name: &'a str, attrs: &'a str, empty: bool },
    Close(&'a str),
    // Character data, still entity-encoded unless it came from a CDATA section.
    Text(&'a str),
    Cdata(&'a str),
}

pub struct Tokens<'a> {
    s: &'a str,
    pos: usize,
}

pub fn tokens(s: &str) -> Tokens<'_> {
    Tokens { s, pos: 0 }
}

// The name after its prefix: "table:table-row" is "table-row", "row" stays "row".
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

impl<'a> Iterator for Tokens<'a> {
    type Item = Tok<'a>;

    fn next(&mut self) -> Option<Tok<'a>> {
        loop {
            let rest = &self.s[self.pos..];
            if rest.is_empty() {
                return None;
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.pos += end;
                return Some(Tok::Text(&rest[..end]));
            }
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = body.find("]]>").unwrap_or(body.len());
                self.pos += 9 + end + 3.min(body.len() - end);
                return Some(Tok::Cdata(&body[..end]));
            }
            // A comment, a processing instruction or a declaration carries nothing a cell shows.
            let skip = if rest.starts_with("<!--") {
                Some("-->")
            } else if rest.starts_with("<?") {
                Some("?>")
            } else if rest.starts_with("<!") {
                Some(">")
            } else {
                None
            };
            if let Some(close) = skip {
                self.pos += rest.find(close).map(|i| i + close.len()).unwrap_or(rest.len());
                continue;
            }
            let end = tag_end(rest);
            let inner = &rest[1..end];
            self.pos += (end + 1).min(rest.len());
            if let Some(name) = inner.strip_prefix('/') {
                return Some(Tok::Close(local(name.trim())));
            }
            let empty = inner.ends_with('/');
            let inner = if empty { &inner[..inner.len() - 1] } else { inner };
            let split = inner.find(|c: char| c.is_ascii_whitespace()).unwrap_or(inner.len());
            return Some(Tok::Open { name: local(&inner[..split]), attrs: &inner[split..], empty });
        }
    }
}

// Where a tag's closing > is, skipping any > inside a quoted attribute value.
fn tag_end(tag: &str) -> usize {
    let mut quote: Option<u8> = None;
    for (i, b) in tag.bytes().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return i,
            None => {}
        }
    }
    tag.len()
}

// One attribute by its local name, decoded: "table:number-columns-repeated" answers to
// "number-columns-repeated", and r answers to r.
pub fn attr(attrs: &str, want: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start();
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let close = after[1..].find(q)? + 1;
        if local(key) == want {
            return Some(decode(&after[1..close]).into_owned());
        }
        rest = &after[close + 1..];
    }
}

// The five named entities and numeric references; anything else is left as written.
pub fn decode(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let semi = tail.find(';').filter(|&i| i <= 12);
        let named = semi.and_then(|i| match &tail[1..i] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        });
        match (named, semi) {
            (Some(c), Some(i)) => {
                out.push(c);
                rest = &tail[i + 1..];
            }
            _ => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

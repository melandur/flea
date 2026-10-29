// ui/js/SyntaxMarkdown.js: markdown is read a line at a time, because a heading, a rule, a fence and a
// list or quote marker are known only from where the line starts. A fenced block draws in the
// language its info string names through syntax.rs's own highlighter, so ```python reads as a .py.
// The JavaScript's regular expressions are spelled out by hand here, each beside its pattern.
use crate::syntax::{self, put, Painted, Role, State};
use std::ops::Range;

type Spans = Vec<(Range<usize>, Role)>;

fn spaces(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ').count()
}

// /^ {0,3}(`{3,}|~{3,}) */: the fence mark, and the rest of the line after it.
fn fence_mark(line: &str) -> Option<(&str, &str)> {
    let lead = spaces(line);
    if lead > 3 {
        return None;
    }
    let body = &line[lead..];
    let c = body.bytes().next().filter(|c| *c == b'`' || *c == b'~')?;
    let n = body.bytes().take_while(|b| *b == c).count();
    (n >= 3).then(|| (&body[..n], &body[n..]))
}

// FENCE's info string: ` *([\w+#.-]*)` after the mark.
fn fence(line: &str) -> Option<(String, String)> {
    let (mark, rest) = fence_mark(line)?;
    let rest = rest.trim_start_matches(' ');
    let info: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || "_+#.-".contains(*c)).collect();
    Some((mark.to_string(), info))
}

// /^ {0,3}#{1,6}(?=[ \t]|$)/
fn heading(line: &str) -> bool {
    let lead = spaces(line);
    let n = line[lead..].bytes().take_while(|b| *b == b'#').count();
    lead <= 3 && (1..=6).contains(&n) && matches!(line.as_bytes().get(lead + n), None | Some(b' ' | b'\t'))
}

// /^ {0,3}([-*_])( *\1){2,} *$/: three or more of one of the three, spaces between, nothing else.
fn rule(line: &str) -> bool {
    let lead = spaces(line);
    let body = &line[lead..];
    let Some(c) = body.chars().next().filter(|c| "-*_".contains(*c)) else { return false };
    lead <= 3 && body.chars().all(|x| x == c || x == ' ') && body.chars().filter(|x| *x == c).count() >= 3
}

// /^ *(?:>|[-*+](?= )|\d{1,9}[.)](?= )) ?/: the marker's length, leading spaces included.
fn marker(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    let lead = spaces(rest);
    let mut k = match b.get(lead) {
        Some(b'>') => lead + 1,
        Some(b'-' | b'*' | b'+') if b.get(lead + 1) == Some(&b' ') => lead + 1,
        Some(d) if d.is_ascii_digit() => {
            let n = b[lead..].iter().take_while(|c| c.is_ascii_digit()).count();
            let e = lead + n;
            if n > 9 || !matches!(b.get(e), Some(b'.' | b')')) || b.get(e + 1) != Some(&b' ') {
                return None;
            }
            e + 1
        }
        _ => return None,
    };
    if b.get(k) == Some(&b' ') {
        k += 1;
    }
    Some(k)
}

// The ALIASES table: info strings that name a language by something other than its extension.
fn fence_spec(info: &str) -> Option<&'static syntax::Spec> {
    let name = info.to_lowercase();
    if name.is_empty() {
        return None;
    }
    let alias = match name.as_str() {
        "python" | "python3" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "rust" => "rs",
        "bash" | "shell" | "console" | "zsh" => "sh",
        "ruby" => "rb",
        "perl" => "pl",
        "golang" => "go",
        "c++" => "cpp",
        "csharp" => "cs",
        "kotlin" => "kt",
        "yml" => "yaml",
        "dockerfile" | "docker" | "containerfile" => "Dockerfile",
        other => other,
    };
    let path = if alias == "Dockerfile" { alias.to_string() } else { format!("x.{alias}") };
    syntax::language(&path).filter(|s| !s.fam.markdown)
}

fn word_at(t: &[u8], at: Option<usize>) -> bool {
    at.and_then(|a| t.get(a)).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

// One INLINE match at `i`: its end and the role, or the link's three parts.
fn inline_at(text: &str, i: usize) -> Option<usize> {
    let t = text.as_bytes();
    let rest = &text[i..];
    let closed_by = |open: &str, body_ok: &dyn Fn(char) -> bool, close: &str, first_ok: &dyn Fn(char) -> bool| -> Option<usize> {
        let after = rest.strip_prefix(open)?;
        let first = after.chars().next().filter(|c| first_ok(*c))?;
        let body = first.len_utf8() + after[first.len_utf8()..].chars().take_while(|c| body_ok(*c)).map(char::len_utf8).sum::<usize>();
        after[body..].starts_with(close).then(|| i + open.len() + body + close.len())
    };
    let not = |x: char| move |c: char| c != x && c != '\n';
    match t[i] {
        // `[^`\n]+`
        b'`' => closed_by("`", &not('`'), "`", &not('`')),
        // \*\*[^*\n]+?\*\*, then \*[^*\s][^*\n]*?\*
        b'*' => closed_by("**", &not('*'), "**", &not('*')).or_else(|| closed_by("*", &not('*'), "*", &|c| c != '*' && !c.is_whitespace())),
        b'_' => closed_by("__", &not('_'), "__", &not('_')).or_else(|| closed_by("_", &not('_'), "_", &|c| c != '_' && !c.is_whitespace())),
        // !?\[[^\]\n]*\]\([^)\n]*\)
        b'!' | b'[' => {
            let s = if t[i] == b'!' { i + 1 } else { i };
            if t.get(s) != Some(&b'[') {
                return None;
            }
            let close = s + 1 + text[s + 1..].find([']', '\n'])?;
            if !text[close..].starts_with("](") {
                return None;
            }
            let end = close + 2 + text[close + 2..].find([')', '\n'])?;
            (t[end] == b')').then_some(end + 1)
        }
        // <https?:\/\/[^>\s]*>
        b'<' => {
            let after = rest.strip_prefix("<https://").or_else(|| rest.strip_prefix("<http://"))?;
            let body: usize = after.chars().take_while(|c| *c != '>' && !c.is_whitespace()).map(char::len_utf8).sum();
            after[body..].starts_with('>').then(|| i + (rest.len() - after.len()) + body + 1)
        }
        _ => None,
    }
}

// The pieces of a line's prose: code spans, strong and emphasis, links and autolinks.
fn inline(text: &str, base: usize, spans: &mut Spans) {
    let t = text.as_bytes();
    let mut i = 0;
    while i < t.len() {
        let Some(end) = inline_at(text, i) else {
            i += 1;
            continue;
        };
        let c = t[i];
        // snake_case is a name, not emphasis: _ opens and closes only at a word's edge.
        if c == b'_' && (word_at(t, i.checked_sub(1)) || word_at(t, Some(end))) {
            i += 1;
            continue;
        }
        let tok = &text[i..end];
        let at = |r: Range<usize>| base + r.start..base + r.end;
        match c {
            b'`' => put(spans, Role::String, at(i..end)),
            _ if tok.starts_with("**") || tok.starts_with("__") => put(spans, Role::Strong, at(i..end)),
            b'*' | b'_' => put(spans, Role::Param, at(i..end)),
            b'<' => put(spans, Role::Number, at(i..end)),
            _ => {
                let close = i + tok.find("](").unwrap_or(0);
                let open = if c == b'!' { 2 } else { 1 };
                put(spans, Role::Builtin, at(i + open..close));
                put(spans, Role::Number, at(close + 2..end - 1));
            }
        }
        i = end;
    }
}

// One line outside a fence; the fence it opens, if any.
fn prose(line: &str, base: usize, spans: &mut Spans) -> Option<State> {
    let whole = base..base + line.len();
    if let Some((mark, info)) = fence(line) {
        put(spans, Role::Comment, whole);
        return Some(State::Fence { mark, spec: fence_spec(&info), inner: None });
    }
    if heading(line) {
        put(spans, Role::Heading, whole);
        return None;
    }
    if rule(line) {
        put(spans, Role::Comment, whole);
        return None;
    }
    let mut at = 0;
    while at < line.len() {
        let Some(n) = marker(&line[at..]) else { break };
        let lead = spaces(&line[at..]);
        put(spans, Role::Operator, base + at + lead..base + at + n);
        at += n;
    }
    inline(&line[at..], base + at, spans);
    None
}

// A fence closes on a line of its own character at least as long as the one that opened it.
fn closes(line: &str, mark: &str) -> bool {
    match fence_mark(line) {
        Some((m, rest)) => rest.bytes().all(|b| b == b' ') && m.as_bytes()[0] == mark.as_bytes()[0] && m.len() >= mark.len(),
        None => false,
    }
}

// SyntaxMarkdown.highlight: the same contract as syntax::highlight, a line at a time.
pub fn highlight(text: &str, state: Option<&State>) -> Painted {
    let mut state = state.cloned();
    let mut spans = vec![];
    let mut base = 0;
    for line in text.split('\n') {
        let whole = base..base + line.len();
        state = match state {
            Some(State::Fence { mark, .. }) if closes(line, &mark) => {
                put(&mut spans, Role::Comment, whole);
                None
            }
            Some(State::Fence { mark, spec: Some(sp), inner }) => {
                let r = syntax::highlight(line, sp, inner.map(State::Open).as_ref());
                spans.extend(r.spans.into_iter().map(|(rg, role)| (rg.start + base..rg.end + base, role)));
                let inner = match r.state {
                    Some(State::Open(o)) => Some(o),
                    _ => None,
                };
                Some(State::Fence { mark, spec: Some(sp), inner })
            }
            Some(State::Fence { mark, spec: None, inner }) => {
                put(&mut spans, Role::String, whole);
                Some(State::Fence { mark, spec: None, inner })
            }
            _ => prose(line, base, &mut spans),
        };
        base += line.len() + 1;
    }
    Painted { spans, state }
}

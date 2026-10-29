// ui/js/Syntax.js: syntax colour for the text previews in PyCharm's Monokai. A lexer, not a parser:
// each role is read off the tokens around it. Where the JavaScript built StyledText, this answers
// byte ranges and roles, so nothing from the file is ever markup and there is nothing to escape;
// syntaxview.rs turns the roles into colours. A run may hold several lines; what a run leaves open,
// a comment or a string, is the state the next run starts in.
use crate::syntaxlang::{self as lang, Family};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::OnceLock;

// Larger files preview as plain text: the colour of a run depends on every run above it.
pub const MAX_BYTES: u64 = 2_097_152;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Keyword,
    Builtin,
    String,
    Number,
    Comment,
    Operator,
    Func,
    Param,
    Heading,
    Strong,
}

// A comment or string still open: what closes it, whether a backslash escapes, whether it spans lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Open {
    pub close: &'static str,
    pub esc: bool,
    pub multi: bool,
    pub comment: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Open(Open),
    // syntaxmd.rs: a fence still open, the language its info string named, and that language's state.
    Fence { mark: String, spec: Option<&'static Spec>, inner: Option<Open> },
}

#[derive(Debug, Default, PartialEq)]
pub struct Painted {
    pub spans: Vec<(Range<usize>, Role)>,
    pub state: Option<State>,
}

#[derive(Debug, PartialEq)]
pub struct Spec {
    pub key: &'static str,
    pub fam: Family,
    blocks: Vec<&'static str>,
    lines: Vec<&'static str>,
    strings: Vec<&'static str>,
    closes: HashMap<&'static str, Open>,
    words: HashSet<String>,
    constants: HashSet<&'static str>,
    builtins: HashSet<&'static str>,
    self_words: HashSet<&'static str>,
    definers: HashSet<&'static str>,
}

fn longest_first(mut v: Vec<&'static str>) -> Vec<&'static str> {
    v.sort_by_key(|s| std::cmp::Reverse(s.len()));
    v
}

fn build(key: &'static str) -> Option<Spec> {
    let fam = lang::family(key)?;
    let mut closes = HashMap::new();
    for (o, c) in fam.block {
        closes.insert(*o, Open { close: c, esc: false, multi: true, comment: true });
    }
    for (o, c, multi) in fam.strings {
        closes.insert(*o, Open { close: c, esc: true, multi: *multi, comment: false });
    }
    let all = format!("{} {}", lang::words(fam.words), lang::CONSTANTS);
    Some(Spec {
        key,
        blocks: longest_first(fam.block.iter().map(|b| b.0).collect()),
        lines: longest_first(fam.line.to_vec()),
        strings: longest_first(fam.strings.iter().map(|s| s.0).collect()),
        closes,
        words: all.split_whitespace().map(|w| if fam.ci { w.to_lowercase() } else { w.to_string() }).collect(),
        constants: lang::CONSTANTS.split_whitespace().collect(),
        builtins: lang::builtins(fam.words).split_whitespace().collect(),
        self_words: fam.self_words.split_whitespace().collect(),
        definers: lang::DEFINERS.split_whitespace().collect(),
        fam,
    })
}

fn specs() -> &'static HashMap<&'static str, Spec> {
    static SPECS: OnceLock<HashMap<&'static str, Spec>> = OnceLock::new();
    SPECS.get_or_init(|| lang::EXTENSIONS.iter().filter_map(|(k, _)| build(k).map(|s| (*k, s))).collect())
}

pub fn spec(key: &str) -> Option<&'static Spec> {
    specs().get(key)
}

// Syntax.language: a well-known name, then the extension, then a well-known name with a suffix of
// its own (Dockerfile.dev); a dotfile with no extension of its own stays plain.
pub fn language(path: &str) -> Option<&'static Spec> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(k) = lang::named(name) {
        return spec(k);
    }
    let dot = name.rfind('.').filter(|d| *d > 0)?;
    let ext = name[dot + 1..].to_lowercase();
    let by_ext = lang::EXTENSIONS.iter().find(|(_, exts)| exts.split(' ').any(|e| e == ext)).map(|(k, _)| *k);
    by_ext.or_else(|| lang::named(&name[..name.find('.').unwrap_or(name.len())])).and_then(spec)
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn run_while(t: &[u8], from: usize, ok: impl Fn(u8) -> bool) -> usize {
    let mut k = from;
    while k < t.len() && ok(t[k]) {
        k += 1;
    }
    k
}

// The sign before a token, past any space or line break: a parameter follows the ( or , of a list.
fn sign_before(t: &[u8], at: usize) -> u8 {
    t[..at].iter().rev().copied().find(|b| *b != b' ' && *b != b'\n').unwrap_or(0)
}

// Where the parameter list just after a defined name closes, or None with no list; open to the end.
fn params_end(t: &[u8], from: usize) -> Option<usize> {
    let mut k = run_while(t, from, |b| b == b' ');
    if t.get(k) != Some(&b'(') {
        return None;
    }
    let mut depth = 0;
    while k < t.len() {
        match t[k] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
        k += 1;
    }
    Some(t.len())
}

fn escaped(t: &[u8], at: usize) -> bool {
    t[..at].iter().rev().take_while(|b| **b == b'\\').count() % 2 == 1
}

// Where an open comment or string ends, and whether it is still open at the run's end.
fn close_at(text: &str, from: usize, open: &Open) -> (usize, bool) {
    let nl = if open.multi { None } else { text[from..].find('\n').map(|n| n + from) };
    let mut j = from;
    loop {
        let k = text[j..].find(open.close).map(|k| k + j);
        if let Some(nl) = nl.filter(|nl| k.is_none_or(|k| *nl < k)) {
            return (nl, false);
        }
        let Some(k) = k else { return (text.len(), open.multi) };
        if open.esc && escaped(text.as_bytes(), k) {
            j = k + 1;
            continue;
        }
        return (k + open.close.len(), false);
    }
}

// Neighbouring text of one role is one span.
pub(crate) fn put(spans: &mut Vec<(Range<usize>, Role)>, role: Role, r: Range<usize>) {
    if r.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some((last, l)) if *l == role && last.end == r.start => last.end = r.end,
        _ => spans.push((r, role)),
    }
}

// A string's escapes draw as constants do, Monokai's \n in purple; a backslash before a line break is none.
fn put_string(spans: &mut Vec<(Range<usize>, Role)>, text: &str, r: Range<usize>, esc: bool) {
    let mut at = r.start;
    if esc {
        let mut k = r.start;
        while k < r.end {
            let rest = &text[k..r.end];
            match rest.strip_prefix('\\').and_then(|after| after.chars().next()).filter(|c| *c != '\n' && *c != '\r') {
                Some(c) => {
                    put(spans, Role::String, at..k);
                    put(spans, Role::Number, k..k + 1 + c.len_utf8());
                    k += 1 + c.len_utf8();
                    at = k;
                }
                None => k += rest.chars().next().map_or(1, char::len_utf8),
            }
        }
    }
    put(spans, Role::String, at..r.end);
}

// One token at `i`, tried in Syntax.spec's order: block, line, string, tag, decorator, number,
// word, sign. The answer is its kind and its end.
#[derive(PartialEq)]
enum Kind {
    Block,
    Line,
    Str,
    Tag,
    Decorator,
    Number,
    Word,
    Op,
}

fn token(sp: &Spec, t: &[u8], i: usize) -> Option<(Kind, usize)> {
    let rest = &t[i..];
    let opener = |list: &[&'static str]| list.iter().find(|o| rest.starts_with(o.as_bytes())).map(|o| i + o.len());
    if let Some(e) = opener(&sp.blocks) {
        return Some((Kind::Block, e));
    }
    if let Some(e) = opener(&sp.lines) {
        return Some((Kind::Line, e));
    }
    if let Some(e) = opener(&sp.strings) {
        return Some((Kind::Str, e));
    }
    let b = t[i];
    if sp.fam.tags && b == b'<' {
        let s = i + 1 + usize::from(t.get(i + 1) == Some(&b'/'));
        if t.get(s).is_some_and(u8::is_ascii_alphabetic) {
            return Some((Kind::Tag, run_while(t, s + 1, |c| is_word(c) || b":.-".contains(&c))));
        }
    }
    if sp.fam.decorators && b == b'@' && t.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
        return Some((Kind::Decorator, run_while(t, i + 2, |c| is_word(c) || c == b'.')));
    }
    if b.is_ascii_digit() && (i == 0 || !is_word(t[i - 1])) {
        return Some((Kind::Number, run_while(t, i + 1, |c| is_word(c) || c == b'.')));
    }
    if sp.fam.id_start.as_bytes().contains(&b) || b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
        return Some((Kind::Word, run_while(t, i + 1, |c| is_word(c) || c == b'$')));
    }
    if sp.fam.ops.as_bytes().contains(&b) {
        return Some((Kind::Op, i + 1));
    }
    None
}

// Syntax.highlight: one run of the file, from the state the run above left.
pub fn highlight(text: &str, sp: &'static Spec, state: Option<&State>) -> Painted {
    if sp.fam.markdown {
        return crate::syntaxmd::highlight(text, state);
    }
    let t = text.as_bytes();
    let mut spans = vec![];
    let mut open_state = None;
    let mut i = 0;
    let open_until = |spans: &mut Vec<(Range<usize>, Role)>, start: usize, from: usize, open: Open, st: &mut Option<State>| {
        let (at, still) = close_at(text, from, &open);
        if open.comment {
            put(spans, Role::Comment, start..at);
        } else {
            put_string(spans, text, start..at, open.esc);
        }
        *st = still.then_some(State::Open(open));
        at
    };
    if let Some(State::Open(o)) = state {
        i = open_until(&mut spans, 0, 0, *o, &mut open_state);
    }
    // Just past a def, fn or function keyword, and where the parameter list after its name closes.
    let (mut def_end, mut params): (Option<usize>, Option<usize>) = (None, None);
    while i < t.len() {
        let Some((kind, end)) = token(sp, t, i) else {
            i += 1;
            continue;
        };
        let start = i;
        i = end;
        let tok = &text[start..end];
        match kind {
            Kind::Line => {
                // A shell's # opens a comment only at the start of a word.
                if sp.fam.hash_word && start > 0 && !matches!(t[start - 1], b' ' | b'\t' | b'\n' | b'\r' | b';') {
                    continue;
                }
                i = text[start..].find('\n').map_or(t.len(), |n| n + start);
                put(&mut spans, Role::Comment, start..i);
            }
            Kind::Block | Kind::Str => {
                // A Rust lifetime is no char literal: 'a str, where 'x' and '\n' are one.
                if kind == Kind::Str && sp.fam.lifetimes && tok == "'" && !is_char_literal(&text[start..]) {
                    continue;
                }
                let mut o = sp.closes[tok];
                o.comment = kind == Kind::Block;
                i = open_until(&mut spans, start, end, o, &mut open_state);
            }
            Kind::Tag => {
                let lead = if t.get(start + 1) == Some(&b'/') { 2 } else { 1 };
                put(&mut spans, Role::Operator, start + lead..end);
            }
            Kind::Decorator => put(&mut spans, Role::Func, start..end),
            Kind::Number => put(&mut spans, Role::Number, start..end),
            Kind::Op => put(&mut spans, Role::Operator, start..end),
            Kind::Word => {
                let w = if sp.fam.ci { tok.to_lowercase() } else { tok.to_string() };
                let sign = sign_before(t, start);
                let (next, after) = (t.get(end).copied().unwrap_or(0), t.get(end + 1).copied().unwrap_or(0));
                let role = if sp.constants.contains(tok) {
                    Some(Role::Number)
                } else if sp.self_words.contains(tok) {
                    Some(Role::Param)
                } else if sp.words.contains(&w) {
                    Some(Role::Keyword)
                } else if def_end.is_some_and(|d| d < start && t[d..start].iter().all(|b| *b == b' ')) {
                    Some(Role::Func)
                } else if params.is_some_and(|p| start < p) && b"(,*".contains(&sign) && sign != 0 {
                    Some(Role::Param)
                } else if sp.fam.kwargs && b"(,".contains(&sign) && sign != 0 && next == b'=' && after != b'=' {
                    Some(Role::Param)
                } else if sp.fam.attrs && next == b'=' {
                    Some(Role::Func)
                } else if sp.builtins.contains(tok) || (sp.fam.macros && next == b'!' && after != b'=') {
                    Some(Role::Builtin)
                } else {
                    None
                };
                if let Some(r) = role {
                    put(&mut spans, r, start..end);
                }
                if role == Some(Role::Func) {
                    params = params_end(t, end);
                }
                def_end = (role == Some(Role::Keyword) && sp.definers.contains(w.as_str())).then_some(end);
            }
        }
    }
    Painted { spans, state: open_state }
}

// /^'(\\.|[^\\'])'/: an escape is any character but a line break, a plain character any but those two.
fn is_char_literal(rest: &str) -> bool {
    let mut c = rest.chars().skip(1);
    match c.next() {
        Some('\\') => c.next().is_some_and(|x| x != '\n' && x != '\r') && c.next() == Some('\''),
        Some('\'') | None => false,
        Some(_) => c.next() == Some('\''),
    }
}

// SyntaxStyle.expandTabs: a tab to the next stop of four, for the column's lines.
pub fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut col = 0;
    for ch in text.chars() {
        match ch {
            '\t' => {
                let pad = 4 - col % 4;
                out.extend(std::iter::repeat_n(' ', pad));
                col += pad;
            }
            '\n' => {
                out.push(ch);
                col = 0;
            }
            _ => {
                out.push(ch);
                col += 1;
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;

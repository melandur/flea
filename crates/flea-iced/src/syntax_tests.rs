// tests/js/syntax.js case for case. A span prints as [K:text] with the JS suite's own letters; plain
// text prints as it is. There is no escaping and no hard space to test, because nothing here is markup.
use super::*;

fn letter(r: Role) -> char {
    match r {
        Role::Keyword => 'K',
        Role::Builtin => 'B',
        Role::String => 'S',
        Role::Number => 'N',
        Role::Comment => 'C',
        Role::Operator => 'O',
        Role::Func => 'F',
        Role::Param => 'P',
        Role::Heading => 'H',
        Role::Strong => 'G',
    }
}

fn show(text: &str, p: &Painted) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (r, role) in &p.spans {
        out.push_str(&text[at..r.start]);
        out.push_str(&format!("[{}:{}]", letter(*role), &text[r.clone()]));
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

fn paint(path: &str, text: &str, state: Option<&State>) -> Painted {
    highlight(text, language(path).expect("a family"), state)
}

fn html(path: &str, text: &str) -> String {
    show(text, &paint(path, text, None))
}

#[test]
fn a_name_finds_its_family() {
    assert!(language("/d/notes.txt").is_none());
    assert_eq!(language("/d/MAIN.RS").unwrap().key, "rust");
    assert_eq!(language("/d/PKGBUILD").unwrap().key, "sh");
    assert!(language("/d/.gitignore").is_none());
    assert_eq!(language("/d/Dockerfile").unwrap().key, "docker");
    assert_eq!(language("/d/Dockerfile.dev").unwrap().key, "docker");
    assert_eq!(language("/d/app.dockerfile").unwrap().key, "docker");
    assert_eq!(language("/d/README.md").unwrap().key, "markdown");
}

#[test]
fn every_family_has_its_four_roles() {
    assert_eq!(html("a.py", "def f(x): return 42"), "[K:def] [F:f]([P:x]): [K:return] [N:42]");
    assert_eq!(html("a.py", "x = None"), "x [O:=] [N:None]");
    assert_eq!(html("a.rs", "let a = 1; // one\nlet"), "[K:let] a [O:=] [N:1]; [C:// one]\n[K:let]");
    assert_eq!(html("a.js", r#"x = "a\"b" + y"#), r#"x [O:=] [S:"a][N:\"][S:b"] [O:+] y"#);
    assert_eq!(html("a.js", r#""if""#), r#"[S:"if"]"#);
    assert_eq!(html("a.c", "x1"), "x1");
    assert_eq!(html("a.c", "#include"), "[K:#include]");
    assert_eq!(html("q.sql", "SELECT a"), "[K:SELECT] a");
}

#[test]
fn monokais_roles_beyond_the_four() {
    assert_eq!(html("a.py", "@decorator(param=1)"), "[F:@decorator]([P:param][O:=][N:1])");
    assert_eq!(html("a.py", "a, b = 1"), "a, b [O:=] [N:1]");
    assert_eq!(html("a.py", "def go(self, *rest):"), "[K:def] [F:go]([P:self], [O:*][P:rest]):");
    assert_eq!(html("a.py", "len(s) + f(s)"), "[B:len](s) [O:+] f(s)");
    assert_eq!(html("a.py", r"'a\nb'"), r"[S:'a][N:\n][S:b']");
    assert_eq!(html("a.rs", "println!(x)"), "[B:println][O:!](x)");
    assert_eq!(html("a.rs", "fn f(x: u8)"), "[K:fn] [F:f]([P:x]: [B:u8])");
    assert_eq!(html("a.sh", "ls -la"), "ls -la");
    assert_eq!(html("a.html", r#"<a href="x">"#), r#"<[O:a] [F:href]=[S:"x"]>"#);
    assert_eq!(html("a.c", "x=/*c*/1"), "x[O:=][C:/*c*/][N:1]");
    assert_eq!(html("Dockerfile", "RUN pip install from # x"), "[K:RUN] pip install from [C:# x]");
    assert_eq!(html("Dockerfile", "FROM a AS b"), "[K:FROM] a [K:AS] b");
    assert_eq!(html("a.html", "<p>hi</p>"), "<[O:p]>hi</[O:p]>");
    assert_eq!(html("a.py", r#"x = '<img src="http://e/x">' & y"#), r#"x [O:=] [S:'<img src="http://e/x">'] [O:&] y"#);
}

#[test]
fn markdown_is_read_a_line_at_a_time() {
    assert_eq!(html("a.md", "## Two words"), "[H:## Two words]");
    assert_eq!(html("a.md", "issue #14"), "issue #14");
    assert_eq!(html("a.md", "- `x` **b** *i*"), "[O:- ][S:`x`] [G:**b**] [P:*i*]");
    assert_eq!(html("a.md", "a snake_case_name"), "a snake_case_name");
    assert_eq!(html("a.md", "[see](http://e/x)"), "[[B:see]]([N:http://e/x])");
    assert_eq!(html("a.md", "> said"), "[O:> ]said");
    assert_eq!(html("a.md", "<img src=x> & y"), "<img src=x> & y");
    assert_eq!(html("a.md", "```python\ndef f(): pass\n```\nafter"), "[C:```python]\n[K:def] [F:f](): [K:pass]\n[C:```]\nafter");
    assert_eq!(html("a.md", "```\n# not a heading\n```"), "[C:```]\n[S:# not a heading]\n[C:```]");
    let fence = paint("a.md", "text\n```c\n/* open", None);
    let next = "still */ x\n```\n# h";
    assert_eq!(show(next, &paint("a.md", next, fence.state.as_ref())), "[C:still */] x\n[C:```]\n[H:# h]");
    assert!(matches!(fence.state, Some(State::Fence { inner: Some(Open { comment: true, .. }), .. })), "the run above keeps its own state");
    assert_eq!(html("a.md", "---"), "[C:---]");
    assert_eq!(html("a.md", "1. one"), "[O:1. ]one");
}

#[test]
fn what_a_run_leaves_open_is_where_the_next_starts() {
    let open = paint("a.c", "x /* one", None);
    assert!(open.state.is_some());
    assert_eq!(show("two */ y", &paint("a.c", "two */ y", open.state.as_ref())), "[C:two */] y");
    let doc = paint("a.py", r#"x = """doc"#, None);
    let more = r#"more""" + y"#;
    assert_eq!(show(more, &paint("a.py", more, doc.state.as_ref())), r#"[S:more"""] [O:+] y"#);
    assert!(paint("a.py", "x = 'abc", None).state.is_none(), "an ordinary string never carries past its line");
    let comment = State::Open(Open { close: "*/", esc: false, multi: true, comment: true });
    assert_eq!(show(" * b */", &paint("a.c", " * b */", Some(&comment))), "[C: * b */]");
    // The column's first lines carry an open comment down from line to line.
    let first = paint("a.c", "/* a", None);
    assert_eq!(show("b */ c", &paint("a.c", "b */ c", first.state.as_ref())), "[C:b */] c");
}

#[test]
fn the_corners_each_family_has() {
    assert_eq!(html("a.rs", "&'a str"), "[O:&]'a [B:str]");
    assert_eq!(html("a.rs", "'x'"), "[S:'x']");
    assert_eq!(html("a.sh", "echo $# x"), "[B:echo] $# x");
    assert_eq!(html("a.sh", "a # c"), "a [C:# c]");
    assert_eq!(html("a.lua", "--[[ x ]] y"), "[C:--[[ x ]]] y");
    assert_eq!(expand_tabs("\tx\na\ty"), "    x\na   y");
    // A file is arbitrary text: a multibyte name and an escape before one stay whole.
    assert_eq!(html("a.py", "é = 'ü\\é'"), "é [O:=] [S:'ü][N:\\é][S:']");
}

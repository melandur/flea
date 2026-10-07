.pragma library

// The language tables ui/js/Syntax.js colours by: which file names belong to which family, and
// what each family calls a comment, a string and a keyword. Data only; Syntax.js builds from it.

var CONSTANTS = "true false null undefined nullptr NULL nil None True False NaN Infinity"
var CLIKE = "if else for while do switch case default break continue return goto new delete this "
    + "class struct enum union interface extends implements public private protected static final "
    + "const void try catch finally throw throws import package using namespace typedef typename "
    + "template virtual override abstract sizeof instanceof typeof in is as var val let fun func "
    + "function def where fn self super async await yield operator extern inline volatile"
var WORDS = {
    js: "var let const function return if else for while do switch case default break continue new "
        + "delete typeof instanceof in of class extends super this import export from as async await "
        + "yield try catch finally throw interface type enum implements public private protected "
        + "readonly static get set declare namespace void property signal required alias",
    rust: "as async await break const continue crate dyn else enum extern fn for if impl in let loop "
        + "match mod move mut pub ref return self Self static struct super trait type unsafe use "
        + "where while",
    c: "auto break case char const continue default do double else enum extern float for goto if "
        + "inline int long register return short signed sizeof static struct switch typedef union "
        + "unsigned void volatile while class namespace template typename public private protected "
        + "virtual override new delete using this operator bool #include #define #ifdef #ifndef "
        + "#endif #if #elif #else #pragma #undef",
    go: "break case chan const continue default defer else fallthrough for func go goto if import "
        + "interface map package range return select struct switch type var",
    py: "and as assert async await break class continue def del elif else except finally for from "
        + "global if import in is lambda nonlocal not or pass raise return try while with yield "
        + "match case self",
    rb: "alias and begin break case class def defined do else elsif end ensure for if in module next "
        + "not or redo rescue retry return self super then undef unless until when while yield",
    sh: "if then else elif fi case esac for while until do done in function select time return "
        + "local export readonly declare unset shift exit source alias set",
    lua: "and break do else elseif end for function goto if in local not or repeat return then until while",
    pl: "my our local sub if elsif else unless while until for foreach last next redo return use "
        + "require package",
    sql: "select from where insert into values update set delete create table drop alter index join "
        + "left right inner outer on as and or not null is in group by order having limit offset "
        + "distinct union all primary key foreign references default begin commit rollback view",
    clike: CLIKE,
    none: ""
}

var DQ = ['"', '"', false], SQ = ["'", "'", false], BQ = ["`", "`", true]
var C_COMMENTS = { line: ["//"], block: [["/*", "*/"]] }

// Each family: line comment openers, block comment pairs, string delimiters as [open, close,
// spans lines], its keyword list, and three flags: SQL's case-insensitive words, the markup's tag
// names, and shell's rule that # opens a comment only at the start of a word.
function family(comments, strings, words, flags) {
    var f = flags || {}
    return { line: comments.line || [], block: comments.block || [], strings: strings,
             words: words, ci: f.ci === true, tags: f.tags === true, hashWord: f.hashWord === true,
             lifetimes: f.lifetimes === true, idStart: f.idStart || "" }
}

var FAMILIES = {
    js: family(C_COMMENTS, [DQ, SQ, BQ], "js"),
    rust: family(C_COMMENTS, [DQ, SQ], "rust", { lifetimes: true }),
    c: family(C_COMMENTS, [DQ, SQ], "c", { idStart: "#" }),
    go: family(C_COMMENTS, [DQ, SQ, BQ], "go"),
    clike: family(C_COMMENTS, [DQ, SQ], "clike"),
    css: family(C_COMMENTS, [DQ, SQ], "none"),
    py: family({ line: ["#"] }, [['"""', '"""', true], ["'''", "'''", true], DQ, SQ], "py"),
    rb: family({ line: ["#"] }, [DQ, SQ], "rb"),
    sh: family({ line: ["#"] }, [DQ, SQ], "sh", { hashWord: true }),
    pl: family({ line: ["#"] }, [DQ, SQ], "pl"),
    lua: family({ line: ["--"], block: [["--[[", "]]"]] }, [DQ, SQ, ["[[", "]]", true]], "lua"),
    sql: family({ line: ["--"], block: [["/*", "*/"]] }, [DQ, SQ], "sql", { ci: true }),
    conf: family({ line: ["#", ";"] }, [DQ, SQ], "none"),
    json: family({}, [DQ], "none"),
    markup: family({ block: [["<!--", "-->"]] }, [DQ], "none", { tags: true })
}

var EXTENSIONS = {
    js: "js mjs cjs jsx ts tsx qml", rust: "rs", c: "c h cpp hpp cc cxx hh", go: "go",
    clike: "java kt kts swift php cs scala dart", css: "css scss less", py: "py pyi",
    rb: "rb", sh: "sh bash zsh fish", pl: "pl pm", lua: "lua", sql: "sql",
    conf: "toml yaml yml ini conf cfg", json: "json", markup: "html htm xml svg xhtml"
}
var NAMES = { "PKGBUILD": "sh", ".bashrc": "sh", ".zshrc": "sh", ".profile": "sh", ".bash_profile": "sh",
              "Makefile": "conf", "Dockerfile": "conf", "CMakeLists.txt": "conf" }

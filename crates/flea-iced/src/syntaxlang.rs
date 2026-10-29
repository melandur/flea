// ui/js/SyntaxLanguages.js: which names belong to which family, and what each family calls a
// comment, a string, a keyword and a builtin. Data only; syntax.rs builds from it.

pub const CONSTANTS: &str = "true false null undefined nullptr NULL nil None True False NaN Infinity";
const CLIKE: &str = "if else for while do switch case default break continue return goto new delete this \
class struct enum union interface extends implements public private protected static final \
const void try catch finally throw throws import package using namespace typedef typename \
template virtual override abstract sizeof instanceof typeof in is as var val let fun func \
function def where fn self super async await yield operator extern inline volatile";

pub fn words(key: &str) -> &'static str {
    match key {
        "js" => "var let const function return if else for while do switch case default break continue new \
delete typeof instanceof in of class extends super this import export from as async await \
yield try catch finally throw interface type enum implements public private protected \
readonly static get set declare namespace void property signal required alias",
        "rust" => "as async await break const continue crate dyn else enum extern fn for if impl in let loop \
match mod move mut pub ref return self Self static struct super trait type unsafe use \
where while",
        "c" => "auto break case char const continue default do double else enum extern float for goto if \
inline int long register return short signed sizeof static struct switch typedef union \
unsigned void volatile while class namespace template typename public private protected \
virtual override new delete using this operator bool #include #define #ifdef #ifndef \
#endif #if #elif #else #pragma #undef",
        "go" => "break case chan const continue default defer else fallthrough for func go goto if import \
interface map package range return select struct switch type var",
        "py" => "and as assert async await break class continue def del elif else except finally for from \
global if import in is lambda nonlocal not or pass raise return try while with yield \
match case self",
        "rb" => "alias and begin break case class def defined do else elsif end ensure for if in module next \
not or redo rescue retry return self super then undef unless until when while yield",
        "sh" => "if then else elif fi case esac for while until do done in function select time return \
local export readonly declare unset shift exit source alias set",
        "lua" => "and break do else elseif end for function goto if in local not or repeat return then until while",
        "pl" => "my our local sub if elsif else unless while until for foreach last next redo return use \
require package",
        "sql" => "select from where insert into values update set delete create table drop alter index join \
left right inner outer on as and or not null is in group by order having limit offset \
distinct union all primary key foreign references default begin commit rollback view",
        "docker" => "FROM AS RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG \
ONBUILD STOPSIGNAL HEALTHCHECK SHELL",
        "clike" => CLIKE,
        _ => "",
    }
}

// Names a language ships rather than reserves, drawn in Monokai's builtin cyan, upright.
pub fn builtins(key: &str) -> &'static str {
    match key {
        "js" => "console Math JSON Object Array String Number Boolean Promise Map Set Date RegExp Error Symbol \
parseInt parseFloat require module exports window document globalThis setTimeout \
setInterval clearTimeout clearInterval Qt",
        "rust" => "Vec String Option Result Box Rc Arc RefCell Cell Some Ok Err HashMap HashSet BTreeMap \
u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64 bool char str",
        "c" => "printf fprintf sprintf snprintf malloc calloc realloc free memcpy memmove memset strlen strcmp \
strcpy size_t ssize_t uint8_t uint16_t uint32_t uint64_t int8_t int16_t int32_t int64_t \
FILE std",
        "go" => "append cap close copy delete len make new panic print println recover string int int8 int16 \
int32 int64 uint uint8 uint16 uint32 uint64 float32 float64 byte rune error bool any",
        "py" => "print len range str int float bool list dict set tuple frozenset bytes bytearray object type \
super isinstance issubclass hasattr getattr setattr delattr open enumerate zip map filter \
sorted reversed min max sum abs any all repr iter next input format id hash round divmod \
pow callable vars dir property staticmethod classmethod Exception ValueError TypeError \
KeyError IndexError RuntimeError StopIteration NotImplementedError AttributeError OSError \
ImportError",
        "rb" => "puts print p require require_relative attr_accessor attr_reader attr_writer include extend \
raise lambda proc",
        "sh" | "docker" => "echo printf cd test read eval exec trap kill",
        "lua" => "print pairs ipairs require type tostring tonumber setmetatable getmetatable error assert pcall \
table string math",
        "pl" => "print printf push pop shift keys values die",
        "clike" => "System String Integer List Map println",
        _ => "",
    }
}

// The operator signs Monokai draws in pink; a shell's and a stylesheet's dash is part of a word.
const OPS: &str = "-+*/%=<>!&|^~?";

// A string delimiter: open, close, and whether it spans lines.
pub type Delim = (&'static str, &'static str, bool);
const DQ: Delim = ("\"", "\"", false);
const SQ: Delim = ("'", "'", false);
const BQ: Delim = ("`", "`", true);

#[derive(Debug, PartialEq)]
pub struct Family {
    pub line: &'static [&'static str],
    pub block: &'static [(&'static str, &'static str)],
    pub strings: &'static [Delim],
    pub words: &'static str,
    pub ci: bool,
    pub tags: bool,
    pub hash_word: bool,
    pub lifetimes: bool,
    pub id_start: &'static str,
    pub ops: &'static str,
    pub decorators: bool,
    pub kwargs: bool,
    pub macros: bool,
    pub attrs: bool,
    pub self_words: &'static str,
    pub markdown: bool,
}

const BASE: Family = Family {
    line: &[],
    block: &[],
    strings: &[],
    words: "none",
    ci: false,
    tags: false,
    hash_word: false,
    lifetimes: false,
    id_start: "",
    ops: OPS,
    decorators: false,
    kwargs: false,
    macros: false,
    attrs: false,
    self_words: "",
    markdown: false,
};
const C_LINE: &[&str] = &["//"];
const C_BLOCK: &[(&str, &str)] = &[("/*", "*/")];

pub fn family(key: &str) -> Option<Family> {
    Some(match key {
        "js" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ, BQ], words: "js", decorators: true, self_words: "this", ..BASE },
        "rust" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ], words: "rust", lifetimes: true, macros: true, self_words: "self", ..BASE },
        "c" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ], words: "c", id_start: "#", self_words: "this", ..BASE },
        "go" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ, BQ], words: "go", ..BASE },
        "clike" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ], words: "clike", decorators: true, self_words: "this self", ..BASE },
        "css" => Family { line: C_LINE, block: C_BLOCK, strings: &[DQ, SQ], ops: "", ..BASE },
        "py" => Family {
            line: &["#"],
            strings: &[("\"\"\"", "\"\"\"", true), ("'''", "'''", true), DQ, SQ],
            words: "py",
            decorators: true,
            kwargs: true,
            self_words: "self cls",
            ..BASE
        },
        "rb" => Family { line: &["#"], strings: &[DQ, SQ], words: "rb", self_words: "self", ..BASE },
        "sh" => Family { line: &["#"], strings: &[DQ, SQ], words: "sh", hash_word: true, ops: "=|&<>!", ..BASE },
        "pl" => Family { line: &["#"], strings: &[DQ, SQ], words: "pl", ..BASE },
        "lua" => Family { line: &["--"], block: &[("--[[", "]]")], strings: &[DQ, SQ, ("[[", "]]", true)], words: "lua", self_words: "self", ..BASE },
        "sql" => Family { line: &["--"], block: C_BLOCK, strings: &[DQ, SQ], words: "sql", ci: true, ..BASE },
        "conf" => Family { line: &["#", ";"], strings: &[DQ, SQ], ops: "=", ..BASE },
        "json" => Family { strings: &[DQ], ops: "", ..BASE },
        // Instructions are upper case by convention, so a lower-case from in a RUN line stays a word.
        "docker" => Family { line: &["#"], strings: &[DQ, SQ], words: "docker", hash_word: true, ops: "=|&<>!", ..BASE },
        // Drawn a line at a time by syntaxmd.rs; the tables here go unused.
        "markdown" => Family { markdown: true, ops: "", ..BASE },
        "markup" => Family { block: &[("<!--", "-->")], strings: &[DQ], tags: true, attrs: true, ops: "", ..BASE },
        _ => return None,
    })
}

pub const EXTENSIONS: &[(&str, &str)] = &[
    ("js", "js mjs cjs jsx ts tsx qml"),
    ("rust", "rs"),
    ("c", "c h cpp hpp cc cxx hh"),
    ("go", "go"),
    ("clike", "java kt kts swift php cs scala dart"),
    ("css", "css scss less"),
    ("py", "py pyi"),
    ("rb", "rb"),
    ("sh", "sh bash zsh fish"),
    ("pl", "pl pm"),
    ("lua", "lua"),
    ("sql", "sql"),
    ("conf", "toml yaml yml ini conf cfg"),
    ("json", "json"),
    ("markup", "html htm xml svg xhtml"),
    ("docker", "dockerfile containerfile"),
    ("markdown", "md markdown mkd"),
];

// The keywords that name a function next, so that name draws in Monokai's green.
pub const DEFINERS: &str = "def fn function func sub";

pub fn named(name: &str) -> Option<&'static str> {
    Some(match name {
        "PKGBUILD" | ".bashrc" | ".zshrc" | ".profile" | ".bash_profile" => "sh",
        "Makefile" | "CMakeLists.txt" => "conf",
        "Dockerfile" | "Containerfile" => "docker",
        _ => return None,
    })
}

//! Motor genérico "familia C": C, C++, C#, Java, Go, JavaScript, TypeScript, Rust, CSS, Lua,
//! SQL. Un `Spec` describe comentarios, comillas, palabras clave y banderas del lenguaje.
use super::{find, is_ident, is_ident_start, number_end, quoted_end, Lang, Out, TokKind};

const K_BLOCK: u8 = 1; // comentario de bloque, param = profundidad
const K_MLSTR: u8 = 2; // cadena multilínea, param: 1 = `..`, 2 = """..""", 3 = verbatim C#
const K_RAW: u8 = 3; // cadena cruda de Rust, param = nº de '#'
const K_LONGC: u8 = 4; // Lua: comentario largo, param = nivel
const K_LONGS: u8 = 5; // Lua: cadena larga, param = nivel
const CSS_IN: u16 = 0x8000;

const F_PREPROC: u32 = 1; // `#directiva` al inicio de línea
const F_RUST: u32 = 1 << 1; // cadenas crudas, lifetimes, atributos, macros
const F_BACKTICK: u32 = 1 << 2; // `...` multilínea (JS/TS/Go)
const F_CSHARP: u32 = 1 << 3; // @"..." $"..."
const F_TRIPLE: u32 = 1 << 4; // """...""" (Java text blocks, C# raw)
const F_LUA: u32 = 1 << 5;
const F_CSS: u32 = 1 << 6;
const F_SQL: u32 = 1 << 7; // '' como escape
const F_CAPTYPES: u32 = 1 << 8; // Identificador con Inicial mayúscula => Type
const F_DOLLAR: u32 = 1 << 9; // `$` válido en identificadores

pub(super) struct Spec {
    line_comments: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
    nested: bool,
    quotes: &'static [u8],
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    constants: &'static [&'static str],
    ci: bool,
    flags: u32,
}

static C_KW: &[&str] = &[
    "auto", "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for",
    "goto", "if", "inline", "register", "restrict", "return", "sizeof", "static", "struct", "switch",
    "typedef", "union", "volatile", "while",
];
static C_TY: &[&str] = &[
    "int", "char", "float", "double", "void", "long", "short", "unsigned", "signed", "size_t", "bool",
    "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "FILE",
];
static C_CONST: &[&str] = &["NULL", "true", "false"];
static CPP_KW: &[&str] = &[
    "auto", "break", "case", "catch", "class", "const", "constexpr", "continue", "decltype", "default",
    "delete", "do", "else", "enum", "explicit", "extern", "final", "for", "friend", "goto", "if",
    "inline", "mutable", "namespace", "new", "noexcept", "operator", "override", "private",
    "protected", "public", "register", "return", "sizeof", "static", "static_cast", "dynamic_cast",
    "reinterpret_cast", "const_cast", "struct", "switch", "template", "this", "throw", "try",
    "typedef", "typename", "union", "using", "virtual", "volatile", "while",
];
static CPP_TY: &[&str] = &[
    "int", "char", "float", "double", "void", "long", "short", "unsigned", "signed", "size_t", "bool",
    "string", "vector", "map", "set", "wchar_t", "int32_t", "int64_t", "uint32_t", "uint64_t",
];
static CPP_CONST: &[&str] = &["NULL", "nullptr", "true", "false"];
static CS_KW: &[&str] = &[
    "abstract", "as", "base", "break", "case", "catch", "checked", "class", "const", "continue",
    "default", "delegate", "do", "else", "enum", "event", "explicit", "extern", "finally", "fixed",
    "for", "foreach", "goto", "if", "implicit", "in", "interface", "internal", "is", "lock",
    "namespace", "new", "operator", "out", "override", "params", "private", "protected", "public",
    "readonly", "ref", "return", "sealed", "sizeof", "stackalloc", "static", "struct", "switch",
    "this", "throw", "try", "typeof", "unchecked", "unsafe", "using", "virtual", "volatile", "while",
    "async", "await", "record", "init", "get", "set", "yield", "partial", "where",
];
static CS_TY: &[&str] = &[
    "bool", "byte", "char", "decimal", "double", "float", "int", "long", "object", "sbyte", "short",
    "string", "uint", "ulong", "ushort", "void", "var", "dynamic",
];
static CS_CONST: &[&str] = &["true", "false", "null"];
static JAVA_KW: &[&str] = &[
    "abstract", "assert", "break", "case", "catch", "class", "const", "continue", "default", "do",
    "else", "enum", "extends", "final", "finally", "for", "goto", "if", "implements", "import",
    "instanceof", "interface", "native", "new", "package", "private", "protected", "public",
    "return", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws",
    "transient", "try", "volatile", "while", "record", "sealed", "permits", "yield",
];
static JAVA_TY: &[&str] = &[
    "boolean", "byte", "char", "double", "float", "int", "long", "short", "void", "var", "String",
];
static JAVA_CONST: &[&str] = &["true", "false", "null"];
static GO_KW: &[&str] = &[
    "break", "default", "func", "interface", "select", "case", "defer", "go", "map", "struct",
    "chan", "else", "goto", "package", "switch", "const", "fallthrough", "if", "range", "type",
    "continue", "for", "import", "return", "var",
];
static GO_TY: &[&str] = &[
    "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8", "int16",
    "int32", "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr", "any",
];
static GO_CONST: &[&str] = &["true", "false", "nil", "iota"];
static JS_KW: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do",
    "else", "export", "extends", "finally", "for", "function", "if", "import", "in", "instanceof",
    "let", "new", "return", "super", "switch", "this", "throw", "try", "typeof", "var", "void",
    "while", "with", "yield", "async", "await", "of", "static", "get", "set", "from", "as",
];
static JS_TY: &[&str] = &[
    "Array", "Object", "String", "Number", "Boolean", "Promise", "Map", "Set", "Date", "Error",
    "Symbol", "JSON", "Math", "RegExp",
];
static JS_CONST: &[&str] = &["true", "false", "null", "undefined", "NaN", "Infinity"];
static TS_KW: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do",
    "else", "export", "extends", "finally", "for", "function", "if", "import", "in", "instanceof",
    "let", "new", "return", "super", "switch", "this", "throw", "try", "typeof", "var", "void",
    "while", "with", "yield", "async", "await", "of", "static", "get", "set", "from", "as",
    "interface", "type", "enum", "implements", "namespace", "public", "private", "protected",
    "readonly", "abstract", "declare", "keyof", "is", "module", "infer",
];
static TS_TY: &[&str] = &[
    "string", "number", "boolean", "any", "unknown", "never", "object", "symbol", "bigint", "Array",
    "Promise", "Map", "Set", "Record", "Partial", "Date", "Error",
];
static RUST_KW: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while", "union",
];
static RUST_TY: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc",
];
static RUST_CONST: &[&str] = &["true", "false", "None", "Some", "Ok", "Err"];
static LUA_KW: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local",
    "not", "or", "repeat", "return", "then", "until", "while",
];
static LUA_CONST: &[&str] = &["true", "false", "nil"];
static SQL_KW: &[&str] = &[
    "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create",
    "alter", "drop", "table", "index", "view", "database", "schema", "join", "inner", "left", "right",
    "full", "outer", "cross", "on", "group", "by", "order", "having", "limit", "offset", "union",
    "all", "distinct", "as", "and", "or", "not", "is", "in", "like", "between", "exists", "case",
    "when", "then", "else", "end", "primary", "key", "foreign", "references", "default", "unique",
    "check", "constraint", "with", "recursive", "begin", "commit", "rollback", "transaction",
    "declare", "procedure", "function", "trigger", "returns", "return", "if", "while", "for", "loop",
    "cascade", "asc", "desc", "top", "truncate", "grant", "revoke", "use", "explain", "replace",
    "merge", "using", "natural", "over", "partition", "fetch", "next", "rows", "only", "intersect",
    "except", "any", "some", "add", "column", "rename", "to", "auto_increment", "identity", "ilike",
];
static SQL_TY: &[&str] = &[
    "int", "integer", "bigint", "smallint", "tinyint", "decimal", "numeric", "float", "real",
    "double", "varchar", "char", "text", "date", "time", "datetime", "timestamp", "boolean", "bool",
    "bit", "blob", "json", "uuid", "serial", "nvarchar", "nchar", "money", "bytea",
];
static SQL_CONST: &[&str] = &["null", "true", "false"];
static CSS_KW: &[&str] = &["important"];

const fn base() -> Spec {
    Spec {
        line_comments: &["//"],
        block: Some(("/*", "*/")),
        nested: false,
        quotes: b"\"'",
        keywords: &[],
        types: &[],
        constants: &[],
        ci: false,
        flags: 0,
    }
}

static S_C: Spec = Spec { keywords: C_KW, types: C_TY, constants: C_CONST, flags: F_PREPROC, ..base() };
static S_CPP: Spec =
    Spec { keywords: CPP_KW, types: CPP_TY, constants: CPP_CONST, flags: F_PREPROC | F_CAPTYPES, ..base() };
static S_CS: Spec = Spec {
    keywords: CS_KW,
    types: CS_TY,
    constants: CS_CONST,
    flags: F_PREPROC | F_CSHARP | F_TRIPLE | F_CAPTYPES,
    ..base()
};
static S_JAVA: Spec =
    Spec { keywords: JAVA_KW, types: JAVA_TY, constants: JAVA_CONST, flags: F_TRIPLE | F_CAPTYPES, ..base() };
static S_GO: Spec =
    Spec { keywords: GO_KW, types: GO_TY, constants: GO_CONST, quotes: b"\"'`", flags: F_BACKTICK, ..base() };
static S_JS: Spec = Spec {
    keywords: JS_KW,
    types: JS_TY,
    constants: JS_CONST,
    quotes: b"\"'`",
    flags: F_BACKTICK | F_DOLLAR | F_CAPTYPES,
    ..base()
};
static S_TS: Spec = Spec {
    keywords: TS_KW,
    types: TS_TY,
    constants: JS_CONST,
    quotes: b"\"'`",
    flags: F_BACKTICK | F_DOLLAR | F_CAPTYPES,
    ..base()
};
static S_RUST: Spec = Spec {
    keywords: RUST_KW,
    types: RUST_TY,
    constants: RUST_CONST,
    nested: true,
    flags: F_RUST | F_CAPTYPES,
    ..base()
};
static S_CSS: Spec = Spec {
    line_comments: &[],
    keywords: CSS_KW,
    flags: F_CSS,
    ..base()
};
static S_LUA: Spec = Spec {
    line_comments: &["--"],
    block: None,
    keywords: LUA_KW,
    constants: LUA_CONST,
    flags: F_LUA,
    ..base()
};
static S_SQL: Spec = Spec {
    line_comments: &["--"],
    keywords: SQL_KW,
    types: SQL_TY,
    constants: SQL_CONST,
    quotes: b"'\"`",
    ci: true,
    flags: F_SQL,
    ..base()
};
static S_PLAIN: Spec = base();

pub(super) fn spec_for(lang: Lang) -> &'static Spec {
    match lang {
        Lang::C => &S_C,
        Lang::Cpp => &S_CPP,
        Lang::CSharp => &S_CS,
        Lang::Java => &S_JAVA,
        Lang::Go => &S_GO,
        Lang::JavaScript => &S_JS,
        Lang::TypeScript => &S_TS,
        Lang::Rust => &S_RUST,
        Lang::Css => &S_CSS,
        Lang::Lua => &S_LUA,
        Lang::Sql => &S_SQL,
        _ => &S_PLAIN,
    }
}

impl Spec {
    fn has(&self, f: u32) -> bool {
        self.flags & f != 0
    }
    fn in_list(&self, list: &[&str], w: &str) -> bool {
        if self.ci {
            list.iter().any(|k| k.eq_ignore_ascii_case(w))
        } else {
            list.contains(&w)
        }
    }
}

fn st(kind: u8, param: usize) -> u16 {
    kind as u16 | ((param.min(127) as u16) << 8)
}

/// Busca el final (índice tras el terminador) de una construcción multilínea desde `from`.
fn cont_end(b: &[u8], from: usize, kind: u8, param: usize) -> Option<usize> {
    match kind {
        K_MLSTR => match param {
            1 => {
                let mut j = from;
                while j < b.len() {
                    match b[j] {
                        b'\\' => j += 2,
                        b'`' => return Some(j + 1),
                        _ => j += 1,
                    }
                }
                None
            }
            2 => find(b, from, b"\"\"\"").map(|e| e + 3),
            _ => {
                let mut j = from;
                while j < b.len() {
                    if b[j] == b'"' {
                        if b.get(j + 1) == Some(&b'"') {
                            j += 2;
                            continue;
                        }
                        return Some(j + 1);
                    }
                    j += 1;
                }
                None
            }
        },
        K_RAW => {
            let mut j = from;
            while j < b.len() {
                if b[j] == b'"' && (0..param).all(|k| b.get(j + 1 + k) == Some(&b'#')) {
                    return Some(j + 1 + param);
                }
                j += 1;
            }
            None
        }
        K_LONGC | K_LONGS => {
            let mut pat = vec![b']'];
            pat.extend(std::iter::repeat_n(b'=', param));
            pat.push(b']');
            find(b, from, &pat).map(|e| e + pat.len())
        }
        _ => Some(from),
    }
}

fn kind_tok(kind: u8) -> TokKind {
    if kind == K_BLOCK || kind == K_LONGC { TokKind::Comment } else { TokKind::String }
}

/// Lua: `[` `=`* `[` en `i` => nivel.
fn long_bracket(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&b'=') {
        j += 1;
    }
    (b.get(j) == Some(&b'[')).then(|| j - i - 1)
}

pub(super) fn scan(sp: &Spec, line: &str, state: u16, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let mut css_in = state & CSS_IN != 0;
    let cst = state & 0x7fff;
    let (kind, param) = ((cst & 0xff) as u8, (cst >> 8) as usize);
    let mut i = 0usize;
    let css = |in_block: bool| if in_block { CSS_IN } else { 0 };

    // ---- continuación de construcciones multilínea ----
    match kind {
        K_BLOCK if sp.block.is_some() => match block(sp, b, 0, 0, param.max(1), o) {
            Ok(e) => i = e,
            Err(s) => return s | css(css_in),
        },
        K_MLSTR | K_RAW | K_LONGC | K_LONGS => match cont_end(b, 0, kind, param) {
            Some(e) => {
                o.push(0, e, kind_tok(kind));
                i = e;
            }
            None => {
                o.push(0, n, kind_tok(kind));
                return st(kind, param) | css(css_in);
            }
        },
        _ => {}
    }

    let first_nonws = b.iter().position(|c| !matches!(c, b' ' | b'\t')).unwrap_or(n);
    let mut after_def: Option<TokKind> = None;
    while i < n {
        let c = b[i];
        if c == b' ' || c == b'\t' {
            i += 1;
            continue;
        }
        // ---- preprocesador / atributos ----
        if c == b'#' && sp.has(F_PREPROC) && i == first_nonws {
            o.push(i, n, TokKind::Meta);
            i = n;
            continue;
        }
        if c == b'#' && sp.has(F_RUST) && matches!(b.get(i + 1), Some(b'[') | Some(b'!')) {
            let mut j = i + 1;
            if b[j] == b'!' {
                j += 1;
            }
            if b.get(j) == Some(&b'[') {
                let mut depth = 0i32;
                let mut e = n;
                for (k, &ch) in b.iter().enumerate().skip(j) {
                    if ch == b'[' {
                        depth += 1;
                    } else if ch == b']' {
                        depth -= 1;
                        if depth == 0 {
                            e = k + 1;
                            break;
                        }
                    }
                }
                o.push(i, e, TokKind::Meta);
                i = e;
                continue;
            }
        }
        // ---- comentarios ----
        if sp.has(F_LUA) && b[i..].starts_with(b"--") {
            if let Some(level) = long_bracket(b, i + 2) {
                match cont_end(b, i + 4 + level, K_LONGC, level) {
                    Some(e) => {
                        o.push(i, e, TokKind::Comment);
                        i = e;
                    }
                    None => {
                        o.push(i, n, TokKind::Comment);
                        return st(K_LONGC, level);
                    }
                }
                continue;
            }
        }
        if let Some(lc) = sp.line_comments.iter().find(|lc| b[i..].starts_with(lc.as_bytes())) {
            let _ = lc;
            o.push(i, n, TokKind::Comment);
            i = n;
            continue;
        }
        if let Some((open, _)) = sp.block {
            if b[i..].starts_with(open.as_bytes()) {
                match block(sp, b, i, i + open.len(), 1, o) {
                    Ok(e) => i = e,
                    Err(s) => return s | css(css_in),
                }
                continue;
            }
        }
        // ---- cadenas ----
        if sp.has(F_LUA) {
            if let Some(level) = long_bracket(b, i) {
                match cont_end(b, i + 2 + level, K_LONGS, level) {
                    Some(e) => {
                        o.push(i, e, TokKind::String);
                        i = e;
                    }
                    None => {
                        o.push(i, n, TokKind::String);
                        return st(K_LONGS, level);
                    }
                }
                continue;
            }
        }
        if sp.has(F_CSHARP) && (c == b'@' || c == b'$') && i + 1 < n {
            let mut j = i;
            let mut verbatim = false;
            while j < n && (b[j] == b'@' || b[j] == b'$') && j < i + 2 {
                verbatim |= b[j] == b'@';
                j += 1;
            }
            if b.get(j) == Some(&b'"') {
                if verbatim {
                    match cont_end(b, j + 1, K_MLSTR, 3) {
                        Some(e) => {
                            o.push(i, e, TokKind::String);
                            i = e;
                        }
                        None => {
                            o.push(i, n, TokKind::String);
                            return st(K_MLSTR, 3);
                        }
                    }
                } else {
                    let e = quoted_end(b, j, b'"');
                    o.push(i, e, TokKind::String);
                    i = e;
                }
                continue;
            }
        }
        if c == b'"' && sp.has(F_TRIPLE) && b[i..].starts_with(b"\"\"\"") {
            match cont_end(b, i + 3, K_MLSTR, 2) {
                Some(e) => {
                    o.push(i, e, TokKind::String);
                    i = e;
                }
                None => {
                    o.push(i, n, TokKind::String);
                    return st(K_MLSTR, 2);
                }
            }
            continue;
        }
        if sp.quotes.contains(&c) {
            if c == b'`' && sp.has(F_BACKTICK) {
                match cont_end(b, i + 1, K_MLSTR, 1) {
                    Some(e) => {
                        o.push(i, e, TokKind::String);
                        i = e;
                    }
                    None => {
                        o.push(i, n, TokKind::String);
                        return st(K_MLSTR, 1) | css(css_in);
                    }
                }
                continue;
            }
            if c == b'\'' && sp.has(F_RUST) {
                // `'a'` carácter o `'lifetime`.
                if b.get(i + 1) == Some(&b'\\') {
                    let e = quoted_end(b, i, b'\'');
                    o.push(i, e, TokKind::String);
                    i = e;
                } else if i + 1 < n {
                    let l = utf8_len(b[i + 1]);
                    if b.get(i + 1 + l) == Some(&b'\'') {
                        o.push(i, i + 2 + l, TokKind::String);
                        i += 2 + l;
                    } else {
                        let mut e = i + 1;
                        while e < n && is_ident(b[e]) {
                            e += 1;
                        }
                        o.push(i, e, TokKind::Type);
                        i = e;
                    }
                } else {
                    i += 1;
                }
                continue;
            }
            let e = if sp.has(F_SQL) {
                sql_quote_end(b, i, c)
            } else {
                quoted_end(b, i, c)
            };
            let k = if c == b'`' && sp.has(F_SQL) { TokKind::Key } else { TokKind::String };
            o.push(i, e, k);
            i = e;
            continue;
        }
        // ---- números ----
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let e = number_end(b, i);
            o.push(i, e, TokKind::Number);
            i = e;
            continue;
        }
        // ---- CSS: #color, .clase, #id, @regla ----
        if sp.has(F_CSS) {
            if c == b'#' {
                let mut e = i + 1;
                while e < n && b[e].is_ascii_hexdigit() {
                    e += 1;
                }
                if css_in && (e - i == 4 || e - i == 7 || e - i == 9 || e - i == 5) && !b.get(e).is_some_and(|&d| is_ident(d)) {
                    o.push(i, e, TokKind::Number);
                } else {
                    while e < n && (is_ident(b[e]) || b[e] == b'-') {
                        e += 1;
                    }
                    o.push(i, e, if css_in { TokKind::Text } else { TokKind::Type });
                }
                i = e.max(i + 1);
                continue;
            }
            if (c == b'.' || c == b'@') && b.get(i + 1).is_some_and(|&d| is_ident_start(d) || d == b'-') {
                let mut e = i + 1;
                while e < n && (is_ident(b[e]) || b[e] == b'-') {
                    e += 1;
                }
                let k = if c == b'@' {
                    TokKind::Keyword
                } else if css_in {
                    TokKind::Text
                } else {
                    TokKind::Type
                };
                o.push(i, e, k);
                i = e;
                continue;
            }
        }
        // ---- identificadores ----
        let css_dash = sp.has(F_CSS) && c == b'-' && b.get(i + 1).is_some_and(|&d| is_ident_start(d) || d == b'-');
        if is_ident_start(c) || (c == b'$' && sp.has(F_DOLLAR)) || css_dash {
            let mut e = i + 1;
            while e < n && (is_ident(b[e]) || (sp.has(F_CSS) && b[e] == b'-') || (sp.has(F_DOLLAR) && b[e] == b'$')) {
                e += 1;
            }
            let word = &line[i..e];
            // Rust: cadenas crudas / de bytes.
            if sp.has(F_RUST) && matches!(word, "r" | "br" | "b") && matches!(b.get(e), Some(b'"') | Some(b'#')) {
                if word == "b" {
                    if b[e] == b'"' {
                        let e2 = quoted_end(b, e, b'"');
                        o.push(i, e2, TokKind::String);
                        i = e2;
                        continue;
                    }
                } else {
                    let mut h = 0;
                    while b.get(e + h) == Some(&b'#') {
                        h += 1;
                    }
                    if b.get(e + h) == Some(&b'"') {
                        match cont_end(b, e + h + 1, K_RAW, h) {
                            Some(e2) => {
                                o.push(i, e2, TokKind::String);
                                i = e2;
                            }
                            None => {
                                o.push(i, n, TokKind::String);
                                return st(K_RAW, h);
                            }
                        }
                        continue;
                    }
                }
            }
            let mut j = e;
            while j < n && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            let next = b.get(j).copied();
            let is_call = b.get(e) == Some(&b'(');
            let mut k = TokKind::Text;
            if let Some(dk) = after_def.take() {
                k = dk;
            } else if sp.in_list(sp.keywords, word) {
                k = TokKind::Keyword;
                after_def = match word {
                    "fn" | "function" | "func" | "def" => Some(TokKind::Function),
                    "class" | "struct" | "enum" | "trait" | "interface" | "namespace" => Some(TokKind::Type),
                    _ => None,
                };
            } else if sp.in_list(sp.constants, word) {
                k = TokKind::Constant;
            } else if sp.in_list(sp.types, word) {
                k = TokKind::Type;
            } else if sp.has(F_CSS) && css_in && next == Some(b':') && b.get(j + 1) != Some(&b':') {
                k = TokKind::Key;
            } else if sp.has(F_RUST) && b.get(e) == Some(&b'!') && b.get(e + 1) != Some(&b'=') {
                o.push(i, e + 1, TokKind::Function);
                i = e + 1;
                continue;
            } else if is_call {
                k = TokKind::Function;
            } else if sp.has(F_CAPTYPES)
                && word.as_bytes()[0].is_ascii_uppercase()
                && word.bytes().any(|d| d.is_ascii_lowercase())
            {
                k = TokKind::Type;
            }
            o.push(i, e, k);
            i = e;
            continue;
        }
        // ---- puntuación / operadores ----
        match c {
            b'{' | b'}' | b'[' | b']' | b'(' | b')' | b';' | b',' | b'.' => {
                if sp.has(F_CSS) {
                    if c == b'{' {
                        css_in = true;
                    } else if c == b'}' {
                        css_in = false;
                    }
                }
                o.push(i, i + 1, TokKind::Punct);
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^' | b'~' | b'?'
            | b':' | b'@' => {
                o.push(i, i + 1, TokKind::Operator);
            }
            _ => {}
        }
        i += 1;
    }
    css(css_in)
}

fn utf8_len(first: u8) -> usize {
    match first {
        0..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn sql_quote_end(b: &[u8], i: usize, q: u8) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == q {
            if b.get(j + 1) == Some(&q) {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

/// Comentario de bloque que empieza en `start` (escaneo desde `from`, profundidad inicial
/// `depth`). `Ok(fin)` si se cierra en esta línea; `Err(estado)` si continúa.
fn block(sp: &Spec, b: &[u8], start: usize, from: usize, mut depth: usize, o: &mut Out) -> Result<usize, u16> {
    let (open, close) = sp.block.unwrap();
    let mut j = from;
    loop {
        let no = if sp.nested { find(b, j, open.as_bytes()) } else { None };
        let nc = find(b, j, close.as_bytes());
        match (no, nc) {
            (Some(p), Some(c)) if p < c => {
                depth += 1;
                j = p + open.len();
            }
            (_, Some(c)) => {
                j = c + close.len();
                depth -= 1;
                if depth == 0 {
                    o.push(start, j, TokKind::Comment);
                    return Ok(j);
                }
            }
            _ => {
                o.push(start, b.len(), TokKind::Comment);
                return Err(st(K_BLOCK, depth));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::tests::*;
    use crate::syntax::{Lang, TokKind::*};

    #[test]
    fn rust_basics() {
        let l = "pub fn main() { let x: u32 = 42; println!(\"hi ñ\"); } // fin";
        assert!(has(Lang::Rust, l, "pub", Keyword));
        assert!(has(Lang::Rust, l, "main", Function));
        assert!(has(Lang::Rust, l, "u32", Type));
        assert!(has(Lang::Rust, l, "42", Number));
        assert!(has(Lang::Rust, l, "println!", Function));
        assert!(has(Lang::Rust, l, "\"hi ñ\"", String));
        assert!(has(Lang::Rust, l, "// fin", Comment));
    }
    #[test]
    fn rust_raw_string_and_lifetime_and_attr() {
        assert!(has(Lang::Rust, "let s = r#\"a \"quoted\" b\"#;", "r#\"a \"quoted\" b\"#", String));
        assert!(has(Lang::Rust, "fn f<'a>(x: &'a str) {}", "'a", Type));
        assert!(has(Lang::Rust, "let c = 'x'; let d = 'ñ';", "'ñ'", String));
        assert!(has(Lang::Rust, "#[derive(Debug)]", "#[derive(Debug)]", Meta));
    }
    #[test]
    fn rust_raw_string_multiline_state() {
        let r = multi(Lang::Rust, "let s = r##\"abc\n\"# still\nend\"## + 1;");
        assert_eq!(r[1], vec![("\"# still".into(), String)]);
        assert!(r[2].contains(&("end\"##".into(), String)));
        assert!(r[2].contains(&("1".into(), Number)));
    }
    #[test]
    fn rust_nested_block_comment() {
        let r = multi(Lang::Rust, "a /* x /* y */ still */ b\n/* open\n/* n */ mid\nend */ let z;");
        assert!(r[0].contains(&("/* x /* y */ still */".into(), Comment)));
        assert_eq!(r[2], vec![("/* n */ mid".into(), Comment)]);
        assert!(r[3].contains(&("end */".into(), Comment)));
        assert!(r[3].contains(&("let".into(), Keyword)));
    }
    #[test]
    fn c_block_comment_across_lines() {
        let r = multi(Lang::C, "int x; /* a\nb\nc */ int y;\n#include <stdio.h>");
        assert_eq!(r[1], vec![("b".into(), Comment)]);
        assert!(r[2].contains(&("c */".into(), Comment)));
        assert!(r[2].contains(&("int".into(), Type)));
        assert_eq!(r[3][0], ("#include <stdio.h>".into(), Meta));
    }
    #[test]
    fn sql_case_insensitive_and_quote_escape() {
        let l = "SELECT name, 'it''s ñ' FROM Users WHERE id = 10 -- c";
        assert!(has(Lang::Sql, l, "SELECT", Keyword));
        assert!(has(Lang::Sql, l, "FROM", Keyword));
        assert!(has(Lang::Sql, l, "WHERE", Keyword));
        assert!(has(Lang::Sql, l, "'it''s ñ'", String));
        assert!(has(Lang::Sql, l, "10", Number));
        assert!(has(Lang::Sql, l, "-- c", Comment));
        assert!(has(Lang::Sql, "select Int, null from t", "null", Constant));
        assert!(has(Lang::Sql, "select cast(x as int)", "int", Type));
        let r = multi(Lang::Sql, "select 1 /* a\n b */ from t");
        assert_eq!(r[1][0], (" b */".into(), Comment));
    }
    #[test]
    fn js_template_multiline_and_keywords() {
        let r = multi(Lang::JavaScript, "const s = `a ${x}\nb` + 1;\nfunction foo() { return null }");
        assert!(r[0].contains(&("`a ${x}".into(), String)));
        assert!(r[0].contains(&("const".into(), Keyword)));
        assert_eq!(r[1][0], ("b`".into(), String));
        assert!(r[2].contains(&("foo".into(), Function)));
        assert!(r[2].contains(&("null".into(), Constant)));
    }
    #[test]
    fn ts_types() {
        assert!(has(Lang::TypeScript, "let a: string = 'x';", "string", Type));
        assert!(has(Lang::TypeScript, "interface Foo {}", "Foo", Type));
    }
    #[test]
    fn go_raw_string_state() {
        let r = multi(Lang::Go, "x := `raw\nmore` + \"s\"");
        assert_eq!(r[1][0], ("more`".into(), String));
    }
    #[test]
    fn csharp_verbatim_and_java_text_block() {
        let r = multi(Lang::CSharp, "var s = @\"a\n\"\"b\"\" c\"; int x;");
        assert!(r[1].contains(&("\"\"b\"\" c\"".into(), String)));
        assert!(r[1].contains(&("int".into(), Type)));
        let r = multi(Lang::Java, "String t = \"\"\"\n  hi ñ\n  \"\"\";");
        assert_eq!(r[1], vec![("  hi ñ".into(), String)]);
    }
    #[test]
    fn lua_long_bracket_states() {
        let r = multi(Lang::Lua, "--[==[ c\n]] still ]==] local x = [[s\nt]]");
        assert_eq!(r[0][0], ("--[==[ c".into(), Comment));
        assert_eq!(r[1][0], ("]] still ]==]".into(), Comment));
        assert!(r[1].contains(&("local".into(), Keyword)));
        assert_eq!(r[2][0], ("t]]".into(), String));
        assert!(has(Lang::Lua, "x = 1 -- hi", "-- hi", Comment));
    }
    #[test]
    fn css_selectors_props_colors() {
        let r = multi(Lang::Css, ".card:hover {\n  color: #fff; margin: 10px 2em;\n}\n@media (x) {}");
        assert!(r[0].contains(&(".card".into(), Type)));
        assert!(r[1].contains(&("color".into(), Key)));
        assert!(r[1].contains(&("#fff".into(), Number)));
        assert!(r[1].contains(&("10px".into(), Number)));
        assert!(r[3].contains(&("@media".into(), Keyword)));
        assert!(has(Lang::Css, "a { background-color: red; }", "background-color", Key));
    }
    #[test]
    fn cpp_and_utf8() {
        assert!(has(Lang::Cpp, "std::string s = \"ñ😀\"; // ü", "\"ñ😀\"", String));
        assert!(has(Lang::Cpp, "class Foo : public Bar {", "Foo", Type));
    }
}

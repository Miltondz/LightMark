//! Resaltado de sintaxis por líneas (WS-A): tokenizadores escritos a mano, a nivel de bytes,
//! con estado por línea (`LineState`) para comentarios de bloque, cadenas multilínea, cercas
//! de Markdown, etc. Todas las fronteras de token caen en bytes ASCII (o inicio/fin de línea),
//! por lo que siempre son fronteras de carácter UTF-8.
#![allow(dead_code)]

mod cfamily;
mod config;
mod json;
mod markdown;
mod markup;
mod scripting;

/// Lenguajes soportados por el resaltado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Plain,
    Markdown,
    Json,
    Xml,
    Html,
    Sql,
    JavaScript,
    TypeScript,
    Css,
    Rust,
    Python,
    Toml,
    Yaml,
    Shell,
    PowerShell,
    Ini,
    C,
    Cpp,
    CSharp,
    Java,
    Go,
    Lua,
}

/// Categoría de un token (índice en la paleta).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokKind {
    Text,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Punct,
    Operator,
    Key,
    Constant,
    Tag,
    Attr,
    Entity,
    Heading,
    Emphasis,
    Code,
    Link,
    Meta,
}

impl TokKind {
    pub const COUNT: usize = 19;
}

/// Estado de tokenizador al inicio de una línea (0 = normal).
pub type LineState = u16;

/// Convierte el id de `editor::detect_language` en `Lang`.
pub fn lang_from_id(id: &str) -> Lang {
    match id {
        "markdown" => Lang::Markdown,
        "json" => Lang::Json,
        "xml" => Lang::Xml,
        "html" => Lang::Html,
        "sql" => Lang::Sql,
        "javascript" => Lang::JavaScript,
        "typescript" => Lang::TypeScript,
        "css" => Lang::Css,
        "rust" => Lang::Rust,
        "python" => Lang::Python,
        "toml" => Lang::Toml,
        "yaml" => Lang::Yaml,
        "bash" | "shell" => Lang::Shell,
        "powershell" => Lang::PowerShell,
        "ini" => Lang::Ini,
        "c" => Lang::C,
        "cpp" => Lang::Cpp,
        "csharp" => Lang::CSharp,
        "java" => Lang::Java,
        "go" => Lang::Go,
        "lua" => Lang::Lua,
        _ => Lang::Plain,
    }
}

/// Emisor de tokens: garantiza tokens ordenados, sin solapes, que cubren toda la línea
/// (los huecos son `Text`) y fusiona tokens adyacentes del mismo tipo.
pub(crate) struct Out<'a> {
    v: &'a mut Vec<(u32, u32, TokKind)>,
    base: usize,
    pos: usize,
}

impl<'a> Out<'a> {
    pub(crate) fn new(v: &'a mut Vec<(u32, u32, TokKind)>) -> Self {
        let base = v.len();
        Out { v, base, pos: 0 }
    }
    fn raw(&mut self, s: usize, e: usize, k: TokKind) {
        if self.v.len() > self.base {
            let last = self.v.last_mut().unwrap();
            if last.2 == k && last.1 as usize == s {
                last.1 = e as u32;
                return;
            }
        }
        self.v.push((s as u32, e as u32, k));
    }
    pub(crate) fn push(&mut self, s: usize, e: usize, k: TokKind) {
        if e <= s || s < self.pos {
            return;
        }
        if s > self.pos {
            self.raw(self.pos, s, TokKind::Text);
        }
        self.raw(s, e, k);
        self.pos = e;
    }
    pub(crate) fn finish(&mut self, len: usize) {
        if self.pos < len {
            self.raw(self.pos, len, TokKind::Text);
            self.pos = len;
        }
    }
}

/// Lenguajes cuyo estado de línea siempre es 0 (se omite el barrido de estado).
pub fn is_stateless(lang: Lang) -> bool {
    matches!(lang, Lang::Plain | Lang::Json | Lang::Ini)
}

/// Tokeniza una línea (sin `\n` final; un `\r` final se ignora): añade `(inicio, fin, tipo)`
/// (bytes) a `out` y devuelve el estado al final de la línea.
pub fn tokenize_line(
    lang: Lang,
    line: &str,
    state: LineState,
    out: &mut Vec<(u32, u32, TokKind)>,
) -> LineState {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut o = Out::new(out);
    let st = match lang {
        Lang::Plain => 0,
        Lang::Json => json::scan(line, &mut o),
        Lang::Xml => markup::scan(line, state, &mut o, false),
        Lang::Html => markup::scan(line, state, &mut o, true),
        Lang::Markdown => markdown::scan(line, state, &mut o),
        Lang::Yaml => config::scan_yaml(line, state, &mut o),
        Lang::Toml => config::scan_ini(line, state, &mut o, true),
        Lang::Ini => config::scan_ini(line, state, &mut o, false),
        Lang::Python => scripting::scan_python(line, state, &mut o),
        Lang::Shell => scripting::scan_shell(line, &mut o),
        Lang::PowerShell => scripting::scan_powershell(line, state, &mut o),
        other => cfamily::scan(cfamily::spec_for(other), line, state, &mut o),
    };
    o.finish(line.len());
    st
}

const fn c(r: u8, g: u8, b: u8) -> slint::Color {
    slint::Color::from_rgb_u8(r, g, b)
}

/// Paleta por tema (0 = Oscuro, 1 = Océano), indexada por `TokKind as usize`.
pub fn palette(theme: u32) -> [slint::Color; TokKind::COUNT] {
    if theme == 1 {
        [
            c(0xe6, 0xed, 0xf0), // Text
            c(0x7f, 0xd4, 0xe0), // Keyword
            c(0x6f, 0xd1, 0xc0), // Type
            c(0xe8, 0xd9, 0xa0), // Function
            c(0xe0, 0xb8, 0x8a), // String
            c(0xb8, 0xe0, 0xa8), // Number
            c(0x6f, 0x9c, 0x92), // Comment
            c(0xc9, 0xd4, 0xd8), // Punct
            c(0xc9, 0xd4, 0xd8), // Operator
            c(0x9a, 0xd7, 0xee), // Key
            c(0x7f, 0xd4, 0xe0), // Constant
            c(0x7f, 0xd4, 0xe0), // Tag
            c(0x9a, 0xd7, 0xee), // Attr
            c(0xe0, 0xc0, 0x8a), // Entity
            c(0x7f, 0xd4, 0xe0), // Heading
            c(0xd6, 0xa6, 0xe0), // Emphasis
            c(0xe0, 0xb8, 0x8a), // Code
            c(0x55, 0xb8, 0xc7), // Link
            c(0xd6, 0xa6, 0xe0), // Meta
        ]
    } else {
        [
            c(0xcc, 0xcc, 0xcc),
            c(0x56, 0x9c, 0xd6),
            c(0x4e, 0xc9, 0xb0),
            c(0xdc, 0xdc, 0xaa),
            c(0xce, 0x91, 0x78),
            c(0xb5, 0xce, 0xa8),
            c(0x6a, 0x99, 0x55),
            c(0xcc, 0xcc, 0xcc),
            c(0xd4, 0xd4, 0xd4),
            c(0x9c, 0xdc, 0xfe),
            c(0x56, 0x9c, 0xd6),
            c(0x56, 0x9c, 0xd6),
            c(0x9c, 0xdc, 0xfe),
            c(0xd7, 0xba, 0x7d),
            c(0x56, 0x9c, 0xd6),
            c(0xc5, 0x86, 0xc0),
            c(0xce, 0x91, 0x78),
            c(0x37, 0x94, 0xff),
            c(0xc5, 0x86, 0xc0),
        ]
    }
}

// --- utilidades compartidas por los tokenizadores ---------------------------------------

pub(crate) fn find(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || from > b.len() || b.len() - from < pat.len() {
        return None;
    }
    let last = b.len() - pat.len();
    let first = pat[0];
    let mut i = from;
    while i <= last {
        if b[i] == first && &b[i..i + pat.len()] == pat {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub(crate) fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

pub(crate) fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// Fin de una cadena entre comillas `q` que empieza en `i` (posición de la comilla de
/// apertura), con escapes `\`. Devuelve el índice tras la comilla de cierre, o `b.len()`.
pub(crate) fn quoted_end(b: &[u8], i: usize, q: u8) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == q => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// Consume un número (decimal/hex/flotante/sufijos) que empieza en `i`.
pub(crate) fn number_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    let hex = b[i] == b'0'
        && b.get(i + 1).is_some_and(|c| matches!(c, b'x' | b'X' | b'b' | b'B' | b'o' | b'O'));
    while j < b.len() {
        let c = b[j];
        if c.is_ascii_alphanumeric() || c == b'_' {
            j += 1;
        } else if c == b'.' && b.get(j + 1).is_some_and(|d| d.is_ascii_digit()) {
            j += 1;
        } else if (c == b'+' || c == b'-') && !hex && j > i && matches!(b[j - 1], b'e' | b'E') {
            j += 1;
        } else {
            break;
        }
    }
    j.max(i + 1)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn toks(lang: Lang, line: &str) -> Vec<(String, TokKind)> {
        let mut v = Vec::new();
        tokenize_line(lang, line, 0, &mut v);
        v.iter().map(|&(s, e, k)| (line[s as usize..e as usize].to_string(), k)).collect()
    }
    pub(crate) fn has(lang: Lang, line: &str, text: &str, kind: TokKind) -> bool {
        toks(lang, line).iter().any(|(t, k)| t == text && *k == kind)
    }
    /// Tokeniza varias líneas encadenando el estado y devuelve los tokens por línea.
    pub(crate) fn multi(lang: Lang, src: &str) -> Vec<Vec<(String, TokKind)>> {
        let mut st = 0;
        src.split('\n')
            .map(|line| {
                let mut v = Vec::new();
                st = tokenize_line(lang, line, st, &mut v);
                v.iter().map(|&(s, e, k)| (line[s as usize..e as usize].to_string(), k)).collect()
            })
            .collect()
    }

    const ALL: [Lang; 22] = [
        Lang::Plain, Lang::Markdown, Lang::Json, Lang::Xml, Lang::Html, Lang::Sql, Lang::JavaScript,
        Lang::TypeScript, Lang::Css, Lang::Rust, Lang::Python, Lang::Toml, Lang::Yaml, Lang::Shell,
        Lang::PowerShell, Lang::Ini, Lang::C, Lang::Cpp, Lang::CSharp, Lang::Java, Lang::Go, Lang::Lua,
    ];

    const CORPUS: &str = "let ñandú = \"héllo 😀 漢字\"; // comentário ñ\n\
        \t<a href='x'>é&amp;😀</a> # ñ -- 😀 /* ñ */ \"\"\"😀\n\
        **ñ** `😀` [漢](url) key: ñ = 'ü' [sec] $ñ ${😀} #ñ 12ñ 0x1F😀 r#\"ñ\"# 'a 😀\n\
        {\"ñ\": \"😀\", \"n\": 1.5e+3} ---\nñ\r\n\
        - item: |\n  ñ 😀\n<!-- ñ --> <![CDATA[😀]]> \"@ '@ #> ]] ]=]";

    #[test]
    fn boundaries_are_char_boundaries_and_cover_line() {
        for lang in ALL {
            for line in CORPUS.split('\n') {
                for state in [0u16, 1, 2, 3, 4, 5, 6, 0x0101, 0x0203, 0x0301, 0x8000, 0x8101, 0xffff] {
                    let mut v = Vec::new();
                    tokenize_line(lang, line, state, &mut v);
                    let l = line.strip_suffix('\r').unwrap_or(line);
                    let mut pos = 0usize;
                    for &(s, e, _) in &v {
                        assert_eq!(s as usize, pos, "{lang:?} gap/overlap in {line:?} state {state}");
                        assert!(e > s, "{lang:?} empty token");
                        assert!(
                            l.is_char_boundary(s as usize) && l.is_char_boundary(e as usize),
                            "{lang:?} {line:?}"
                        );
                        pos = e as usize;
                    }
                    assert_eq!(pos, l.len(), "{lang:?} does not cover {line:?} state {state}");
                }
            }
        }
    }

    #[test]
    fn empty_line_has_no_tokens_and_keeps_state() {
        for lang in ALL {
            let mut v = Vec::new();
            tokenize_line(lang, "", 0, &mut v);
            assert!(v.is_empty());
        }
    }

    #[test]
    fn crlf_is_ignored() {
        assert_eq!(toks(Lang::Json, "{\"a\": 1}\r"), toks(Lang::Json, "{\"a\": 1}"));
    }

    #[test]
    fn palette_differs_between_themes() {
        assert_eq!(palette(0).len(), TokKind::COUNT);
        assert_ne!(palette(0)[TokKind::Keyword as usize], palette(1)[TokKind::Keyword as usize]);
    }

    #[test]
    fn lang_ids() {
        assert_eq!(lang_from_id("json"), Lang::Json);
        assert_eq!(lang_from_id("nope"), Lang::Plain);
    }

    #[test]
    fn perf_1mb_json_window_under_50ms() {
        let mut doc = String::new();
        let mut i = 0;
        while doc.len() < 1_000_000 {
            doc.push_str(&format!(
                "  {{\"id\": {i}, \"name\": \"item número {i}\", \"ok\": true, \"v\": [1, 2.5, null]}},\n"
            ));
            i += 1;
        }
        let lines: Vec<&str> = doc.split('\n').collect();
        let t = std::time::Instant::now();
        let mut v = Vec::new();
        for l in lines.iter().take(200) {
            v.clear();
            tokenize_line(Lang::Json, l, 0, &mut v);
        }
        assert!(t.elapsed().as_millis() < 50, "{:?}", t.elapsed());
        // Documento entero (solo el resaltado con estado necesita esto): no debe ser patológico.
        let t = std::time::Instant::now();
        for l in &lines {
            v.clear();
            tokenize_line(Lang::Json, l, 0, &mut v);
        }
        assert!(t.elapsed().as_millis() < 5000, "{:?}", t.elapsed());
    }
}

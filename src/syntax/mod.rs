//! Resaltado de sintaxis por líneas (WS-A). Wave 0: solo el esqueleto de tipos y un
//! tokenizador que devuelve un único token `Text` por línea.
#![allow(dead_code)] // wave-1 fills (WS-A)

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

/// Tokeniza una línea: añade `(inicio, fin, tipo)` (bytes) a `out` y devuelve el estado
/// al final de la línea. Stub: un único token `Text` que cubre toda la línea.
pub fn tokenize_line(
    _lang: Lang,
    line: &str,
    state: LineState,
    out: &mut Vec<(u32, u32, TokKind)>,
) -> LineState {
    if !line.is_empty() {
        out.push((0, line.len() as u32, TokKind::Text));
    }
    state
}

/// Paleta por tema (0 = Oscuro, 1 = Océano). Stub: todo el color de texto.
pub fn palette(_theme: u32) -> [slint::Color; TokKind::COUNT] {
    [slint::Color::from_rgb_u8(0xcc, 0xcc, 0xcc); TokKind::COUNT]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_covers_line() {
        let mut v = Vec::new();
        tokenize_line(Lang::Json, "{\"a\": 1}", 0, &mut v);
        assert_eq!(v, vec![(0, 8, TokKind::Text)]);
    }

    #[test]
    fn lang_ids() {
        assert_eq!(lang_from_id("json"), Lang::Json);
        assert_eq!(lang_from_id("nope"), Lang::Plain);
    }
}

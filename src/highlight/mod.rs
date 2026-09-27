#![allow(dead_code)]

use ropey::Rope;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Bold,
    Italic,
    CodeInline,
    CodeBlock,
    Link,
    LinkText,
    Quote,
    ListMarker,
    ListItem,
    Comment,
    Tag,
    Attribute,
    StringLiteral,
    Number,
    Keyword,
    Operator,
    Punctuation,
    HtmlTag,
    JsonKey,
    JsonString,
    JsonNumber,
    JsonBoolean,
    JsonNull,
    Url,
    Email,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub range: std::ops::Range<usize>,
    pub line: usize,
}

pub struct Highlighter {
    language: String,
}

impl Highlighter {
    pub fn new(language: &str) -> Self {
        Self {
            language: language.to_string(),
        }
    }

    pub fn highlight(&self, rope: &Rope) -> Vec<Token> {
        match self.language.as_str() {
            "markdown" => self.highlight_markdown(rope),
            "html" | "htm" => self.highlight_html(rope),
            "sql" => self.highlight_sql(rope),
            "json" => self.highlight_json(rope),
            "xml" => self.highlight_xml(rope),
            "javascript" => self.highlight_js(rope),
            "plaintext" | "txt" => self.highlight_text(rope),
            _ => self.highlight_text(rope),
        }
    }

    fn highlight_text(&self, rope: &Rope) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut offset = 0usize;

        for (line, lr) in rope.lines(ropey::LineType::Unicode).enumerate() {
            let text = lr.to_string();
            tokens.push(Token {
                kind: TokenKind::Text,
                range: offset..offset + text.len(),
                line,
            });
            offset += text.len();
        }

        tokens
    }

    fn highlight_markdown(&self, rope: &Rope) -> Vec<Token> {
        let mut tokens = Vec::new();
        let content = rope.to_string();
        let mut offset = 0usize;

        for (line, lr) in rope.lines(ropey::LineType::Unicode).enumerate() {
            let text = lr.to_string();
            let line_start = offset;

            if text.starts_with('#') {
                let level = text
                    .chars()
                    .take_while(|&c| c == '#' && c != ' ')
                    .count()
                    .min(6);
                let heading_kind = match level {
                    1 => TokenKind::Heading1,
                    2 => TokenKind::Heading2,
                    3 => TokenKind::Heading3,
                    4 => TokenKind::Heading4,
                    5 => TokenKind::Heading5,
                    _ => TokenKind::Heading6,
                };

                tokens.push(Token {
                    kind: heading_kind,
                    range: line_start..offset + text.len(),
                    line,
                });
            } else if text.starts_with('>') {
                tokens.push(Token {
                    kind: TokenKind::Quote,
                    range: line_start..line_start + 1,
                    line,
                });
                tokens.push(Token {
                    kind: TokenKind::Text,
                    range: line_start + 1..offset + text.len(),
                    line,
                });
            } else if text.trim_start().starts_with("```") {
                tokens.push(Token {
                    kind: TokenKind::CodeBlock,
                    range: line_start..offset + text.len(),
                    line,
                });
            } else if text.starts_with("- ")
                || text.starts_with("* ")
                || text.starts_with("+ ")
                || text
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            {
                tokens.push(Token {
                    kind: TokenKind::ListMarker,
                    range: line_start..line_start + 2,
                    line,
                });
                tokens.push(Token {
                    kind: TokenKind::ListItem,
                    range: line_start + 2..offset + text.len(),
                    line,
                });
            } else {
                tokens.push(Token {
                    kind: TokenKind::Text,
                    range: line_start..offset + text.len(),
                    line,
                });

                // Find inline code, bold, italic
                self.find_inline_markdown(&content, line_start, line, &mut tokens);
            }

            offset += text.len();
        }

        tokens
    }

    fn find_inline_markdown(
        &self,
        content: &str,
        line_start: usize,
        line: usize,
        tokens: &mut Vec<Token>,
    ) {
        // Escanea por CARACTERES (no bytes crudos) para no partir un carácter multibyte a la
        // mitad al construir los rangos de token (hallazgo G: pánico UTF-8 con "función",
        // "t😀", etc. — `content.as_bytes().windows(2)` podía producir rangos que caían en
        // mitad de una secuencia UTF-8, que luego reventaba al indexar `&content[range]`).
        let limit = (line_start + 1000).min(content.len());
        let mut cut = limit;
        while cut < content.len() && !content.is_char_boundary(cut) {
            cut += 1;
        }
        let Some(slice) = content.get(line_start..cut) else {
            return;
        };
        let chars: Vec<(usize, char)> = slice.char_indices().map(|(i, c)| (line_start + i, c)).collect();
        let mut i = 0usize;
        while i < chars.len() {
            let (pos, c) = chars[i];
            match c {
                '`' => {
                    tokens.push(Token { kind: TokenKind::CodeInline, range: pos..pos + 1, line });
                    i += 1;
                }
                '*' | '_' => {
                    if i + 1 < chars.len() && chars[i + 1].1 == c {
                        let end = chars[i + 1].0 + 1;
                        tokens.push(Token { kind: TokenKind::Bold, range: pos..end, line });
                        i += 2;
                    } else {
                        tokens.push(Token { kind: TokenKind::Italic, range: pos..pos + 1, line });
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }
    }

    fn highlight_sql(&self, rope: &Rope) -> Vec<Token> {
        let content = rope.to_string();
        let keywords = [
            "select",
            "from",
            "where",
            "insert",
            "into",
            "values",
            "update",
            "set",
            "delete",
            "create",
            "table",
            "alter",
            "drop",
            "join",
            "inner",
            "outer",
            "left",
            "right",
            "union",
            "group",
            "order",
            "by",
            "having",
            "and",
            "or",
            "not",
            "null",
            "true",
            "false",
            "as",
            "on",
            "in",
            "between",
            "like",
            "is",
            "primary",
            "key",
            "foreign",
            "references",
            "index",
            "view",
            "database",
            "schema",
            "commit",
            "rollback",
            "begin",
            "transaction",
            "case",
            "when",
            "then",
            "else",
            "end",
            "if",
            "exists",
        ];

        self.tokenize_with_keywords(rope, &keywords, &content, TokenKind::Keyword, Vec::new())
    }

    fn highlight_json(&self, rope: &Rope) -> Vec<Token> {
        let mut tokens = Vec::new();
        let content = rope.to_string();

        for (i, ch) in content.char_indices() {
            let token = match ch {
                '"' => {
                    // Check if this is a key (followed by colon)
                    let mut j = i + 1;
                    while j < content.len() && content.as_bytes()[j] != b'"' {
                        j += 1;
                    }
                    j += 1; // skip closing quote
                    while j < content.len() && content.as_bytes()[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < content.len() && content.as_bytes()[j] == b':' {
                        TokenKind::JsonKey
                    } else {
                        TokenKind::JsonString
                    }
                }
                '0'..='9' | '-' => TokenKind::JsonNumber,
                // `starts_with` no requiere que el límite caiga en frontera de carácter (a
                // diferencia de indexar `&content[i..i+N]`, que podía entrar en pánico con
                // UTF-8 multibyte justo después de la 't'/'f'/'n' — hallazgo G).
                't' if content[i..].starts_with("true") => TokenKind::JsonBoolean,
                'f' if content[i..].starts_with("false") => TokenKind::JsonBoolean,
                'n' if content[i..].starts_with("null") => TokenKind::JsonNull,
                _ => continue,
            };

            let end = i + ch.len_utf8();
            tokens.push(Token {
                kind: token,
                range: i..end,
                line: content[..i].matches('\n').count(),
            });
        }

        tokens
    }

    fn highlight_html(&self, rope: &Rope) -> Vec<Token> {
        let content = rope.to_string();
        let tags = [
            "div", "span", "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "a", "img",
            "table", "tr", "td", "th", "html", "head", "body", "title", "meta", "link", "script",
            "style", "form", "input", "label", "button",
        ];

        let mut tokens = Vec::new();
        self.tokenize_html(rope, &content, &tags, &mut tokens);
        tokens
    }

    fn highlight_xml(&self, rope: &Rope) -> Vec<Token> {
        let content = rope.to_string();
        let mut tokens = Vec::new();
        self.tokenize_html(rope, &content, &[], &mut tokens);
        tokens
    }

    fn highlight_js(&self, rope: &Rope) -> Vec<Token> {
        let keywords = [
            "const",
            "let",
            "var",
            "function",
            "return",
            "if",
            "else",
            "for",
            "while",
            "do",
            "break",
            "continue",
            "switch",
            "case",
            "default",
            "try",
            "catch",
            "finally",
            "throw",
            "new",
            "this",
            "class",
            "extends",
            "import",
            "export",
            "default",
            "async",
            "await",
            "true",
            "false",
            "null",
            "undefined",
        ];

        self.tokenize_with_keywords(
            rope,
            &keywords,
            &rope.to_string(),
            TokenKind::Keyword,
            Vec::new(),
        )
    }

    fn tokenize_with_keywords(
        &self,
        rope: &Rope,
        keywords: &[&str],
        content: &str,
        _keyword_kind: TokenKind,
        mut tokens: Vec<Token>,
    ) -> Vec<Token> {
        let bytes = content.as_bytes();
        let mut i = 0usize;
        let mut line = 0usize;

        while i < bytes.len() {
            let ch = bytes[i];
            let char_start = i;

            if ch.is_ascii_whitespace() {
                if ch == b'\n' {
                    line += 1;
                }
                i += 1;
                continue;
            }

            if ch == b'"' || ch == b'\'' {
                // String literal
                let quote = ch;
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // skip closing quote
                }
                tokens.push(Token {
                    kind: TokenKind::StringLiteral,
                    range: char_start..i,
                    line,
                });
                continue;
            }

            if ch == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                // SQL comment
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Comment,
                    range: char_start..i,
                    line,
                });
                continue;
            }

            if ch.is_ascii_alphanumeric() || ch == b'_' {
                let word_start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = content[word_start..i].to_lowercase();

                let kind = if keywords.contains(&word.as_str()) {
                    TokenKind::Keyword
                } else if content[word_start..i].chars().all(|c| c.is_ascii_digit()) {
                    TokenKind::Number
                } else {
                    TokenKind::Text
                };

                tokens.push(Token {
                    kind,
                    range: word_start..i,
                    line,
                });
                continue;
            }

            // Operadores y puntuación (ASCII): un byte, un carácter, avance seguro de 1.
            if ch.is_ascii() {
                tokens.push(Token {
                    kind: match ch {
                        b'=' | b'<' | b'>' | b'!' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|'
                        | b'^' | b'~' => TokenKind::Operator,
                        b'(' | b')' | b'{' | b'}' | b'[' | b']' | b';' | b':' | b',' | b'.' => {
                            TokenKind::Punctuation
                        }
                        _ => TokenKind::Text,
                    },
                    range: char_start..i + 1,
                    line,
                });
                i += 1;
                continue;
            }
            // Carácter no ASCII (letras acentuadas, emoji...): se avanza el carácter Unicode
            // completo para que el rango del token nunca caiga en mitad de una secuencia
            // UTF-8 multibyte (hallazgo G: pánico con "café", "1é", emoji...).
            let clen = content[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            tokens.push(Token {
                kind: TokenKind::Text,
                range: char_start..char_start + clen,
                line,
            });
            i += clen;
        }

        // Also tokenize rope lines for line info
        let _ = rope;
        tokens
    }

    fn tokenize_html(&self, rope: &Rope, content: &str, _tags: &[&str], tokens: &mut Vec<Token>) {
        let bytes = content.as_bytes();
        let mut i = 0usize;
        let mut line = 0usize;

        while i < bytes.len() {
            let ch = bytes[i];
            let char_start = i;

            if ch == b'\n' {
                line += 1;
                i += 1;
                continue;
            }

            if ch == b'<' {
                i += 1;
                // Check if it's a closing tag or comment
                if i < bytes.len() && bytes[i] == b'!' {
                    while i < bytes.len() && bytes[i] != b'>' {
                        if bytes[i] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                    tokens.push(Token {
                        kind: TokenKind::Comment,
                        range: char_start..i,
                        line,
                    });
                    continue;
                }

                // Read tag name
                let tag_start = i;
                while i < bytes.len() && bytes[i] != b'>' && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let _tag_name = content[tag_start..i].to_lowercase();

                // Read to end of tag
                while i < bytes.len() && bytes[i] != b'>' {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // skip '>'
                }

                tokens.push(Token {
                    kind: TokenKind::HtmlTag,
                    range: char_start..i,
                    line,
                });
                continue;
            }

            // Text content
            let text_start = i;
            while i < bytes.len() && bytes[i] != b'<' && bytes[i] != b'\n' {
                i += 1;
            }
            if i > text_start {
                tokens.push(Token {
                    kind: TokenKind::Text,
                    range: text_start..i,
                    line,
                });
            }
        }

        let _ = rope;
    }
}

pub fn detect_language_for_file(path: &std::path::Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("markdown") => "markdown",
        Some("html") | Some("htm") => "html",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        Some("sql") => "sql",
        Some("json") => "json",
        Some("xml") => "xml",
        Some("txt") => "plaintext",
        _ => "plaintext",
    }
    .to_string()
}

pub fn render_syntax_view(content: &str, lang: &str) -> String {
    let rope = Rope::from_str(content);
    let highlighter = Highlighter::new(lang);
    let tokens = highlighter.highlight(&rope);

    let mut result = String::new();
    result.push_str("═══════════════════════════════════════════════════════════════\n");
    result.push_str(&format!(
        " 🎨 ANÁLISIS DE SINTAXIS | Lenguaje: {} | Tokens: {}\n",
        lang.to_uppercase(),
        tokens.len()
    ));
    result.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        result.push_str("(Documento vacío)\n");
        return result;
    }

    // Group tokens by line
    for (line_idx, line_text) in lines.iter().enumerate().take(300) {
        let line_num = line_idx + 1;
        result.push_str(&format!("{:3} │ ", line_num));

        let line_tokens: Vec<&Token> = tokens.iter().filter(|t| t.line == line_idx).collect();
        if line_tokens.is_empty() {
            result.push_str(line_text);
        } else {
            let mut sorted = line_tokens.clone();
            sorted.sort_by_key(|t| t.range.start);

            for t in sorted {
                let badge = match t.kind {
                    TokenKind::Heading1 => "[H1]",
                    TokenKind::Heading2 => "[H2]",
                    TokenKind::Heading3 => "[H3]",
                    TokenKind::Heading4 => "[H4]",
                    TokenKind::Heading5 => "[H5]",
                    TokenKind::Heading6 => "[H6]",
                    TokenKind::Bold => "[BOLD]",
                    TokenKind::Italic => "[ITALIC]",
                    TokenKind::CodeInline => "[CODE]",
                    TokenKind::CodeBlock => "[CODEBLOCK]",
                    TokenKind::Link | TokenKind::Url => "[URL]",
                    TokenKind::Quote => "[QUOTE]",
                    TokenKind::ListMarker => "[LIST]",
                    TokenKind::Comment => "[COMMENT]",
                    TokenKind::Tag | TokenKind::HtmlTag => "[TAG]",
                    TokenKind::Attribute => "[ATTR]",
                    TokenKind::StringLiteral | TokenKind::JsonString => "[STRING]",
                    TokenKind::Number | TokenKind::JsonNumber => "[NUM]",
                    TokenKind::Keyword => "[KEYWORD]",
                    TokenKind::Operator => "[OP]",
                    TokenKind::JsonKey => "[KEY]",
                    TokenKind::JsonBoolean => "[BOOL]",
                    TokenKind::JsonNull => "[NULL]",
                    _ => "",
                };
                let token_slice = if t.range.start < content.len()
                    && t.range.end <= content.len()
                    && content.is_char_boundary(t.range.start)
                    && content.is_char_boundary(t.range.end)
                {
                    &content[t.range.clone()]
                } else {
                    ""
                };
                let token_text = token_slice.trim();
                if !badge.is_empty() && !token_text.is_empty() {
                    result.push_str(&format!("{} {} ", badge, token_text));
                } else if !token_text.is_empty() {
                    result.push_str(token_text);
                    result.push(' ');
                }
            }
        }
        result.push('\n');
    }

    if lines.len() > 300 {
        result.push_str(&format!("\n... ({} líneas más)\n", lines.len() - 300));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_syntax_view_does_not_panic_on_multibyte_markdown() {
        let out = render_syntax_view("función *café* y t😀 texto", "markdown");
        assert!(out.contains("función") || !out.is_empty());
    }

    #[test]
    fn render_syntax_view_does_not_panic_on_multibyte_sql() {
        let out = render_syntax_view("SELECT 1é, t😀col FROM tablañ", "sql");
        assert!(!out.is_empty());
    }

    #[test]
    fn render_syntax_view_does_not_panic_on_multibyte_javascript() {
        let out = render_syntax_view("const x = \"1é\" + t😀; // café", "javascript");
        assert!(!out.is_empty());
    }

    #[test]
    fn render_syntax_view_does_not_panic_on_crlf() {
        let out = render_syntax_view("SELECT 1\r\nFROM t\r\n", "sql");
        assert!(!out.is_empty());
    }

    #[test]
    fn highlight_markdown_bold_and_italic_tokens_on_multibyte_line() {
        let highlighter = Highlighter::new("markdown");
        let rope = Rope::from_str("café **t😀ext** más *é*");
        // No debe entrar en pánico, y debe producir al menos un token Bold e Italic.
        let tokens = highlighter.highlight(&rope);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Bold));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Italic));
    }

    #[test]
    fn highlight_json_true_false_null_near_multibyte_does_not_panic() {
        let highlighter = Highlighter::new("json");
        let rope = Rope::from_str(r#"{"a": true, "b": "é", "c": null}"#);
        let _ = highlighter.highlight(&rope);
    }
}

#![allow(
    dead_code,
    unused_assignments,
    unused_mut,
    clippy::single_char_add_str,
    clippy::if_same_then_else
)]

#[derive(Clone, Copy)]
pub enum FormatLanguage {
    Json,
    Xml,
    Html,
    Sql,
    Markdown,
    JavaScript,
}

impl FormatLanguage {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "html" | "htm" => Some(Self::Html),
            "sql" => Some(Self::Sql),
            "md" | "markdown" => Some(Self::Markdown),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "txt" => None,
            _ => None,
        }
    }
}

pub fn format_text(content: &str, language: FormatLanguage) -> Result<String, FormatError> {
    match language {
        FormatLanguage::Json => format_json(content),
        FormatLanguage::Xml => format_xml(content),
        FormatLanguage::Html => format_html(content),
        FormatLanguage::Sql => format_sql(content),
        FormatLanguage::Markdown => Ok(content.to_string()),
        FormatLanguage::JavaScript => Ok(content.to_string()),
    }
}

#[derive(Debug)]
pub enum FormatError {
    ParseError(String),
    SyntaxError(String),
}

pub fn format_json(content: &str) -> Result<String, FormatError> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| FormatError::ParseError(format!("JSON parse error: {}", e)))?;

    serde_json::to_string_pretty(&value)
        .map(|s| s + "\n")
        .map_err(|e| FormatError::SyntaxError(format!("JSON serialize error: {}", e)))
}

pub fn format_xml(content: &str) -> Result<String, FormatError> {
    let mut result = String::new();
    let tokens = tokenize_xml(content)?;

    let mut indent_level = 0usize;
    let mut last_token_was_text = false;

    for token in &tokens {
        match token {
            XmlToken::Open(name) => {
                if last_token_was_text {
                    result.push('\n');
                    indent_level = indent_level.saturating_sub(1);
                    last_token_was_text = false;
                }

                if !result.ends_with('\n') && !result.is_empty() {
                    result.push('\n');
                }
                push_indent(&mut result, indent_level);
                result.push_str("<");
                result.push_str(name);
                indent_level += 1;
                last_token_was_text = false;
            }
            XmlToken::SelfClosing(name) => {
                if last_token_was_text {
                    result.push('\n');
                    indent_level = indent_level.saturating_sub(1);
                    last_token_was_text = false;
                }

                if !result.ends_with('\n') && !result.is_empty() {
                    result.push('\n');
                }
                push_indent(&mut result, indent_level);
                result.push_str("<");
                result.push_str(name);
                result.push_str("/>");
                last_token_was_text = false;
            }
            XmlToken::Close(name) => {
                indent_level = indent_level.saturating_sub(1);
                if last_token_was_text {
                    result.push_str("</");
                    result.push_str(name);
                    result.push_str(">");
                    last_token_was_text = false;
                } else {
                    if !result.ends_with('\n') && !result.is_empty() {
                        result.push('\n');
                    }
                    push_indent(&mut result, indent_level);
                    result.push_str("</");
                    result.push_str(name);
                    result.push_str(">");
                    last_token_was_text = false;
                }
            }
            XmlToken::Text(text) => {
                if text.starts_with('\n') || result.is_empty() {
                    result.push_str(text);
                } else {
                    result.push_str(text);
                }
                last_token_was_text = true;
            }
        }
    }

    if !result.ends_with('\n') {
        result.push('\n');
    }

    Ok(result)
}

enum XmlToken {
    Open(String),
    SelfClosing(String),
    Close(String),
    Text(String),
}

fn tokenize_xml(content: &str) -> Result<Vec<XmlToken>, FormatError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '<' {
            // Parse tag
            if i + 1 < chars.len() && chars[i + 1] == '/' {
                // Closing tag
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '>' {
                    j += 1;
                }
                let name: String = chars[i + 2..j].iter().collect();
                tokens.push(XmlToken::Close(name.trim().to_string()));
                i = j + 1;
            } else {
                // Opening or self-closing tag
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '>' && chars[j] != '/' {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                let name = name.trim().to_string();

                if j < chars.len() && chars[j] == '/' {
                    // Self-closing
                    tokens.push(XmlToken::SelfClosing(name));
                    i = j + 2; // skip />
                } else {
                    // Opening
                    tokens.push(XmlToken::Open(name));
                    i = j + 1; // skip >
                }
            }
        } else {
            // Text content
            let mut j = i;
            while j < chars.len() && chars[j] != '<' {
                j += 1;
            }
            let text: String = chars[i..j].iter().collect();
            if !text.trim().is_empty() {
                tokens.push(XmlToken::Text(text));
            }
            i = j;
        }
    }

    Ok(tokens)
}

fn push_indent(result: &mut String, level: usize) {
    for _ in 0..level {
        result.push_str("  ");
    }
}

pub fn format_html(content: &str) -> Result<String, FormatError> {
    format_xml(content)
}

pub fn format_sql(content: &str) -> Result<String, FormatError> {
    let keywords = [
        "SELECT",
        "FROM",
        "WHERE",
        "INSERT",
        "INTO",
        "VALUES",
        "UPDATE",
        "SET",
        "DELETE",
        "CREATE",
        "TABLE",
        "ALTER",
        "DROP",
        "JOIN",
        "INNER",
        "OUTER",
        "LEFT",
        "RIGHT",
        "UNION",
        "GROUP",
        "ORDER",
        "BY",
        "HAVING",
        "AND",
        "OR",
        "NOT",
        "NULL",
        "AS",
        "ON",
        "IN",
        "BETWEEN",
        "LIKE",
        "IS",
        "PRIMARY",
        "KEY",
        "FOREIGN",
        "REFERENCES",
        "INDEX",
        "VIEW",
        "DATABASE",
        "SCHEMA",
        "COMMIT",
        "ROLLBACK",
        "BEGIN",
        "TRANSACTION",
        "CASE",
        "WHEN",
        "THEN",
        "ELSE",
        "END",
        "IF",
        "EXISTS",
    ];

    let upper_kw: Vec<&str> = keywords.to_vec();
    let lower_kw: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();

    let mut result = String::new();
    let mut indent_level = 0usize;

    // Simple SQL formatter
    let mut tokens = tokenize_sql(content);

    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let upper = token.to_uppercase();
        let is_upper_kw = upper_kw.contains(&upper.as_str());
        let is_lower_kw = lower_kw.iter().any(|k| k == &token.to_lowercase());

        // Keywords that start a new clause
        let clause_keywords = [
            "SELECT", "FROM", "WHERE", "GROUP", "ORDER", "HAVING", "AND", "OR",
        ];
        let new_clause_kw = [
            "INSERT", "UPDATE", "DELETE", "CREATE", "ALTER", "DROP", "UNION", "CASE",
        ];

        if is_upper_kw || is_lower_kw {
            if i > 0
                && (clause_keywords.contains(&upper.as_str())
                    || new_clause_kw.contains(&upper.as_str()))
            {
                result.push('\n');
                push_indent(&mut result, indent_level);
            }

            // Uppercase keywords
            if is_lower_kw {
                result.push_str(&upper);
            } else {
                result.push_str(token);
            }

            // Handle comma after keyword content
            if i + 1 < tokens.len() && tokens[i + 1] == "," {
                result.push(',');
                i += 2;
                continue;
            }

            result.push(' ');
        } else if token == "," {
            result.push_str(",\n");
            push_indent(&mut result, indent_level + 1);
        } else if token == "(" {
            indent_level += 1;
            result.push_str("(\n");
            push_indent(&mut result, indent_level);
        } else if token == ")" {
            indent_level = indent_level.saturating_sub(1);
            result.push('\n');
            push_indent(&mut result, indent_level);
            result.push(')');
        } else {
            result.push_str(token);
            if i + 1 < tokens.len()
                && tokens[i + 1] != ","
                && tokens[i + 1] != "("
                && tokens[i + 1] != ")"
            {
                result.push(' ');
            }
        }

        i += 1;
    }

    result.push('\n');
    Ok(result)
}

fn tokenize_sql(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];

        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        if ch == '\'' || ch == '"' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1;
            }
            let lit: String = chars[start..i].iter().collect();
            tokens.push(lit);
        } else if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            tokens.push(word);
        } else if ch == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            // Line comment
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            tokens.push("\n".to_string());
        } else {
            tokens.push(ch.to_string());
            i += 1;
        }
    }

    tokens
}

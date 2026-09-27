#![allow(dead_code)]

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

/// Formatea JSON preservando el orden de las claves y la precisión de los números
/// (hallazgo B: con `serde_json` features `preserve_order` + `arbitrary_precision` activadas
/// en `Cargo.toml`, `Value` no reordena objetos ni cambia `12345678901234567890123` o `1e2`).
/// Rechaza objetos con claves duplicadas (hallazgo L13): con `preserve_order`, `Value` las
/// fusiona en silencio quedándose con la última y perdiendo el resto sin avisar; aquí se
/// detectan ANTES de reformatear y se devuelve un error sin tocar el texto original.
pub fn format_json(content: &str) -> Result<String, FormatError> {
    if let Some(key) = find_duplicate_json_key(content) {
        return Err(FormatError::SyntaxError(format!(
            "JSON con clave duplicada: \"{}\"",
            key
        )));
    }

    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| FormatError::ParseError(format!("JSON parse error: {}", e)))?;

    serde_json::to_string_pretty(&value)
        .map(|s| s + "\n")
        .map_err(|e| FormatError::SyntaxError(format!("JSON serialize error: {}", e)))
}

/// Extremo (exclusivo) y valor decodificado de un literal de cadena JSON `"..."` que empieza
/// en `chars[start]` (la comilla de apertura). Decodifica los escapes estándar (`\"`, `\\`,
/// `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX`) para que dos claves que difieren solo en su
/// forma de escape (p.ej. `"a"` y `"a"`) se reconozcan como la misma clave. Si la cadena
/// no está bien formada, devuelve `None` (no debería ocurrir: solo se llama sobre JSON que ya
/// pasó `serde_json::from_str`, así que las cadenas están garantizadas bien formadas).
fn read_json_string(chars: &[char], start: usize) -> Option<(String, usize)> {
    let n = chars.len();
    let mut j = start + 1;
    let mut out = String::new();
    while j < n {
        match chars[j] {
            '"' => return Some((out, j + 1)),
            '\\' if j + 1 < n => {
                match chars[j + 1] {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' if j + 5 < n => {
                        let hex: String = chars[j + 2..j + 6].iter().collect();
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            // No intenta recomponer pares suplentes UTF-16: para detectar
                            // duplicados basta con una representación estable, y
                            // `char::from_u32` puede fallar con un suplente aislado (se usa el
                            // carácter de reemplazo en ese caso, igual para ambas ocurrencias).
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        j += 4;
                    }
                    other => out.push(other),
                }
                j += 2;
                continue;
            }
            c => {
                out.push(c);
                j += 1;
            }
        }
    }
    None
}

/// Escanea el texto JSON en busca de un objeto con una clave repetida en el MISMO nivel
/// (hallazgo L13), devolviendo la primera clave duplicada encontrada. Solo se invoca sobre
/// texto ya validado como JSON sintácticamente correcto, así que basta con: llevar una pila de
/// conjuntos de claves (uno por cada `{` abierto sin cerrar) y, cada vez que una cadena está
/// seguida (tras espacio opcional) de `:`, es la clave del objeto que está en la cima de la
/// pila — en JSON válido, `:` solo puede aparecer justo después de una clave de objeto, nunca
/// tras un valor ni dentro de un array.
fn find_duplicate_json_key(content: &str) -> Option<String> {
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len();
    let mut stack: Vec<std::collections::HashSet<String>> = Vec::new();
    let mut i = 0usize;
    while i < n {
        match chars[i] {
            '"' => {
                let (s, end) = read_json_string(&chars, i)?;
                i = end;
                let mut j = i;
                while j < n && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < n && chars[j] == ':' {
                    if let Some(top) = stack.last_mut() {
                        if !top.insert(s.clone()) {
                            return Some(s);
                        }
                    }
                }
            }
            '{' => {
                stack.push(std::collections::HashSet::new());
                i += 1;
            }
            '}' => {
                stack.pop();
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// XML / HTML — tokenizador correcto (hallazgo B)
// ---------------------------------------------------------------------------
//
// El formateador anterior perdía `>`, cortaba tags en `/` dentro de atributos, rompía
// comentarios/CDATA/void elements y no dejaba verbatim el cuerpo de `<script>`/`<style>`/
// `<pre>`/`<textarea>`. Este tokenizador captura cada construcción TAL CUAL aparece en el
// origen (incluidas comillas con `/` y `>`, comentarios, `<?...?>`, `<!DOCTYPE ...>`,
// `<![CDATA[...]]>` y los void elements de HTML) y el formateador solo decide DÓNDE insertar
// saltos de línea e indentación entre esos fragmentos verbatim — nunca los reescribe.

const HTML_VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];

const HTML_RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "pre"];

#[derive(Debug, Clone)]
enum MarkupToken {
    /// Texto entre construcciones (puede ser solo espacio, o contenido real).
    Text(String),
    /// Tag de apertura (o self-closing / void), texto verbatim incluidas comillas/atributos.
    Open { text: String, self_closing: bool },
    /// Tag de cierre `</nombre>`, texto verbatim.
    Close { text: String },
    /// Comentario, `<?...?>`, `<!DOCTYPE...>` o `<![CDATA[...]]>`: verbatim, sin reindentar
    /// su interior, se coloca en su propia línea.
    Verbatim(String),
    /// `<script>`/`<style>`/`<pre>`/`<textarea>`: apertura + cuerpo + cierre, todo verbatim y
    /// pegado (el cuerpo nunca se reindenta: cambiaría el JS/CSS/texto contenido).
    RawBlock { open: String, body: String, close: String },
}

fn find_unquoted_gt(s: &str, start: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = start;
    let mut in_quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    in_quote = Some(c);
                } else if c == b'>' {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Busca `needle` (ascii, en minúsculas) en `haystack` sin distinguir mayúsculas, exigiendo que
/// el carácter siguiente a la coincidencia no sea alfanumérico (para no confundir
/// `</script>` con `</scriptx>`).
fn find_ci_word(haystack: &str, needle_lower: &str) -> Option<usize> {
    let hb = haystack.as_bytes();
    let nb = needle_lower.as_bytes();
    if nb.is_empty() || hb.len() < nb.len() {
        return None;
    }
    'outer: for i in 0..=(hb.len() - nb.len()) {
        for j in 0..nb.len() {
            if hb[i + j].to_ascii_lowercase() != nb[j] {
                continue 'outer;
            }
        }
        let next_ok = hb
            .get(i + nb.len())
            .map(|c| !c.is_ascii_alphanumeric())
            .unwrap_or(true);
        if next_ok {
            return Some(i);
        }
    }
    None
}

fn extract_tag_name(inner: &str) -> String {
    inner
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
        .collect()
}

fn tokenize_markup(content: &str) -> Vec<MarkupToken> {
    let mut tokens = Vec::new();
    let mut rest = content;

    while !rest.is_empty() {
        match rest.find('<') {
            Some(0) => {}
            Some(lt) => {
                tokens.push(MarkupToken::Text(rest[..lt].to_string()));
                rest = &rest[lt..];
            }
            None => {
                tokens.push(MarkupToken::Text(rest.to_string()));
                break;
            }
        }

        if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|i| i + 3).unwrap_or(rest.len());
            tokens.push(MarkupToken::Verbatim(rest[..end].to_string()));
            rest = &rest[end..];
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").map(|i| i + 3).unwrap_or(rest.len());
            tokens.push(MarkupToken::Verbatim(rest[..end].to_string()));
            rest = &rest[end..];
        } else if rest.starts_with("<?") {
            let end = rest
                .find("?>")
                .map(|i| i + 2)
                .or_else(|| find_unquoted_gt(rest, 2).map(|i| i + 1))
                .unwrap_or(rest.len());
            tokens.push(MarkupToken::Verbatim(rest[..end].to_string()));
            rest = &rest[end..];
        } else if rest.starts_with("<!") {
            let end = find_unquoted_gt(rest, 2).map(|i| i + 1).unwrap_or(rest.len());
            tokens.push(MarkupToken::Verbatim(rest[..end].to_string()));
            rest = &rest[end..];
        } else if rest.starts_with("</") {
            let end = find_unquoted_gt(rest, 2).map(|i| i + 1).unwrap_or(rest.len());
            tokens.push(MarkupToken::Close { text: rest[..end].to_string() });
            rest = &rest[end..];
        } else {
            // Open, self-closing o void.
            let end = find_unquoted_gt(rest, 1).map(|i| i + 1).unwrap_or(rest.len());
            let tag_text = &rest[..end];
            let inner_end = end.saturating_sub(1).max(1);
            let name = extract_tag_name(&rest[1..inner_end]);
            let self_closing_syntax = tag_text[..tag_text.len().saturating_sub(1)]
                .trim_end()
                .ends_with('/');
            let lname = name.to_lowercase();
            let is_void = HTML_VOID_ELEMENTS.contains(&lname.as_str());

            if !self_closing_syntax && HTML_RAW_TEXT_ELEMENTS.contains(&lname.as_str()) {
                let after_open = &rest[end..];
                let close_needle = format!("</{}", lname);
                match find_ci_word(after_open, &close_needle) {
                    Some(pos) => {
                        let close_end = find_unquoted_gt(&after_open[pos..], 0)
                            .map(|i| pos + i + 1)
                            .unwrap_or(after_open.len());
                        tokens.push(MarkupToken::RawBlock {
                            open: tag_text.to_string(),
                            body: after_open[..pos].to_string(),
                            close: after_open[pos..close_end].to_string(),
                        });
                        rest = &after_open[close_end..];
                    }
                    None => {
                        tokens.push(MarkupToken::RawBlock {
                            open: tag_text.to_string(),
                            body: after_open.to_string(),
                            close: String::new(),
                        });
                        rest = "";
                    }
                }
            } else {
                tokens.push(MarkupToken::Open {
                    text: tag_text.to_string(),
                    self_closing: self_closing_syntax || is_void,
                });
                rest = &rest[end..];
            }
        }
    }

    tokens
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn push_indent(result: &mut String, level: usize) {
    for _ in 0..level {
        result.push_str("  ");
    }
}

fn ensure_newline(result: &mut String) {
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
}

/// Árbol de construcciones markup (hallazgo C22): a diferencia de la lista plana de
/// `MarkupToken` (que no sabe qué `Open` empareja con qué `Close`), este árbol agrupa cada
/// elemento con sus hijos DIRECTOS, lo necesario para detectar "contenido mixto" (un
/// elemento que mezcla texto no-blanco con elementos hijos al mismo nivel) y decidir, para
/// ESE elemento, si se reformatea recursivamente o se emite verbatim.
#[derive(Debug, Clone)]
enum MarkupNode {
    Text(String),
    Verbatim(String),
    RawBlock { open: String, body: String, close: String },
    Element { open: String, self_closing: bool, children: Vec<MarkupNode>, close: Option<String> },
    /// Un `Close` sin `Open` correspondiente por delante (markup malformado): se conserva
    /// para no perder texto, tratado por el renderizador igual que un `Close` normal.
    StrayClose(String),
}

/// Agrupa la lista plana de `tokens` (a partir de `*pos`) en hijos directos, hasta el
/// primer `Close` (si `in_element`, ese `Close` pertenece al elemento que llamó y se
/// devuelve por separado) o hasta el final de la lista. Empareja por posición (LIFO), no
/// por nombre de etiqueta — igual que el contador plano de indentación original: CUALQUIER
/// `Close` cierra el `Open` abierto más reciente, sin comprobar que los nombres coincidan
/// (el formateador nunca ha validado balanceo de nombres, solo estructura).
fn parse_markup_children(tokens: &[MarkupToken], pos: &mut usize, in_element: bool) -> (Vec<MarkupNode>, Option<String>) {
    let mut children = Vec::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            MarkupToken::Close { text } => {
                let text = text.clone();
                *pos += 1;
                if in_element {
                    return (children, Some(text));
                }
                children.push(MarkupNode::StrayClose(text));
            }
            MarkupToken::Open { text, self_closing } => {
                let text = text.clone();
                let self_closing = *self_closing;
                *pos += 1;
                if self_closing {
                    children.push(MarkupNode::Element { open: text, self_closing: true, children: Vec::new(), close: None });
                } else {
                    let (sub_children, close) = parse_markup_children(tokens, pos, true);
                    children.push(MarkupNode::Element { open: text, self_closing: false, children: sub_children, close });
                }
            }
            MarkupToken::Text(t) => {
                children.push(MarkupNode::Text(t.clone()));
                *pos += 1;
            }
            MarkupToken::Verbatim(t) => {
                children.push(MarkupNode::Verbatim(t.clone()));
                *pos += 1;
            }
            MarkupToken::RawBlock { open, body, close } => {
                children.push(MarkupNode::RawBlock { open: open.clone(), body: body.clone(), close: close.clone() });
                *pos += 1;
            }
        }
    }
    (children, None)
}

/// Hallazgo C22: un elemento tiene "contenido mixto" si mezcla, entre sus hijos DIRECTOS,
/// texto no-blanco Y al menos un elemento (o bloque verbatim tipo `<script>`/`<pre>`).
/// Reformatear ese elemento insertando saltos de línea entre construcciones que en el
/// original estaban pegadas cambia el ESPACIADO RENDERIZADO real: dos elementos inline
/// pegados sin espacio entre ellos (p.ej. `<b>x</b><i>y</i>`) se renderizan pegados; con un
/// salto de línea insertado entre ellos, cualquier visor HTML lo colapsa a un espacio
/// visible entre "x" e "y" que no existía en el original.
fn markup_has_mixed_content(children: &[MarkupNode]) -> bool {
    let has_nonblank_text = children.iter().any(|c| matches!(c, MarkupNode::Text(t) if !is_blank(t)));
    let has_element_child = children
        .iter()
        .any(|c| matches!(c, MarkupNode::Element { .. } | MarkupNode::RawBlock { .. }));
    has_nonblank_text && has_element_child
}

/// Reconstruye el texto EXACTO original de `node` (concatenando las porciones verbatim que
/// ya guarda cada token: la tokenización nunca pierde ni modifica caracteres). Usado para
/// emitir un elemento de contenido mixto (hallazgo C22) sin reformatear su interior.
fn markup_raw(node: &MarkupNode) -> String {
    match node {
        MarkupNode::Text(t) | MarkupNode::Verbatim(t) | MarkupNode::StrayClose(t) => t.clone(),
        MarkupNode::RawBlock { open, body, close } => format!("{open}{body}{close}"),
        MarkupNode::Element { open, self_closing, children, close } => {
            let mut s = open.clone();
            if !self_closing {
                for c in children {
                    s.push_str(&markup_raw(c));
                }
                if let Some(c) = close {
                    s.push_str(c);
                }
            }
            s
        }
    }
}

fn render_markup_children(
    children: &[MarkupNode],
    result: &mut String,
    indent_level: &mut usize,
    pending_inline_text: &mut Option<String>,
) {
    for node in children {
        render_markup_node(node, result, indent_level, pending_inline_text);
    }
}

fn render_markup_node(
    node: &MarkupNode,
    result: &mut String,
    indent_level: &mut usize,
    pending_inline_text: &mut Option<String>,
) {
    match node {
        MarkupNode::Text(t) => {
            if is_blank(t) {
                // Espacio puramente estructural: no aporta nada, el formateador decide
                // los saltos de línea/indentación por su cuenta.
                return;
            }
            // Texto real: se pega justo después de la construcción anterior (estilo
            // "<b>Bold</b>"). Conserva un espacio simple donde el original tenía espacio
            // en blanco inicial/final (colapsado a uno), para no pegar palabras que en el
            // origen estaban separadas por un salto de línea u otro espacio estructural
            // (hallazgo L6: "Hello <b>world</b> again" no debe perder el espacio antes de
            // "again", y "<v> x  y </v>" no debe cambiar el valor de texto).
            let leading_ws = t.starts_with(|c: char| c.is_whitespace());
            let trailing_ws = t.ends_with(|c: char| c.is_whitespace());
            if leading_ws && !result.is_empty() {
                result.push(' ');
            }
            result.push_str(t.trim());
            if trailing_ws {
                result.push(' ');
            }
            *pending_inline_text = Some(t.clone());
        }
        MarkupNode::Verbatim(text) => {
            ensure_newline(result);
            push_indent(result, *indent_level);
            result.push_str(text);
            *pending_inline_text = None;
        }
        MarkupNode::RawBlock { open, body, close } => {
            ensure_newline(result);
            push_indent(result, *indent_level);
            result.push_str(open);
            result.push_str(body);
            result.push_str(close);
            *pending_inline_text = None;
        }
        MarkupNode::StrayClose(text) => {
            // Igual que el `Close` normal más abajo: el nivel SIEMPRE decrece (hallazgo
            // L1), pegado al texto inline si venía justo después, o en su propia línea.
            *indent_level = indent_level.saturating_sub(1);
            if pending_inline_text.is_some() {
                result.push_str(text);
            } else {
                ensure_newline(result);
                push_indent(result, *indent_level);
                result.push_str(text);
            }
            *pending_inline_text = None;
        }
        MarkupNode::Element { open, self_closing, children, close } => {
            if markup_has_mixed_content(children) {
                // Hallazgo C22: todo el elemento (etiquetas + contenido) se emite
                // verbatim, en su propia línea, sin recursar en sus hijos.
                ensure_newline(result);
                push_indent(result, *indent_level);
                result.push_str(&markup_raw(node));
                *pending_inline_text = None;
                return;
            }

            ensure_newline(result);
            push_indent(result, *indent_level);
            result.push_str(open);
            *pending_inline_text = None;

            if !self_closing {
                *indent_level += 1;
                render_markup_children(children, result, indent_level, pending_inline_text);
                // El nivel de indentación SIEMPRE decrece en un cierre (hallazgo L1): antes,
                // si había texto inline pegado justo antes, el decremento se omitía por
                // completo y el nivel crecía sin límite en documentos con mucho texto
                // inline (con documentos grandes, un `push_indent` de tamaño
                // ~O(profundidad) por línea que degeneraba en uso de memoria
                // catastrófico). Lo único que depende de `pending_inline_text` es si el
                // cierre se pega al texto (sin salto de línea/indentación) o va en su
                // propia línea.
                *indent_level = indent_level.saturating_sub(1);
                if let Some(close_text) = close {
                    if pending_inline_text.is_some() {
                        // Cierre pegado al texto inline: "<b>Bold</b>".
                        result.push_str(close_text);
                    } else {
                        ensure_newline(result);
                        push_indent(result, *indent_level);
                        result.push_str(close_text);
                    }
                }
                *pending_inline_text = None;
            }
        }
    }
}

fn format_markup(content: &str) -> Result<String, FormatError> {
    let tokens = tokenize_markup(content);
    let mut pos = 0usize;
    let (roots, _) = parse_markup_children(&tokens, &mut pos, false);

    let mut result = String::new();
    let mut indent_level = 0usize;
    let mut pending_inline_text: Option<String> = None;
    render_markup_children(&roots, &mut result, &mut indent_level, &mut pending_inline_text);

    if !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

pub fn format_xml(content: &str) -> Result<String, FormatError> {
    format_markup(content)
}

pub fn format_html(content: &str) -> Result<String, FormatError> {
    format_markup(content)
}

// ---------------------------------------------------------------------------
// SQL — formateador CONSERVADOR (hallazgo L2).
// ---------------------------------------------------------------------------
//
// El formateador anterior tokenizaba y reconstruía el SQL insertando espacios entre
// tokens y poniendo las palabras clave en mayúsculas — eso cambia la SEMÁNTICA de
// identificadores/alias unicode ("año" se partía en "a ñ o"), de cadenas y literales
// sensibles a mayúsculas/minúsculas ("N'x'" pasaba a "N 'x'"), y de operadores propios
// de cada motor (`@@ROWCOUNT`, `#temp`, `t.*`, corchetes `[..]]..]`, `$$..$$`, etc.).
//
// DISEÑO OBLIGATORIO (ver R2_LOGIC.md L2): `format_sql` NUNCA inserta espacios, NUNCA
// parte tokens y NUNCA cambia mayúsculas/minúsculas. El único cambio permitido es
// sustituir un tramo de whitespace YA EXISTENTE en el original (fuera de literales de
// cadena, identificadores citados, comentarios y bloques `$tag$...$tag$`) por un salto
// de línea + indentación, justo delante de palabras clave de cláusula, y justo detrás
// de una ',' de nivel superior si ya había espacio después. Como consecuencia directa,
// el invariante `input.split_whitespace() == output.split_whitespace()` se cumple
// siempre (los tokens no-whitespace nunca se tocan; solo se reemplaza whitespace por
// otro whitespace).
const SQL_CLAUSE_KEYWORDS: &[&str] = &[
    "select", "from", "where", "group", "order", "having", "limit", "join", "inner", "outer",
    "left", "right", "full", "cross", "union", "values", "set", "insert", "update", "delete",
    "on",
];

/// Extremo (exclusivo) de la región `'...'` que empieza en `chars[start]` (la comilla de
/// apertura). Soporta el escape estándar `''` y el escape de barra invertida (MySQL) `\'`
/// (hallazgo L2: `'it\'s   spaced'` no debía romperse en dos tokens).
fn sql_span_single_quoted(chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut j = start + 1;
    while j < n {
        match chars[j] {
            '\\' if j + 1 < n => j += 2,
            '\'' => {
                if j + 1 < n && chars[j + 1] == '\'' {
                    j += 2;
                } else {
                    return j + 1;
                }
            }
            _ => j += 1,
        }
    }
    n
}

/// Como `sql_span_single_quoted`, pero SIN el escape de barra invertida (SQL
/// estándar/ANSI, Postgres, SQL Server sin `\`): solo `''` cierra sobre sí misma, un `\`
/// es un carácter literal cualquiera. Usado para detectar el hallazgo C5: en SQL estándar
/// `'C:\temp\'` termina justo después de `temp\` (la comilla SÍ cierra la cadena, el `\`
/// es parte del contenido), pero la convención MySQL de `sql_span_single_quoted`
/// interpretaría ese mismo `\'` como una comilla escapada y seguiría de largo,
/// devorando el siguiente literal (`'x from y'`) y reformateando texto que en realidad
/// ya no está dentro de ninguna cadena.
fn sql_span_single_quoted_standard(chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut j = start + 1;
    while j < n {
        if chars[j] == '\'' {
            if j + 1 < n && chars[j + 1] == '\'' {
                j += 2;
            } else {
                return j + 1;
            }
        } else {
            j += 1;
        }
    }
    n
}

/// Extremo (exclusivo) de un identificador citado `"..."` (doblando `""`) o `` `...` ``
/// (sin escape estándar, pero se admite doblado por seguridad).
fn sql_span_quoted_ident(chars: &[char], start: usize, quote: char) -> usize {
    let n = chars.len();
    let mut j = start + 1;
    while j < n {
        if chars[j] == quote {
            if j + 1 < n && chars[j + 1] == quote {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    n
}

/// Extremo (exclusivo) de un identificador entre corchetes `[..]`, donde `]]` dentro
/// representa un `]` literal (hallazgo L2: `[my]]col]` es un único identificador).
fn sql_span_bracket_ident(chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut j = start + 1;
    while j < n {
        if chars[j] == ']' {
            if j + 1 < n && chars[j + 1] == ']' {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    n
}

/// Extremo (exclusivo) de un comentario de línea `-- ...` (hasta el `\n`, sin incluirlo).
fn sql_span_line_comment(chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut j = start + 2;
    while j < n && chars[j] != '\n' {
        j += 1;
    }
    j
}

/// Extremo (exclusivo) de un comentario de bloque `/* ... */`, soportando anidamiento
/// (hallazgo L2: `/* outer /* inner */ still */`).
fn sql_span_block_comment(chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut j = start + 2;
    let mut depth = 1usize;
    while j < n && depth > 0 {
        if chars[j] == '/' && j + 1 < n && chars[j + 1] == '*' {
            depth += 1;
            j += 2;
        } else if chars[j] == '*' && j + 1 < n && chars[j + 1] == '/' {
            depth -= 1;
            j += 2;
        } else {
            j += 1;
        }
    }
    j
}

/// Si `chars[start]` (un `$`) abre un bloque `$tag$...$tag$` (Postgres, `tag` opcionalmente
/// vacío como en `$$..$$`) y existe su cierre más adelante, devuelve el extremo (exclusivo)
/// del bloque completo. Si no hay cierre, devuelve `None` (entonces el `$` se trata como un
/// carácter normal, p.ej. el parámetro posicional `$1`).
fn sql_span_dollar_quote(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut k = start + 1;
    while k < n && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') {
        k += 1;
    }
    if k >= n || chars[k] != '$' {
        return None;
    }
    let tag: Vec<char> = chars[start..=k].to_vec(); // incluye ambos '$'
    let open_end = k + 1;
    let mut m = open_end;
    while m + tag.len() <= n {
        if chars[m..m + tag.len()] == tag[..] {
            return Some(m + tag.len());
        }
        m += 1;
    }
    None
}

/// Palabra (ASCII o unicode) que comienza exactamente en `pos`, en minúsculas, junto con su
/// extremo (exclusivo). `None` si `pos` no es el inicio de una palabra.
fn sql_peek_word(chars: &[char], pos: usize) -> Option<(String, usize)> {
    let n = chars.len();
    if pos >= n || !(chars[pos].is_alphabetic() || chars[pos] == '_') {
        return None;
    }
    let mut j = pos;
    while j < n && (chars[j].is_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    Some((chars[pos..j].iter().collect::<String>().to_lowercase(), j))
}

/// Formatea SQL de forma conservadora: ver el comentario de diseño más arriba. Por lo
/// general nunca falla (no valida sintaxis, solo reubica whitespace ya existente), salvo
/// el caso del hallazgo C5: si un literal `'...'` contiene un `\` justo antes de una
/// comilla, SQL estándar y la convención de escape de MySQL discrepan sobre dónde termina
/// ese literal — devuelve `Err` en vez de arriesgarse a reformatear dentro de otra cadena.
pub fn format_sql(content: &str) -> Result<String, FormatError> {
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(content.len() + 32);
    let mut paren_depth: usize = 0;
    let mut i = 0usize;

    while i < n {
        let c = chars[i];
        match c {
            '\'' => {
                // Hallazgo C5: si la convención de escape de barra invertida (MySQL) y
                // la de SQL estándar (solo `''`) discrepan sobre dónde termina este
                // literal, no hay forma segura de saber cuál aplica sin conocer el
                // motor de destino — no formatear en vez de arriesgarse a reformatear
                // texto que en realidad sigue dentro de una cadena.
                let end_mysql = sql_span_single_quoted(&chars, i);
                let end_standard = sql_span_single_quoted_standard(&chars, i);
                if end_mysql != end_standard {
                    return Err(FormatError::SyntaxError(
                        "Literal de cadena ambiguo: contiene \\' y SQL estándar/MySQL no coinciden en dónde termina la cadena".to_string(),
                    ));
                }
                out.extend(&chars[i..end_mysql]);
                i = end_mysql;
            }
            '"' | '`' => {
                let end = sql_span_quoted_ident(&chars, i, c);
                out.extend(&chars[i..end]);
                i = end;
            }
            '[' => {
                let end = sql_span_bracket_ident(&chars, i);
                out.extend(&chars[i..end]);
                i = end;
            }
            '-' if i + 1 < n && chars[i + 1] == '-' => {
                let end = sql_span_line_comment(&chars, i);
                out.extend(&chars[i..end]);
                i = end;
            }
            '/' if i + 1 < n && chars[i + 1] == '*' => {
                let end = sql_span_block_comment(&chars, i);
                out.extend(&chars[i..end]);
                i = end;
            }
            '$' if sql_span_dollar_quote(&chars, i).is_some() => {
                let end = sql_span_dollar_quote(&chars, i).unwrap();
                out.extend(&chars[i..end]);
                i = end;
            }
            '(' => {
                out.push('(');
                paren_depth += 1;
                i += 1;
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                out.push(')');
                i += 1;
            }
            ',' => {
                out.push(',');
                i += 1;
                // Solo reformatea si YA hay whitespace tras la coma (nunca inserta donde no
                // había) y estamos en el nivel superior (fuera de listas de argumentos entre
                // paréntesis, para no reformatear `VALUES (1,2)` ni llamadas a función).
                if paren_depth == 0 && i < n && chars[i].is_whitespace() {
                    let ws_start = i;
                    while i < n && chars[i].is_whitespace() {
                        i += 1;
                    }
                    let _ = ws_start;
                    out.push('\n');
                    push_indent(&mut out, paren_depth + 1);
                }
            }
            _ if c.is_whitespace() => {
                let ws_start = i;
                while i < n && chars[i].is_whitespace() {
                    i += 1;
                }
                // ¿La palabra que sigue a este tramo de whitespace es una palabra clave de
                // cláusula? Si es así, sustituye el whitespace EXISTENTE por salto de línea +
                // indentación; si no, se copia el whitespace tal cual (no se toca).
                match sql_peek_word(&chars, i) {
                    Some((word, _)) if SQL_CLAUSE_KEYWORDS.contains(&word.as_str()) && !out.is_empty() => {
                        out.push('\n');
                        push_indent(&mut out, paren_depth);
                    }
                    _ => {
                        out.extend(&chars[ws_start..i]);
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quita todo el espacio en blanco que está estrictamente entre `>` y el siguiente `<`
    /// (espacio puramente inter-tag, el único que el formateador tiene permitido reescribir).
    fn strip_inter_tag_whitespace(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        let mut just_closed_tag = true; // true al inicio: cuenta como "tras un '>'"
        while let Some(c) = chars.next() {
            if just_closed_tag && c.is_whitespace() {
                while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                    chars.next();
                }
                continue;
            }
            out.push(c);
            just_closed_tag = c == '>';
        }
        out
    }

    #[test]
    fn json_preserves_key_order_and_number_precision() {
        let input = r#"{"z": 1, "a": 2, "big": 12345678901234567890123, "sci": 1e2}"#;
        let out = format_json(input).unwrap();
        let iz = out.find("\"z\"").unwrap();
        let ia = out.find("\"a\"").unwrap();
        assert!(iz < ia, "el orden de las claves debe conservarse: {}", out);
        assert!(out.contains("12345678901234567890123"));
        // La notación exponencial puede normalizarse ligeramente (p.ej. "1e2" -> "1e+2"), pero
        // el valor numérico debe seguir siendo exactamente 100 (no se pierde precisión).
        let reparsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(reparsed["sci"].as_f64(), Some(100.0));
    }

    #[test]
    fn json_invalid_returns_parse_error() {
        assert!(format_json("{ not json").is_err());
    }

    #[test]
    fn json_duplicate_top_level_key_is_rejected() {
        let err = format_json(r#"{"a":1,"b":0,"a":2}"#).unwrap_err();
        assert!(matches!(err, FormatError::SyntaxError(_)));
    }

    #[test]
    fn json_duplicate_key_in_nested_object_is_rejected() {
        assert!(format_json(r#"{"outer":{"x":1,"x":2}}"#).is_err());
    }

    #[test]
    fn json_same_key_in_different_objects_is_not_a_duplicate() {
        // Misma clave "x" en dos objetos hermanos distintos (dentro de un array): no es un
        // duplicado, cada objeto tiene su propio ámbito de claves.
        assert!(format_json(r#"[{"x":1},{"x":2}]"#).is_ok());
    }

    #[test]
    fn json_duplicate_key_via_unicode_escape_is_detected() {
        // "a" y "a" decodifican a la misma clave.
        assert!(format_json(r#"{"a":1,"a":2}"#).is_err());
    }

    #[test]
    fn xml_round_trip_simple() {
        let input = "<root><a>1</a><b>2</b></root>";
        let out = format_xml(input).unwrap();
        assert_eq!(
            strip_inter_tag_whitespace(&out),
            strip_inter_tag_whitespace(input)
        );
    }

    #[test]
    fn xml_round_trip_attributes_with_slash_and_gt() {
        let input = r#"<a href="a/b>c" title='x'><img src="1.png"/></a>"#;
        let out = format_xml(input).unwrap();
        assert_eq!(
            strip_inter_tag_whitespace(&out),
            strip_inter_tag_whitespace(input)
        );
    }

    #[test]
    fn xml_round_trip_comment_cdata_pi_doctype() {
        let input = "<!DOCTYPE root><?xml-stylesheet href=\"a.xsl\"?><root><!-- comentario --><![CDATA[<raw> & stuff]]></root>";
        let out = format_xml(input).unwrap();
        assert_eq!(
            strip_inter_tag_whitespace(&out),
            strip_inter_tag_whitespace(input)
        );
        assert!(out.contains("<!-- comentario -->"));
        assert!(out.contains("<![CDATA[<raw> & stuff]]>"));
    }

    #[test]
    fn html_round_trip_void_elements() {
        let input = "<div><br><img src=\"a.png\"><hr></div>";
        let out = format_html(input).unwrap();
        assert_eq!(
            strip_inter_tag_whitespace(&out),
            strip_inter_tag_whitespace(input)
        );
    }

    #[test]
    fn html_script_body_kept_verbatim() {
        let input = "<script>if (a < b) { x(); }\nconsole.log(\"<b>not html</b>\");</script>";
        let out = format_html(input).unwrap();
        assert!(out.contains("if (a < b) { x(); }\nconsole.log(\"<b>not html</b>\");"));
        assert_eq!(
            strip_inter_tag_whitespace(&out),
            strip_inter_tag_whitespace(input)
        );
    }

    #[test]
    fn html_pre_body_not_reindented() {
        let input = "<pre>  line1\n    line2</pre>";
        let out = format_html(input).unwrap();
        assert!(out.contains("  line1\n    line2"));
    }

    #[test]
    fn sql_multi_char_operators_kept_together() {
        let out = format_sql("select a >= 1 and b <> 2 and c || d").unwrap();
        assert!(out.contains(">="));
        assert!(out.contains("<>"));
        assert!(out.contains("||"));
    }

    #[test]
    fn sql_decimal_numbers_preserved() {
        let out = format_sql("select 3.14, 42").unwrap();
        assert!(out.contains("3.14"));
        assert!(out.contains("42"));
    }

    #[test]
    fn sql_dotted_identifier_stays_together() {
        let out = format_sql("select t.col from t").unwrap();
        assert!(out.contains("t.col"), "esperado t.col pegado, salió: {}", out);
    }

    #[test]
    fn sql_quoted_identifiers_and_params_preserved() {
        let out = format_sql(r#"select "id", `name`, [col] from t where x = @p1 and y = $1"#).unwrap();
        assert!(out.contains("\"id\""));
        assert!(out.contains("`name`"));
        assert!(out.contains("[col]"));
        assert!(out.contains("@p1"));
        assert!(out.contains("$1"));
    }

    #[test]
    fn sql_string_literal_with_escaped_quote_preserved() {
        let out = format_sql("select 'it''s ok'").unwrap();
        assert!(out.contains("'it''s ok'"));
    }

    #[test]
    fn sql_comments_preserved_verbatim_not_uppercased() {
        let out = format_sql("select 1 -- a select comment\n/* block create */").unwrap();
        assert!(out.contains("-- a select comment"));
        assert!(out.contains("/* block create */"));
    }

    #[test]
    fn c5_ambiguous_backslash_before_quote_is_rejected_not_misformatted() {
        // Hallazgo C5: en SQL estándar (Postgres, SQL Server...) el backslash NO escapa
        // la comilla, así que `'C:\temp\'` termina justo tras "temp\" — pero la
        // convención MySQL SÍ lo trataría como escape y seguiría de largo, devorando el
        // siguiente literal `'x from y'` como si fuera parte del primero. En vez de
        // adivinar (y arriesgarse a reformatear dentro de una cadena ajena), debe
        // devolver Err.
        let sql = "SELECT * FROM t WHERE p = 'C:\\temp\\' AND name = 'x from y'";
        let result = format_sql(sql);
        assert!(result.is_err(), "se esperaba Err por literal ambiguo, se obtuvo: {:?}", result);
    }

    #[test]
    fn c5_backslash_not_before_quote_is_unambiguous_and_formats() {
        // Un backslash que no aparece justo antes de una comilla de cierre no es
        // ambiguo: ambas convenciones (estándar y MySQL) coinciden en dónde termina el
        // literal, así que debe seguir formateando con normalidad.
        let sql = "select 'C:\\temp\\files' from t";
        let out = format_sql(sql).unwrap();
        assert!(out.contains(r"'C:\temp\files'"));
    }

    /// L1: tras un texto inline pegado al cierre anterior, `indent_level` debe seguir
    /// decrementando en cada cierre. Antes de la corrección, el nivel crecía sin límite en
    /// documentos con mucho texto inline: la última línea del documento (el cierre de la raíz)
    /// debía quedar SIEMPRE en indentación 0, y el tamaño de la salida debe ser lineal respecto
    /// a la entrada (no exponencial/cuadrático por indentación descontrolada).
    #[test]
    fn xml_close_indent_never_grows_unbounded() {
        let mut input = String::from("<root>");
        for i in 0..500 {
            input.push_str(&format!("<i{}>texto {}</i{}>", i, i, i));
        }
        input.push_str("</root>");
        let out = format_xml(&input).unwrap();
        let last_line = out.lines().last().unwrap_or("");
        assert_eq!(last_line, "</root>", "el cierre de la raíz debe quedar en indent 0");
        // Tamaño lineal: unas pocas veces la entrada, nunca cuadrático/exponencial.
        assert!(
            out.len() < input.len() * 10,
            "salida desproporcionadamente grande: entrada={} salida={}",
            input.len(),
            out.len()
        );
    }

    /// L6: el texto real dentro de un elemento no debe perder el espacio que lo separaba de
    /// las construcciones vecinas ("Hello <b>world</b> again" no debe pegar "world</b>again"),
    /// y el whitespace puramente interno de un valor de texto no debe cambiarlo de más de lo
    /// que un espacio colapsado permitiría ("<v> x  y </v>" sigue conteniendo "x" y "y"
    /// separados, nunca pegados como "xy").
    #[test]
    fn xml_inline_text_keeps_separating_whitespace() {
        let out = format_xml("<p>Hello <b>world</b> again</p>").unwrap();
        let rendered: String = out.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(rendered.contains("Hello<b>world</b>again"), "salida: {}", out);
        assert!(out.contains("Hello"));
        assert!(out.contains("world"));
        assert!(out.contains("again"));
        assert!(!out.contains("Helloworld"));
        assert!(!out.contains("worldagain"));
    }

    #[test]
    fn xml_text_value_whitespace_not_collapsed_into_words() {
        let out = format_xml("<v> x  y </v>").unwrap();
        assert!(out.contains('x'));
        assert!(out.contains('y'));
        assert!(!out.contains("xy"), "el valor de texto no debe pegar x e y: {}", out);
    }

    #[test]
    fn c22_adjacent_inline_elements_in_mixed_content_stay_verbatim_no_inserted_whitespace() {
        // Hallazgo C22: "<b>x</b><i>y</i>" sin espacio entre ellos, dentro de contenido
        // mixto, no debe ganar NINGÚN whitespace nuevo entre "</b>" e "<i>" — insertar un
        // salto de línea ahí (como hacía el formateador de línea plana) cambia el
        // espaciado RENDERIZADO real (un visor HTML colapsa ese salto de línea a un
        // espacio visible entre "x" e "y", que no existía en el original).
        let out = format_xml("<p>Hello <b>x</b><i>y</i> world</p>").unwrap();
        assert!(
            out.contains("<b>x</b><i>y</i>"),
            "no debe insertarse whitespace entre </b> y <i>: {}",
            out
        );
    }

    #[test]
    fn c22_mixed_content_element_is_emitted_verbatim() {
        // El elemento con contenido mixto se emite EXACTAMENTE como en el original
        // (mismas comillas, mismo espaciado interno), sin reformatear su interior.
        let input = "<root><p>Hello <b>world</b> again</p></root>";
        let out = format_xml(input).unwrap();
        assert!(out.contains("<p>Hello <b>world</b> again</p>"), "salida: {}", out);
    }

    #[test]
    fn c22_non_mixed_siblings_still_get_reformatted_normally() {
        // El fix es específico del elemento con contenido mixto: un hermano SIN mezcla de
        // texto+elementos (p.ej. otro <p> que solo contiene texto, o solo elementos) debe
        // seguir reformateándose con normalidad, con su propia indentación.
        let input = "<root><p>Hello <b>world</b> again</p><q><a>1</a><b>2</b></q></root>";
        let out = format_xml(input).unwrap();
        assert!(out.contains("<p>Hello <b>world</b> again</p>"));
        // <q> no tiene texto directo (solo dos elementos hijos): no es contenido mixto,
        // así que sigue reformateándose en varias líneas con indentación.
        assert!(out.contains("<q>\n"), "salida: {}", out);
    }

    /// L2: invariante obligatorio del diseño conservador — el formateador SQL nunca inserta,
    /// borra ni reordena tokens no-whitespace; solo puede sustituir un tramo de whitespace por
    /// otro. Por tanto, partir la entrada y la salida por espacios en blanco debe dar
    /// exactamente la misma secuencia de tokens no vacíos, para todos los casos difíciles
    /// (unicode, notación científica, cadenas con prefijo, variables de motor, comentarios
    /// anidados, dollar-quoting, `CASE WHEN`, `t.*`, escapes de barra invertida, identificadores
    /// citados dobles/con corchetes escapados, operadores `->>`, cadenas sin cerrar, etc.).
    #[test]
    fn sql_conservative_format_never_touches_non_whitespace_tokens() {
        let cases: &[&str] = &[
            "SELECT año, café FROM tabla_niño",
            "SELECT 1e10, 1.5e-3, .5, 0x1F FROM t",
            "SELECT N'hola', X'FF', E'a' FROM t",
            "SELECT @@ROWCOUNT; SELECT * FROM #temp",
            "SELECT 'a -- b' AS x -- real comment\nFROM t",
            "/* outer /* inner */ still */ SELECT 1",
            "CREATE FUNCTION f() RETURNS int AS $$ SELECT  'it''s' ,  1 $$ LANGUAGE sql;",
            "SELECT $tag$ it's   here $tag$ FROM t",
            "SELECT CASE WHEN a > 1 THEN 'x' ELSE 'y' END AS c FROM t",
            "SELECT t.*, COUNT(*) FROM t",
            // Hallazgo C5: "SELECT 'it\\'s   spaced' FROM t" (escape de barra invertida
            // MySQL) se retiró de esta lista a propósito — un `\'` justo antes de una
            // comilla es EXACTAMENTE el patrón ambiguo que C5 identificó (SQL estándar y
            // MySQL discrepan en dónde cierra la cadena), y ahora `format_sql` devuelve
            // `Err` en vez de arriesgarse a adivinar mal (ver `c5_ambiguous_backslash_*`
            // más abajo, y `sql_string_literal_with_escaped_quote_preserved` para el
            // escape estándar `''`, que sigue soportado sin ambigüedad).
            r#"SELECT "we ""ird" FROM t"#,
            "SELECT [my]]col] FROM t",
            r#"SELECT s."Col", t.[x] FROM s"#,
            "SELECT data->>'k', data->'a' FROM t WHERE x::int >= 2",
            "SELECT a FROM t -- trailing",
            "SELECT 'abc FROM t",
            "SELECT -1, a-1, b - -2",
            "SELECT a FROM t WHERE b IN (1,2,3) AND c LIKE 'x%'",
            "select a >= 1 and b <> 2 and c || d",
            "select 3.14, 42",
            "select t.col from t",
            r#"select "id", `name`, [col] from t where x = @p1 and y = $1"#,
            "select 'it''s ok'",
        ];
        for input in cases {
            let out = format_sql(input).unwrap();
            let in_tokens: Vec<&str> = input.split_whitespace().collect();
            let out_tokens: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(
                in_tokens, out_tokens,
                "el formateador SQL cambió los tokens no-whitespace para: {:?}",
                input
            );
        }
    }
}

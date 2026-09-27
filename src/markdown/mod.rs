#![allow(dead_code)]

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Convert Markdown to raw HTML
pub fn render_markdown_to_html(input: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(input, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

struct TableCollector {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
}

impl TableCollector {
    fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            current_row: Vec::new(),
            current_cell: String::new(),
            in_head: false,
        }
    }

    fn render(&self) -> String {
        let num_cols = self.headers.len().max(
            self.rows.iter().map(|r| r.len()).max().unwrap_or(0),
        );
        if num_cols == 0 {
            return String::new();
        }

        let mut widths = vec![4usize; num_cols];
        for (i, h) in self.headers.iter().enumerate() {
            if i < num_cols {
                widths[i] = widths[i].max(h.chars().count());
            }
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
        }
        for w in &mut widths {
            *w = (*w).min(60);
        }

        let mut out = String::new();
        out.push('\n');

        // Top border: ┌───────┬───────┐
        out.push('┌');
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"─".repeat(*w + 2));
            if i + 1 < num_cols {
                out.push('┬');
            }
        }
        out.push_str("┐\n");

        // Header: │ Campo │ Detalle │
        if !self.headers.is_empty() {
            out.push('│');
            for (i, w) in widths.iter().enumerate() {
                let text = self.headers.get(i).map(|s| s.as_str()).unwrap_or("");
                let text_chars = text.chars().count();
                let pad = if text_chars < *w { w - text_chars } else { 0 };
                out.push(' ');
                out.push_str(text);
                out.push_str(&" ".repeat(pad));
                out.push_str(" │");
            }
            out.push('\n');

            // Header separator: ├───────┼───────┤
            out.push('├');
            for (i, w) in widths.iter().enumerate() {
                out.push_str(&"─".repeat(*w + 2));
                if i + 1 < num_cols {
                    out.push('┼');
                }
            }
            out.push_str("┤\n");
        }

        // Data rows: │ Val 1 │ Val 2 │
        for row in &self.rows {
            out.push('│');
            for (i, w) in widths.iter().enumerate() {
                let text = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let text_chars = text.chars().count();
                let pad = if text_chars < *w { w - text_chars } else { 0 };
                out.push(' ');
                out.push_str(text);
                out.push_str(&" ".repeat(pad));
                out.push_str(" │");
            }
            out.push('\n');
        }

        // Bottom border: └───────┴───────┘
        out.push('└');
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"─".repeat(*w + 2));
            if i + 1 < num_cols {
                out.push('┴');
            }
        }
        out.push_str("┘\n\n");

        out
    }
}

/// Render Markdown into structured, formatted text for in-app preview
pub fn render_markdown_preview(input: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(input, options);
    let mut result = String::new();
    let mut in_code_block = false;
    let mut current_heading_level: Option<usize> = None;
    // Pila de contadores de lista (hallazgo G/M6): cada nivel de anidamiento tiene su propio
    // contador independiente; antes un único `Option<u64>` hacía que una lista ordenada
    // anidada pisara y rompiera la numeración de la lista exterior.
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_blockquote = false;
    let mut current_link_url: Option<String> = None;
    let mut current_table: Option<TableCollector> = None;

    for event in parser {
        if let Some(ref mut tbl) = current_table {
            match event {
                Event::Start(Tag::TableHead) => {
                    tbl.in_head = true;
                }
                Event::End(TagEnd::TableHead) => {
                    tbl.in_head = false;
                }
                Event::Start(Tag::TableRow) => {
                    tbl.current_row.clear();
                }
                Event::End(TagEnd::TableRow) => {
                    if !tbl.in_head && !tbl.current_row.is_empty() {
                        let row = std::mem::take(&mut tbl.current_row);
                        tbl.rows.push(row);
                    }
                }
                Event::Start(Tag::TableCell) => {
                    tbl.current_cell.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    let text = std::mem::take(&mut tbl.current_cell);
                    let cleaned = text.trim().to_string();
                    if tbl.in_head {
                        tbl.headers.push(cleaned);
                    } else {
                        tbl.current_row.push(cleaned);
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(t) = current_table.take() {
                        result.push_str(&t.render());
                    }
                }
                Event::Text(text) => {
                    tbl.current_cell.push_str(&text);
                }
                Event::Code(code) => {
                    tbl.current_cell.push_str(&format!("[{}]", code));
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let lvl = match level {
                        pulldown_cmark::HeadingLevel::H1 => 1,
                        pulldown_cmark::HeadingLevel::H2 => 2,
                        pulldown_cmark::HeadingLevel::H3 => 3,
                        pulldown_cmark::HeadingLevel::H4 => 4,
                        pulldown_cmark::HeadingLevel::H5 => 5,
                        pulldown_cmark::HeadingLevel::H6 => 6,
                    };
                    current_heading_level = Some(lvl);
                    if !result.is_empty() && !result.ends_with("\n\n") {
                        if !result.ends_with('\n') {
                            result.push('\n');
                        }
                        result.push('\n');
                    }
                    if lvl >= 3 {
                        result.push_str("▸ ");
                    }
                }
                Tag::Paragraph => {
                    if !result.is_empty() && !result.ends_with("\n\n") {
                        if !result.ends_with('\n') {
                            result.push('\n');
                        }
                        result.push('\n');
                    }
                    if in_blockquote {
                        result.push_str("  │ ");
                    }
                }
                Tag::BlockQuote(_) => {
                    in_blockquote = true;
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                    let lang_name = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(l) => {
                            if l.is_empty() {
                                "código".to_string()
                            } else {
                                l.to_string()
                            }
                        }
                        pulldown_cmark::CodeBlockKind::Indented => "código".to_string(),
                    };
                    result.push_str(&format!(
                        "┌─── Código ({}) ─────────────────────────────\n",
                        lang_name
                    ));
                }
                Tag::List(start_num) => {
                    list_stack.push(start_num);
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                Tag::Item => {
                    let depth = list_stack.len().saturating_sub(1);
                    let indent = "  ".repeat(depth + 1);
                    if let Some(Some(num)) = list_stack.last_mut() {
                        result.push_str(&format!("{}{}. ", indent, num));
                        *num += 1;
                    } else {
                        result.push_str(&format!("{}• ", indent));
                    }
                }
                // V19: la vista previa no aplica negrita/cursiva por tramo (es un panel
                // de líneas con un solo estilo por línea), así que dejar los marcadores
                // `**`/`*`/`~` en el texto solo los mostraba literalmente sin dar ningún
                // énfasis real. Se omiten: el texto queda limpio, sin marcadores.
                Tag::Strong | Tag::Emphasis | Tag::Strikethrough => {}
                Tag::Link { dest_url, .. } => {
                    current_link_url = Some(dest_url.to_string());
                }
                Tag::Image { dest_url, .. } => {
                    result.push_str(&format!("🖼️ [Imagen: {}]", dest_url));
                }
                Tag::Table(_) => {
                    current_table = Some(TableCollector::new());
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    if let Some(lvl) = current_heading_level.take() {
                        result.push('\n');
                        if lvl == 1 {
                            result.push_str("────────────────────────────────────────────────────────\n\n");
                        } else if lvl == 2 {
                            result.push_str("┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈\n\n");
                        } else {
                            result.push('\n');
                        }
                    }
                }
                TagEnd::Paragraph => {
                    result.push_str("\n\n");
                }
                TagEnd::BlockQuote(_) => {
                    in_blockquote = false;
                    result.push('\n');
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push_str("└────────────────────────────────────────\n\n");
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                TagEnd::Item => {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                }
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {}
                TagEnd::Link => {
                    if let Some(url) = current_link_url.take() {
                        result.push_str(&format!(" 🔗 ({})", url));
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    for line in text.lines() {
                        result.push_str("│ ");
                        result.push_str(line);
                        result.push('\n');
                    }
                } else {
                    // V19: el H1 se mostraba forzado a MAYÚSCULAS; se conserva la
                    // capitalización original del documento.
                    result.push_str(&text);
                }
            }
            Event::Code(code) => {
                // V19: el código en línea se mostraba entre corchetes literales
                // (`[Ctrl+S]`); se muestra el contenido tal cual, sin corchetes.
                result.push_str(&code);
            }
            Event::TaskListMarker(checked) => {
                if checked {
                    result.push_str("[✓] ");
                } else {
                    result.push_str("[ ] ");
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let rendered_html = render_html_preview(&html);
                if !rendered_html.is_empty() {
                    result.push_str(&rendered_html);
                }
            }
            Event::SoftBreak => {
                // Un salto de línea suave (una sola línea nueva en el origen dentro del mismo
                // párrafo) es un espacio en Markdown/CommonMark, no un salto de línea real
                // (hallazgo M6): antes cortaba frases a media línea en la vista previa.
                result.push(' ');
            }
            Event::HardBreak => {
                result.push('\n');
                if in_blockquote {
                    result.push_str("  │ ");
                }
            }
            Event::Rule => {
                result.push_str("\n────────────────────────────────────────\n\n");
            }
            _ => {}
        }
    }

    // V19: distintas combinaciones de reglas (fin de párrafo + inicio de párrafo/
    // encabezado, ambas añadiendo su propio separador) podían encadenar 3+ saltos de
    // línea consecutivos, viéndose como una "banda vacía" grande; se colapsa cualquier
    // run de blancos consecutivos a una sola línea en blanco (exactamente "\n\n").
    let collapsed = collapse_blank_lines(&result);
    let trimmed = collapsed.trim().to_string();
    if trimmed.is_empty() {
        "(Documento vacío)".to_string()
    } else {
        trimmed
    }
}

/// Colapsa 2 o más líneas en blanco consecutivas (posiblemente con espacios) a una sola.
fn collapse_blank_lines(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut blank_run = 0usize;
    for line in input.split('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Render raw HTML into structured, formatted text for in-app preview
pub fn render_html_preview(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '<' {
            let start = i + 1;
            while i < len && chars[i] != '>' {
                i += 1;
            }
            if i < len {
                let tag_str: String = chars[start..i].iter().collect();
                let tag_trimmed = tag_str.trim().to_lowercase();
                let tag_name = tag_trimmed.split_whitespace().next().unwrap_or("");

                match tag_name {
                    "h1" => {
                        if !result.is_empty() && !result.ends_with('\n') {
                            result.push('\n');
                        }
                    }
                    "/h1" => {
                        result.push_str("\n════════════════════════════════════════\n\n");
                    }
                    "h2" => {
                        if !result.is_empty() && !result.ends_with('\n') {
                            result.push('\n');
                        }
                    }
                    "/h2" => {
                        result.push_str("\n────────────────────────────────────────\n\n");
                    }
                    "h3" | "h4" | "h5" | "h6" => {
                        if !result.is_empty() && !result.ends_with('\n') {
                            result.push('\n');
                        }
                        result.push_str("▸ ");
                    }
                    "/h3" | "/h4" | "/h5" | "/h6" => {
                        result.push_str("\n\n");
                    }
                    "p" | "div" => {
                        if !result.is_empty() && !result.ends_with('\n') {
                            result.push('\n');
                        }
                    }
                    "/p" | "/div" => {
                        result.push_str("\n\n");
                    }
                    "br" | "br/" => {
                        result.push('\n');
                    }
                    "hr" | "hr/" => {
                        result.push_str("\n────────────────────────────────────────\n\n");
                    }
                    "b" | "strong" => {
                        result.push('【');
                    }
                    "/b" | "/strong" => {
                        result.push('】');
                    }
                    "i" | "em" => {
                        result.push('_');
                    }
                    "/i" | "/em" => {
                        result.push('_');
                    }
                    "code" => {
                        result.push('`');
                    }
                    "/code" => {
                        result.push('`');
                    }
                    "pre" => {
                        result.push_str("\n┌─── Código ─────────────────────────────\n│ ");
                    }
                    "/pre" => {
                        result.push_str("\n└────────────────────────────────────────\n\n");
                    }
                    "li" => {
                        result.push_str("  • ");
                    }
                    "/li" => {
                        result.push('\n');
                    }
                    "blockquote" => {
                        result.push_str("\n  │ ");
                    }
                    "/blockquote" => {
                        result.push('\n');
                    }
                    _ => {}
                }
                i += 1; // skip '>'
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result.trim().to_string()
}

/// Tamaño máximo (en bytes) de documento que se renderiza completo en la vista previa
/// (hallazgo L19): más allá de esto, el árbol/eventos de `pulldown-cmark` sobre un documento de
/// varios MB tardaba varios cientos de ms POR TECLA (la vista previa se regenera en cada
/// edición), haciendo el editor perceptiblemente lento en documentos grandes.
const PREVIEW_MAX_BYTES: usize = 1_000_000;

/// Si `input` supera `PREVIEW_MAX_BYTES`, devuelve el primer tramo (cortado en un límite de
/// carácter válido) junto con una nota indicando el tamaño real; si no, `input` completo y sin
/// nota.
fn truncate_for_preview(input: &str) -> (&str, Option<String>) {
    if input.len() <= PREVIEW_MAX_BYTES {
        return (input, None);
    }
    let mut end = PREVIEW_MAX_BYTES;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    let total_mb = input.len() as f64 / 1_000_000.0;
    let note = format!("(vista previa truncada: documento de {:.1} MB)", total_mb);
    (&input[..end], Some(note))
}

/// Dispatch preview rendering based on detected language. Solo se usa el renderizador HTML
/// cuando el lenguaje detectado es realmente `html` (hallazgo G/M6): antes, cualquier
/// documento Markdown que empezara por `<` (p.ej. un comentario HTML embebido, o texto que
/// simplemente empezaba con "<3" o una etiqueta suelta) se previsualizaba entero como HTML.
/// Limita el documento renderizado a `PREVIEW_MAX_BYTES` (hallazgo L19), añadiendo una nota de
/// truncado al final cuando corresponde; la firma pública no cambia.
pub fn render_preview(input: &str, lang: &str) -> String {
    let (body, note) = truncate_for_preview(input);
    let rendered = if lang == "html" {
        render_html_preview(body)
    } else {
        render_markdown_preview(body)
    };
    match note {
        Some(n) => format!("{}\n\n{}", rendered, n),
        None => rendered,
    }
}

/// Generate a complete, standalone, beautifully styled HTML5 page for browser preview
pub fn generate_standalone_html(input: &str, lang: &str) -> String {
    let body_content = if lang == "html" {
        if input.to_lowercase().contains("<html") {
            return input.to_string();
        }
        input.to_string()
    } else {
        render_markdown_to_html(input)
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>LightMark - Vista Previa</title>
<style>
  :root {{
    --bg-color: #1e1e1e;
    --card-bg: #252526;
    --text-color: #d4d4d4;
    --heading-color: #ffffff;
    --accent-color: #007acc;
    --border-color: #3e3e42;
    --code-bg: #2d2d30;
  }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    background-color: var(--bg-color);
    color: var(--text-color);
    line-height: 1.6;
    padding: 2.5rem;
    max-width: 900px;
    margin: 0 auto;
  }}
  h1, h2, h3, h4, h5, h6 {{
    color: var(--heading-color);
    margin-top: 1.5rem;
    margin-bottom: 0.6rem;
    font-weight: 600;
    border-bottom: 1px solid var(--border-color);
    padding-bottom: 0.3rem;
  }}
  h1 {{ font-size: 2.2rem; }}
  h2 {{ font-size: 1.6rem; }}
  h3 {{ font-size: 1.3rem; border-bottom: none; }}
  a {{ color: #3794ff; text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
  code {{
    font-family: Consolas, "Fira Code", monospace;
    background: var(--code-bg);
    color: #ce9178;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
    font-size: 0.9em;
  }}
  pre {{
    background: var(--card-bg);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 1.2rem;
    overflow-x: auto;
  }}
  pre code {{
    background: transparent;
    padding: 0;
    color: #9cdcfe;
    font-size: 0.95em;
  }}
  blockquote {{
    border-left: 4px solid var(--accent-color);
    margin: 1rem 0;
    padding: 0.5rem 1rem;
    background: #25252644;
    color: #999999;
  }}
  table {{
    border-collapse: collapse;
    width: 100%;
    margin: 1.5rem 0;
  }}
  th, td {{
    border: 1px solid var(--border-color);
    padding: 8px 14px;
    text-align: left;
  }}
  th {{
    background: var(--code-bg);
    color: #ffffff;
  }}
  tr:nth-child(even) {{
    background: #25252655;
  }}
  ul, ol {{
    padding-left: 1.8rem;
  }}
  li {{
    margin-bottom: 0.3rem;
  }}
  hr {{
    border: none;
    border-top: 1px solid var(--border-color);
    margin: 2rem 0;
  }}
  .task-list-item {{
    list-style-type: none;
  }}
  .badge {{
    display: inline-block;
    padding: 2px 8px;
    font-size: 12px;
    border-radius: 12px;
    background: var(--accent-color);
    color: #ffffff;
    margin-bottom: 1rem;
  }}
</style>
</head>
<body>
<div class="badge">LightMark Preview</div>
{}
</body>
</html>"#,
        body_content
    )
}

pub fn markdown_to_plain_text(input: &str) -> String {
    let parser = Parser::new(input);
    let mut result = String::new();

    for event in parser {
        match event {
            Event::Text(text) => {
                result.push_str(&text);
            }
            Event::Code(code) => {
                result.push_str(&code);
            }
            Event::SoftBreak => {
                result.push(' ');
            }
            Event::HardBreak => {
                result.push('\n');
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_)) => {
                result.push_str("\n\n");
            }
            Event::End(TagEnd::Item) => {
                result.push('\n');
            }
            Event::Rule => {
                result.push_str("\n---\n");
            }
            _ => {}
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_break_becomes_space_not_hard_break() {
        let out = render_markdown_preview("uno\ndos");
        assert_eq!(out, "uno dos");
    }

    #[test]
    fn emphasis_inside_list_item_is_rendered() {
        // V19: los marcadores de énfasis (**, *) ya no se muestran literalmente en la
        // vista previa (no aportaban ningún énfasis visual real, solo ruido).
        let out = render_markdown_preview("- **Rust** es genial");
        assert!(out.contains("Rust"), "salida: {}", out);
        assert!(!out.contains("**Rust**"), "salida: {}", out);
    }

    #[test]
    fn v19_h1_keeps_original_case_and_inline_code_has_no_brackets() {
        let out = render_markdown_preview("# Rust\n\nUsa `Ctrl+S` para guardar.");
        assert!(out.contains("Rust"), "salida: {}", out);
        assert!(!out.contains("RUST"), "salida: {}", out);
        assert!(out.contains("Ctrl+S"), "salida: {}", out);
        assert!(!out.contains("[Ctrl+S]"), "salida: {}", out);
    }

    #[test]
    fn v19_consecutive_blank_lines_are_collapsed() {
        let out = render_markdown_preview("Uno\n\n\n\n\nDos");
        assert!(!out.contains("\n\n\n"), "salida contiene banda vacía: {:?}", out);
    }

    #[test]
    fn nested_ordered_lists_do_not_break_outer_numbering() {
        let out = render_markdown_preview("1. uno\n   1. anidado a\n   2. anidado b\n2. dos");
        assert!(out.contains("1. uno"), "salida: {}", out);
        assert!(out.contains("2. dos"), "salida: {}", out);
        assert!(out.contains("1. anidado a"), "salida: {}", out);
        assert!(out.contains("2. anidado b"), "salida: {}", out);
    }

    #[test]
    fn render_preview_dispatches_html_only_by_lang() {
        // Aunque empiece por '<', si el lenguaje detectado no es "html" se trata como Markdown.
        let md_like = "<3 esto no es HTML, es una emoticono al inicio de un párrafo";
        let out_md = render_preview(md_like, "markdown");
        let out_html = render_preview("<p>hola</p>", "html");
        assert!(!out_md.is_empty());
        assert!(out_html.contains("hola"));
    }

    #[test]
    fn markdown_to_plain_text_handles_utf8() {
        let out = markdown_to_plain_text("café con **t😀ext**");
        assert!(out.contains("café"));
        assert!(out.contains("t😀ext"));
    }

    #[test]
    fn render_preview_truncates_large_documents_with_a_note() {
        let para = "texto de relleno repetido varias veces. ";
        let big = para.repeat(1_000_000 / para.len() + 10); // > 1MB
        assert!(big.len() > super::PREVIEW_MAX_BYTES);
        let out = render_preview(&big, "markdown");
        assert!(
            out.contains("vista previa truncada"),
            "debería avisar de que se truncó: {}",
            &out[out.len().saturating_sub(120)..]
        );
        // La salida no debe intentar renderizar el documento completo.
        assert!(out.len() < big.len());
    }

    #[test]
    fn render_preview_small_document_has_no_truncation_note() {
        let out = render_preview("# Hola\n\nmundo", "markdown");
        assert!(!out.contains("truncada"));
    }
}


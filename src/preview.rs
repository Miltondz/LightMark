//! Construcción de la vista previa (panel derecho / vista `view_mode == 2`).
//!
//! Este módulo concentra `build_preview_items` (estilizado línea a línea para el modo
//! "lista", no seleccionable) y `extract_inner_text` (extractor de texto plano desde
//! fragmentos HTML), ambos movidos aquí desde el antiguo `main.rs` monolítico. También
//! expone `sync_preview`, que empuja `preview_text` / `preview_items` / `preview_title` /
//! `preview_available` a la UI para el lenguaje/documento dados.

use crate::{App, PreviewLine};

fn c_rgb(r: u8, g: u8, b: u8) -> slint::Color {
    slint::Color::from_argb_u8(255, r, g, b)
}

fn c_rgba(r: u8, g: u8, b: u8, a: u8) -> slint::Color {
    slint::Color::from_argb_u8(a, r, g, b)
}

/// `true` si el panel de vista previa tiene algo útil que mostrar para `lang`
/// (habilita el botón "Abrir en navegador" y evita forzar `view_mode` a 0).
pub fn is_preview_available(lang: &str) -> bool {
    matches!(lang, "markdown" | "html")
}

/// Título de cabecera del panel de vista previa, p.ej. `"VISTA PREVIA — Markdown"`.
pub fn preview_title_for(lang: &str) -> String {
    match lang {
        "markdown" => "VISTA PREVIA — Markdown".to_string(),
        "html" => "VISTA PREVIA — HTML".to_string(),
        _ => format!("VISTA PREVIA — {}", lang),
    }
}

/// Extrae el texto visible de un fragmento HTML de una sola línea, descartando las
/// etiquetas (`<...>`).
pub fn extract_inner_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Paleta de colores del panel de vista previa (modo "lista"), dependiente del tema
/// activo (hallazgo del agente de tema, ver `scratchpad/R3_HANDOFF.md`): antes esos
/// colores eran constantes FIJAS pensadas para el fondo gris de VS Code del tema
/// "Oscuro" original; con el tema "Océano" (degradado petróleo/casi negro) activo, esas
/// cajas quedaban como un recuadro azul/gris de VS Code "pegado" sobre un fondo
/// completamente distinto. `theme` es `Settings.theme` (0 = Oscuro, 1 = Océano — ver
/// `ui/theme.slint` → `Theme.current`); cualquier otro valor cae en Oscuro.
struct PreviewPalette {
    c_white: slint::Color,
    c_text: slint::Color,
    c_blue_h2: slint::Color,
    c_blue_h3: slint::Color,
    c_border: slint::Color,
    c_code: slint::Color,
    c_code_bg: slint::Color,
    c_quote: slint::Color,
    c_quote_border: slint::Color,
    c_green: slint::Color,
    c_gray: slint::Color,
    c_table_head: slint::Color,
    c_url: slint::Color,
}

fn preview_palette(theme: u32) -> PreviewPalette {
    match theme {
        // Océano: valores del agente de tema (`ui/theme.slint` → `Theme.*` para el
        // tema 1), elegidos para que estas cajas se lean como parte del mismo tema
        // petróleo/teal en vez de un recuadro de VS Code ajeno.
        1 => PreviewPalette {
            c_white: c_rgb(255, 255, 255),
            c_text: c_rgb(230, 237, 240),       // #e6edf0 — Theme.text
            c_blue_h2: c_rgb(127, 212, 224),    // variante clara del acento #3fa7b8
            c_blue_h3: c_rgb(127, 212, 224),
            c_border: c_rgb(27, 46, 53),        // #1b2e35 — Theme.border
            c_code: c_rgb(206, 145, 120),
            c_code_bg: c_rgb(15, 33, 39),       // #0f2127 — Theme.dialog-bg
            c_quote: c_rgb(187, 187, 187),
            c_quote_border: c_rgb(63, 167, 184), // #3fa7b8 — Theme.accent
            c_green: c_rgb(61, 201, 160),
            c_gray: c_rgb(159, 176, 182),        // #9fb0b6 — Theme.text-muted
            c_table_head: c_rgb(15, 36, 43),     // #0f242b — próximo a Theme.sidebar-bg
            c_url: c_rgb(63, 167, 184),          // #3fa7b8 — mismo acento
        },
        // Oscuro (por defecto, y cualquier valor desconocido): paleta original, sin
        // cambios.
        _ => PreviewPalette {
            c_white: c_rgb(255, 255, 255),
            c_text: c_rgb(212, 212, 212),
            c_blue_h2: c_rgb(86, 156, 214),
            c_blue_h3: c_rgb(79, 193, 255),
            c_border: c_rgb(62, 62, 66),
            c_code: c_rgb(206, 145, 120),
            c_code_bg: c_rgb(37, 37, 38),
            c_quote: c_rgb(187, 187, 187),
            c_quote_border: c_rgb(0, 122, 204),
            c_green: c_rgb(61, 201, 160),
            c_gray: c_rgb(136, 136, 136),
            c_table_head: c_rgb(45, 45, 48),
            c_url: c_rgb(55, 148, 255),
        },
    }
}

/// `true` si `line` es ESENCIALMENTE un enlace renderizado (`render_markdown_preview`
/// convierte `[etiqueta](url)` en `"etiqueta 🔗 (url)"`), no una frase de prosa que
/// simplemente menciona uno en medio de más texto: sin prosa después de la URL, y una
/// etiqueta razonablemente corta antes del marcador `🔗` (una etiqueta de enlace típica,
/// no un párrafo entero que solo de pasada contiene un enlace).
fn line_is_mostly_a_link(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(marker_pos) = trimmed.find('🔗') else {
        return false;
    };
    let after_marker = &trimmed[marker_pos..];
    let Some(close_rel) = after_marker.find(')') else {
        return false;
    };
    let link_end = marker_pos + close_rel + 1;
    if !trimmed[link_end..].trim().is_empty() {
        return false;
    }
    trimmed[..marker_pos].trim().chars().count() <= 80
}

/// Construye las líneas estilizadas del panel de vista previa (modo "lista", no
/// seleccionable) para `content` interpretado como `lang`, con la paleta de `theme` (0 =
/// Oscuro, 1 = Océano — ver `preview_palette`).
pub fn build_preview_items(content: &str, lang: &str, theme: u32) -> Vec<PreviewLine> {
    let c_trans = c_rgba(0, 0, 0, 0);
    let PreviewPalette {
        c_white,
        c_text,
        c_blue_h2,
        c_blue_h3,
        c_border,
        c_code,
        c_code_bg,
        c_quote,
        c_quote_border,
        c_green,
        c_gray,
        c_table_head,
        c_url,
    } = preview_palette(theme);

    let mut items = Vec::new();

    if lang == "html" {
        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter().take(500) {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();

            if lower.starts_with("<h1") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_white,
                    bg_color: c_trans,
                    font_size: 18.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 10.0,
                    padding_bottom: 6.0,
                });
            } else if lower.starts_with("<h2") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 15.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 8.0,
                    padding_bottom: 4.0,
                });
            } else if lower.starts_with("<h3") || lower.starts_with("<h4") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("▸ {}", inner).into(),
                    color: c_blue_h3,
                    bg_color: c_trans,
                    font_size: 13.5,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 2.0,
                });
            } else if lower.starts_with("<blockquote") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_quote,
                    bg_color: c_code_bg,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: true,
                    border_left_color: c_quote_border,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 12.0,
                    padding_top: 4.0,
                    padding_bottom: 4.0,
                });
            } else if lower.starts_with("<pre") || lower.starts_with("<code") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("  {}", inner).into(),
                    color: c_code,
                    bg_color: c_code_bg,
                    font_size: 12.5,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 8.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else if lower.starts_with("<li") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("  • {}", inner).into(),
                    color: c_text,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else {
                let inner = extract_inner_text(trimmed);
                if !inner.is_empty() {
                    items.push(PreviewLine {
                        text: inner.into(),
                        color: c_text,
                        bg_color: c_trans,
                        font_size: 13.0,
                        is_bold: false,
                        is_mono: false,
                        has_border_left: false,
                        border_left_color: c_trans,
                        has_border_bottom: false,
                        border_bottom_color: c_trans,
                        padding_left: 6.0,
                        padding_top: 2.0,
                        padding_bottom: 2.0,
                    });
                }
            }
        }
    } else {
        // Markdown (y cualquier otro lenguaje sin vista dedicada: se muestra como texto).
        let rendered = crate::markdown::render_markdown_preview(content);
        let raw_lines: Vec<&str> = rendered.lines().collect();
        let mut in_code_block = false;
        let mut in_table = false;
        let mut is_table_header = false;

        // Hallazgo C14: tope de líneas renderizadas en modo lista (antes 500, sin
        // ningún aviso al alcanzarlo: un documento más largo simplemente se veía
        // "cortado" en la vista previa sin que nada indicara que faltaba contenido).
        // Subido a 5000 (cubre con margen cualquier documento markdown real) Y, si aun
        // así se alcanza, se añade una línea explícita de aviso tras el bucle.
        const PREVIEW_MAX_ITEMS: usize = 5000;
        let mut i = 0;
        while i < raw_lines.len() && items.len() < PREVIEW_MAX_ITEMS {
            let line = raw_lines[i];

            if i + 1 < raw_lines.len()
                && raw_lines[i + 1].starts_with("──────")
                && !line.starts_with('┌')
                && !line.starts_with('├')
                && !line.starts_with('└')
            {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_white,
                    bg_color: c_trans,
                    font_size: 18.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 10.0,
                    padding_bottom: 6.0,
                });
                i += 2;
                continue;
            }

            if i + 1 < raw_lines.len() && raw_lines[i + 1].starts_with("┈┈┈┈") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 15.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 8.0,
                    padding_bottom: 4.0,
                });
                i += 2;
                continue;
            }

            if line.starts_with("▸ ") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h3,
                    bg_color: c_trans,
                    font_size: 14.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("┌─── Código") {
                in_code_block = true;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_gray,
                    bg_color: c_code_bg,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            if in_code_block && line.starts_with("└────────────────") {
                in_code_block = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_gray,
                    bg_color: c_code_bg,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 6.0,
                });
                i += 1;
                continue;
            }

            if in_code_block {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_code,
                    bg_color: c_code_bg,
                    font_size: 12.5,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("┌──") && line.contains('┬') {
                in_table = true;
                is_table_header = true;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 0.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("├──") && line.contains('┼') {
                is_table_header = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 0.0,
                    padding_bottom: 0.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("└──") && line.contains('┴') {
                in_table = false;
                is_table_header = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 0.0,
                    padding_bottom: 6.0,
                });
                i += 1;
                continue;
            }

            if in_table && line.starts_with('│') {
                let bg_color = if is_table_header { c_table_head } else { c_trans };
                let color = if is_table_header { c_white } else { c_text };
                items.push(PreviewLine {
                    text: line.into(),
                    color,
                    bg_color,
                    font_size: 12.0,
                    is_bold: is_table_header,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("  │ ") {
                let quote_text = line.strip_prefix("  │ ").unwrap_or(line);
                items.push(PreviewLine {
                    text: quote_text.into(),
                    color: c_quote,
                    bg_color: c_code_bg,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: true,
                    border_left_color: c_quote_border,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 12.0,
                    padding_top: 3.0,
                    padding_bottom: 3.0,
                });
                i += 1;
                continue;
            }

            if line.contains("[✓]") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_green,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            if line.contains("[ ]") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_gray,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            if line.trim().is_empty() {
                items.push(PreviewLine {
                    text: " ".into(),
                    color: c_trans,
                    bg_color: c_trans,
                    font_size: 6.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 0.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else {
                // Hallazgo del agente de tema: una línea que simplemente MENCIONA un
                // enlace en medio de más prosa ("Revisa este 🔗 (url) recurso") se
                // coloreaba ENTERA en azul de enlace solo por contener el marcador
                // "🔗" — coloreando de más texto que no es parte del enlace. Solo se
                // colorea como enlace cuando la línea es ESENCIALMENTE el enlace (nada
                // de prosa después de la URL, y una etiqueta razonablemente corta antes
                // del marcador); en cualquier otro caso ("línea mixta") se usa el color
                // de texto normal para toda la línea (no hay coloreado por tramos).
                let color = if line_is_mostly_a_link(line) { c_url } else { c_text };
                items.push(PreviewLine {
                    text: line.into(),
                    color,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            }

            i += 1;
        }

        // Hallazgo C14: si el bucle se detuvo por el tope (quedan líneas sin procesar),
        // decirlo explícitamente en vez de dejar que la vista previa se vea cortada sin
        // explicación.
        if i < raw_lines.len() {
            items.push(PreviewLine {
                text: "(vista previa truncada: el documento es más largo)".into(),
                color: c_gray,
                bg_color: c_trans,
                font_size: 12.0,
                is_bold: false,
                is_mono: false,
                has_border_left: false,
                border_left_color: c_trans,
                has_border_bottom: false,
                border_bottom_color: c_trans,
                padding_left: 6.0,
                padding_top: 8.0,
                padding_bottom: 8.0,
            });
        }
    }

    if items.is_empty() {
        items.push(PreviewLine {
            text: "(Documento vacío)".into(),
            color: c_gray,
            bg_color: c_trans,
            font_size: 13.0,
            is_bold: false,
            is_mono: false,
            has_border_left: false,
            border_left_color: c_trans,
            has_border_bottom: false,
            border_bottom_color: c_trans,
            padding_left: 6.0,
            padding_top: 10.0,
            padding_bottom: 10.0,
        });
    }

    items
}

/// Empuja `preview_title` / `preview_text` / `preview_items` / `preview_available` a la
/// UI para `content` interpretado como `lang`. Solo se debe llamar cuando la vista previa
/// esté realmente visible (`view_mode != 0`) — el temporizador de 150 ms en `main.rs` ya
/// respeta esa condición para no gastar CPU en segundo plano.
pub fn sync_preview(ui: &App, content: &str, lang: &str, theme: u32) {
    let available = is_preview_available(lang);
    ui.set_preview_available(available);
    ui.set_preview_title(preview_title_for(lang).into());

    // Hallazgo W13: `render_preview` (texto plano seleccionable) y `build_preview_items`
    // (lista de líneas con estilo) recorren y "renderizan" el mismo Markdown por
    // caminos completamente independientes — hacer los dos SIEMPRE, en cada tick de
    // 150ms mientras se escribe, es el doble del trabajo real necesario, porque el
    // panel solo muestra uno de los dos a la vez (`preview_selectable`). Solo se
    // recalcula el que está realmente visible; el otro conserva su último valor (se
    // recalculará en cuanto se alterne el toggle, vía `settings-changed` ->
    // `refresh_preview_if_visible`).
    if ui.get_preview_selectable() {
        let plain = crate::markdown::render_preview(content, lang);
        ui.set_preview_text(plain.into());
    } else {
        // Hallazgo U27: `zoom_level` no tenía ningún efecto sobre la vista previa (solo
        // escalaba `font_size` del editor); se aplica aquí el mismo factor a cada línea
        // estilizada para que Ctrl+"+"/"-" también agrande/reduzca la vista previa.
        let zoom = ui.get_zoom_level();
        let mut items = build_preview_items(content, lang, theme);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for item in &mut items {
                item.font_size *= zoom;
            }
        }
        let model = std::rc::Rc::new(slint::VecModel::from(items));
        ui.set_preview_items(model.into());
    }
}

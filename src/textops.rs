//! Operaciones de línea puras y conscientes del cursor/selección (desplazamientos en bytes,
//! igual que `TextInput` de Slint). No dependen de `Rope`: trabajan sobre `&str` y devuelven
//! el nuevo texto junto con la nueva posición de ancla/cursor.
/// Resultado de una operación de línea: nuevo texto y nueva selección (ancla/cursor en bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub anchor: usize,
    pub cursor: usize,
}

// ---------------------------------------------------------------------------
// Utilidades internas
// ---------------------------------------------------------------------------

/// Ajusta `i` a un límite de carácter válido dentro de `text`, sin exceder su longitud.
fn clamp(text: &str, mut i: usize) -> usize {
    if i > text.len() {
        i = text.len();
    }
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Encuentra el inicio (en bytes) de la línea que contiene `pos`.
fn find_line_start(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = pos.min(bytes.len());
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

/// Encuentra el final de la línea que empieza en o contiene `start`.
/// Devuelve `(fin_sin_salto, fin_con_salto)`: soporta `\n` y `\r\n`.
fn find_line_end(text: &str, start: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let mut i = start.min(bytes.len());
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    if i < bytes.len() {
        let end_with_nl = i + 1;
        let end_without_nl = if i > start && bytes[i - 1] == b'\r' {
            i - 1
        } else {
            i
        };
        (end_without_nl, end_with_nl)
    } else {
        (i, i)
    }
}

/// Calcula el rango de líneas afectado por una selección: `(inicio, fin_sin_salto, fin_con_salto)`.
/// Si el final de la selección cae justo en el inicio de una línea (nada de esa línea está
/// seleccionado), esa línea se excluye — igual que la mayoría de editores.
fn line_range(text: &str, anchor: usize, cursor: usize) -> (usize, usize, usize) {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let lo = a.min(c);
    let hi = a.max(c);
    let start = find_line_start(text, lo);
    let mut hi_probe = hi;
    if hi > lo && hi == find_line_start(text, hi) {
        hi_probe = hi - 1;
        if hi_probe < start {
            hi_probe = start;
        }
    }
    let (end_no_nl, end_with_nl) = find_line_end(text, hi_probe);
    (start, end_no_nl, end_with_nl)
}

fn clamp_len(pos: usize, len: usize) -> usize {
    pos.min(len)
}

// ---------------------------------------------------------------------------
// Operaciones públicas
// ---------------------------------------------------------------------------

/// Duplica la(s) línea(s) cubiertas por la selección justo debajo; el cursor se mueve a la copia.
pub fn duplicate_lines(text: &str, anchor: usize, cursor: usize) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let (start, end_no_nl, end_with_nl) = line_range(text, a, c);
    let block = &text[start..end_no_nl];

    let anchor_off = a.max(start).min(end_no_nl) - start;
    let cursor_off = c.max(start).min(end_no_nl) - start;

    let (insert_pos, copy_start, insert_str) = if end_with_nl > end_no_nl {
        (end_with_nl, end_with_nl, format!("{}\n", block))
    } else {
        (end_no_nl, end_no_nl + 1, format!("\n{}", block))
    };

    let mut new_text = String::with_capacity(text.len() + insert_str.len());
    new_text.push_str(&text[..insert_pos]);
    new_text.push_str(&insert_str);
    new_text.push_str(&text[insert_pos..]);

    let new_anchor = clamp_len(copy_start + anchor_off, new_text.len());
    let new_cursor = clamp_len(copy_start + cursor_off, new_text.len());
    Edit {
        text: new_text,
        anchor: new_anchor,
        cursor: new_cursor,
    }
}

/// Borra la(s) línea(s) cubiertas por la selección, incluyendo su salto de línea.
pub fn delete_lines(text: &str, anchor: usize, cursor: usize) -> Edit {
    let (start, end_no_nl, end_with_nl) = line_range(text, anchor, cursor);
    let (del_start, del_end) = if end_with_nl > end_no_nl {
        (start, end_with_nl)
    } else if start > 0 {
        (start - 1, end_no_nl)
    } else {
        (start, end_no_nl)
    };

    let mut new_text = String::with_capacity(text.len());
    new_text.push_str(&text[..del_start]);
    new_text.push_str(&text[del_end..]);
    let pos = clamp_len(del_start, new_text.len());
    Edit {
        text: new_text,
        anchor: pos,
        cursor: pos,
    }
}

/// Mueve la(s) línea(s) seleccionadas una posición hacia arriba; sin efecto si ya están arriba.
pub fn move_lines_up(text: &str, anchor: usize, cursor: usize) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let (start, end_no_nl, end_with_nl) = line_range(text, a, c);
    if start == 0 {
        return Edit {
            text: text.to_string(),
            anchor: a,
            cursor: c,
        };
    }

    let prev_start = find_line_start(text, start - 1);
    let (prev_end_no_nl, _prev_end_with_nl) = find_line_end(text, prev_start);
    let prev_content = &text[prev_start..prev_end_no_nl];
    let sep = &text[prev_end_no_nl..start];
    let block_content = &text[start..end_no_nl];
    let block_nl = &text[end_no_nl..end_with_nl];

    let mut new_text = String::with_capacity(text.len());
    new_text.push_str(&text[..prev_start]);
    new_text.push_str(block_content);
    new_text.push_str(sep);
    new_text.push_str(prev_content);
    new_text.push_str(block_nl);
    new_text.push_str(&text[end_with_nl..]);

    let anchor_off = a.max(start).min(end_no_nl) - start;
    let cursor_off = c.max(start).min(end_no_nl) - start;
    let new_start = prev_start;
    Edit {
        text: new_text.clone(),
        anchor: clamp_len(new_start + anchor_off, new_text.len()),
        cursor: clamp_len(new_start + cursor_off, new_text.len()),
    }
}

/// Mueve la(s) línea(s) seleccionadas una posición hacia abajo; sin efecto si ya están abajo.
pub fn move_lines_down(text: &str, anchor: usize, cursor: usize) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let (start, end_no_nl, end_with_nl) = line_range(text, a, c);
    if end_with_nl >= text.len() {
        return Edit {
            text: text.to_string(),
            anchor: a,
            cursor: c,
        };
    }

    let next_start = end_with_nl;
    let (next_end_no_nl, next_end_with_nl) = find_line_end(text, next_start);
    let block_content = &text[start..end_no_nl];
    let sep = &text[end_no_nl..end_with_nl];
    let next_content = &text[next_start..next_end_no_nl];
    let next_nl = &text[next_end_no_nl..next_end_with_nl];

    let mut new_text = String::with_capacity(text.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(next_content);
    new_text.push_str(sep);
    new_text.push_str(block_content);
    new_text.push_str(next_nl);
    new_text.push_str(&text[next_end_with_nl..]);

    let anchor_off = a.max(start).min(end_no_nl) - start;
    let cursor_off = c.max(start).min(end_no_nl) - start;
    let new_block_start = start + next_content.len() + sep.len();
    Edit {
        text: new_text.clone(),
        anchor: clamp_len(new_block_start + anchor_off, new_text.len()),
        cursor: clamp_len(new_block_start + cursor_off, new_text.len()),
    }
}

/// Devuelve los delimitadores de comentario `(apertura, cierre)` para un lenguaje.
fn comment_markers(lang: &str) -> (&'static str, &'static str) {
    match lang.to_lowercase().as_str() {
        "rust" | "javascript" | "typescript" | "c" | "cpp" | "java" | "csharp" | "go"
        | "json" => ("// ", ""),
        "python" | "ruby" | "bash" | "shell" | "yaml" | "toml" | "powershell" => ("# ", ""),
        "ini" => ("; ", ""),
        "sql" | "lua" => ("-- ", ""),
        "html" | "xml" | "markdown" => ("<!-- ", " -->"),
        _ => ("// ", ""),
    }
}

/// Etiqueta corta del marcador de comentario del lenguaje, para tooltips: `"//"`,
/// `"#"`, `"--"`, `";"` o `"<!-- -->"`.
pub fn comment_marker_label(lang: &str) -> String {
    let (open, close) = comment_markers(lang);
    if close.is_empty() {
        open.trim().to_string()
    } else {
        format!("{} {}", open.trim(), close.trim())
    }
}

/// Alterna el comentario de la(s) línea(s) seleccionadas. Si todas las líneas no vacías ya
/// están comentadas, las descomenta; en caso contrario, comenta las no vacías. Conserva la
/// indentación de cada línea.
pub fn toggle_comment(text: &str, anchor: usize, cursor: usize, lang: &str) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let (start, end_no_nl, _end_with_nl) = line_range(text, a, c);
    let (open, close) = comment_markers(lang);
    let block = &text[start..end_no_nl];
    let lines: Vec<&str> = if block.is_empty() {
        vec![""]
    } else {
        block.split('\n').collect()
    };

    // Separa un posible `\r` final de cada línea (soporte CRLF).
    let cores: Vec<(&str, &str)> = lines
        .iter()
        .map(|l| match l.strip_suffix('\r') {
            Some(stripped) => (stripped, "\r"),
            None => (*l, ""),
        })
        .collect();

    let is_commented = |core: &str| -> bool {
        let t = core.trim_start();
        if close.is_empty() {
            t.starts_with(open)
        } else {
            t.starts_with(open) && t.ends_with(close)
        }
    };

    let non_blank_commented: Vec<bool> = cores
        .iter()
        .filter(|(core, _)| !core.trim().is_empty())
        .map(|(core, _)| is_commented(core))
        .collect();
    let all_commented = !non_blank_commented.is_empty() && non_blank_commented.iter().all(|b| *b);

    let mut new_lines: Vec<String> = Vec::with_capacity(cores.len());
    for (core, cr) in &cores {
        let indent_len = core.len() - core.trim_start().len();
        let indent = &core[..indent_len];
        let rest = &core[indent_len..];
        let new_core = if all_commented {
            if is_commented(core) {
                let after_open = &rest[open.len()..];
                let after_open = if !close.is_empty() && after_open.ends_with(close) {
                    &after_open[..after_open.len() - close.len()]
                } else {
                    after_open
                };
                format!("{}{}", indent, after_open)
            } else {
                (*core).to_string()
            }
        } else if rest.is_empty() || is_commented(core) {
            (*core).to_string()
        } else {
            format!("{}{}{}{}", indent, open, rest, close)
        };
        new_lines.push(format!("{}{}", new_core, cr));
    }
    let new_block = new_lines.join("\n");

    let mut new_text = String::with_capacity(text.len() + new_block.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(&new_block);
    new_text.push_str(&text[end_no_nl..]);

    let map_pos = |p: usize| -> usize {
        if p < start {
            return p;
        }
        let p_in_block = p.min(end_no_nl) - start;
        let mut k = 0usize;
        let mut orig_line_start = 0usize;
        let mut acc = 0usize;
        for (i, l) in lines.iter().enumerate() {
            if acc <= p_in_block {
                k = i;
                orig_line_start = acc;
            } else {
                break;
            }
            acc += l.len() + 1;
        }
        let offset_in_line = (p_in_block - orig_line_start).min(lines[k].len());
        let new_line_start: usize = new_lines[..k].iter().map(|s| s.len() + 1).sum();
        let (core_k, _) = cores[k];
        let old_prefix_len = core_k.len() - core_k.trim_start().len();
        let delta: isize = new_lines[k].len() as isize - lines[k].len() as isize;
        let new_offset = if offset_in_line <= old_prefix_len {
            offset_in_line
        } else {
            ((offset_in_line as isize) + delta).max(old_prefix_len as isize) as usize
        };
        clamp_len(start + new_line_start + new_offset, new_text.len())
    };

    let new_a = map_pos(a);
    let new_c = map_pos(c);
    Edit {
        text: new_text,
        anchor: new_a,
        cursor: new_c,
    }
}

/// Inserta `unit` en el cursor (sin selección multi-línea) o prefija cada línea de la
/// selección multi-línea con `unit`.
pub fn indent(text: &str, anchor: usize, cursor: usize, unit: &str) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let lo = a.min(c);
    let hi = a.max(c);

    if !text[lo..hi].contains('\n') {
        let mut new_text = String::with_capacity(text.len() + unit.len());
        new_text.push_str(&text[..lo]);
        new_text.push_str(unit);
        new_text.push_str(&text[hi..]);
        let pos = lo + unit.len();
        return Edit {
            text: new_text,
            anchor: pos,
            cursor: pos,
        };
    }

    let (start, end_no_nl, _end_with_nl) = line_range(text, lo, hi);
    let block = &text[start..end_no_nl];
    let lines: Vec<&str> = if block.is_empty() {
        vec![""]
    } else {
        block.split('\n').collect()
    };

    let mut new_block = String::with_capacity(block.len() + unit.len() * lines.len());
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            new_block.push('\n');
        }
        new_block.push_str(unit);
        new_block.push_str(l);
    }

    let mut new_text = String::with_capacity(text.len() + new_block.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(&new_block);
    new_text.push_str(&text[end_no_nl..]);

    let shift_for = |p: usize| -> usize {
        let clamped_p = p.max(start).min(end_no_nl);
        let k = text[start..clamped_p].matches('\n').count();
        unit.len() * (k + 1)
    };

    let new_a = clamp_len(a + shift_for(a), new_text.len());
    let new_c = clamp_len(c + shift_for(c), new_text.len());
    Edit {
        text: new_text,
        anchor: new_a,
        cursor: new_c,
    }
}

/// Cuenta cuántos caracteres de indentación iniciales se deben eliminar de una línea:
/// una tabulación, o hasta `unit_width` espacios.
fn leading_removal(line: &str, unit_width: usize) -> usize {
    let bytes = line.as_bytes();
    if !bytes.is_empty() && bytes[0] == b'\t' {
        return 1;
    }
    let mut n = 0usize;
    while n < bytes.len() && n < unit_width && bytes[n] == b' ' {
        n += 1;
    }
    n
}

/// Elimina hasta `unit_width` espacios (o una tabulación) del inicio de cada línea seleccionada.
pub fn unindent(text: &str, anchor: usize, cursor: usize, unit_width: usize) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let (start, end_no_nl, _end_with_nl) = line_range(text, a, c);
    let block = &text[start..end_no_nl];
    let lines: Vec<&str> = if block.is_empty() {
        vec![""]
    } else {
        block.split('\n').collect()
    };

    let mut orig_starts = Vec::with_capacity(lines.len());
    let mut removed = Vec::with_capacity(lines.len());
    let mut acc = 0usize;
    for l in &lines {
        orig_starts.push(acc);
        removed.push(leading_removal(l, unit_width));
        acc += l.len() + 1;
    }

    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        new_lines.push(&l[removed[i]..]);
    }
    let new_block = new_lines.join("\n");

    let mut new_text = String::with_capacity(text.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(&new_block);
    new_text.push_str(&text[end_no_nl..]);

    let map = |p: usize| -> usize {
        let p_in_block = p.max(start).min(end_no_nl) - start;
        let mut k = 0usize;
        for i in 0..orig_starts.len() {
            if orig_starts[i] <= p_in_block {
                k = i;
            } else {
                break;
            }
        }
        let offset_in_line = p_in_block - orig_starts[k];
        let new_line_start: usize = (0..k).map(|i| lines[i].len() - removed[i] + 1).sum();
        let new_offset = offset_in_line
            .saturating_sub(removed[k])
            .min(lines[k].len() - removed[k]);
        clamp_len(start + new_line_start + new_offset, new_text.len())
    };

    let new_a = map(a);
    let new_c = map(c);
    Edit {
        text: new_text,
        anchor: new_a,
        cursor: new_c,
    }
}

/// Une las líneas seleccionadas (o la línea actual con la siguiente si no hay selección
/// multi-línea) con un único espacio, recortando los espacios iniciales de cada línea unida.
pub fn join_lines(text: &str, anchor: usize, cursor: usize) -> Edit {
    let a = clamp(text, anchor);
    let c = clamp(text, cursor);
    let lo = a.min(c);
    let hi = a.max(c);
    let (start, mut end_no_nl, end_with_nl) = line_range(text, lo, hi);

    let single_line_block = !text[start..end_no_nl].contains('\n');
    if single_line_block {
        if end_with_nl < text.len() {
            let (next_end_no_nl, _next_end_with_nl) = find_line_end(text, end_with_nl);
            end_no_nl = next_end_no_nl;
        } else {
            return Edit {
                text: text.to_string(),
                anchor: a,
                cursor: c,
            };
        }
    }

    let block = &text[start..end_no_nl];
    let lines: Vec<&str> = block.split('\n').collect();
    let mut joined = String::with_capacity(block.len());
    let mut join_point = 0usize;
    for (i, l) in lines.iter().enumerate() {
        let piece = l.trim_end_matches('\r');
        if i == 0 {
            joined.push_str(piece);
            join_point = piece.len();
        } else {
            joined.push(' ');
            joined.push_str(piece.trim_start());
        }
    }

    let mut new_text = String::with_capacity(text.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(&joined);
    new_text.push_str(&text[end_no_nl..]);

    let pos = clamp_len(start + join_point, new_text.len());
    Edit {
        text: new_text,
        anchor: pos,
        cursor: pos,
    }
}

/// Devuelve `(línea, columna)` en base 1 (columna en caracteres) para un desplazamiento en bytes.
/// Ya no la usa `edit_ops::on_cursor_moved` en caliente (hallazgo F: sustituida por
/// `rope_line_col`, que evita el `to_string()` del documento completo en cada
/// movimiento de cursor), pero se conserva pública porque sigue siendo la utilidad de
/// referencia (y la que cubren los tests de este módulo) para línea/columna a partir de
/// un `&str` ya materializado.
#[allow(dead_code)]
pub fn line_col(text: &str, byte: usize) -> (usize, usize) {
    let b = clamp(text, byte);
    let line_start = find_line_start(text, b);
    let line = text[..line_start].matches('\n').count() + 1;
    let col = text[line_start..b].chars().count() + 1;
    (line, col)
}

/// Devuelve el desplazamiento en bytes del inicio de la línea `line_1based` (base 1), acotado
/// al rango válido de líneas del documento. Útil para "Ir a línea".
pub fn line_start_offset(text: &str, line_1based: usize) -> usize {
    let total = line_count(text);
    let target_line = line_1based.max(1).min(total);
    let bytes = text.as_bytes();
    let mut current_line = 1usize;
    let mut pos = 0usize;
    while current_line < target_line && pos < bytes.len() {
        if bytes[pos] == b'\n' {
            current_line += 1;
        }
        pos += 1;
    }
    pos
}

/// Número total de líneas del documento (una línea vacía cuenta como 1; un salto de línea
/// final añade una línea vacía adicional).
pub fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

/// Detecta el estilo de fin de línea predominante: `"CRLF"` si aparece al menos un `\r\n`,
/// `"LF"` en caso contrario.
/// Ya no la usa `app::refresh_flags` para la barra de estado (ver el comentario junto a
/// `t.line_ending.as_str()` en `app/mod.rs`: sobre el documento en memoria, siempre
/// normalizado a LF, esta función solo podía devolver "LF"). Se conserva pública/testeada
/// como utilidad de detección independiente (equivalente en espíritu a
/// `normalize_newlines`, que hace su propia detección inline porque además necesita el
/// texto ya normalizado).
#[allow(dead_code)]
pub fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "CRLF"
    } else {
        "LF"
    }
}

// ---------------------------------------------------------------------------
// Diff mínimo prefijo/sufijo — hallazgo W12
// ---------------------------------------------------------------------------

/// Calcula el prefijo y sufijo comunes (en bytes, alineados a límite de carácter) entre
/// `old` y `new` (hallazgo W12): permite que el llamador reemplace solo la porción
/// central que realmente cambió sobre un `Rope` existente (`remove` + `insert`), en vez
/// de reconstruirlo entero desde una cadena nueva. Al mutar el `Rope` en el sitio, las
/// partes del árbol no tocadas quedan compartidas (vía `Rc`, interno de `ropey`) entre
/// la versión anterior (aún viva en la pila de deshacer) y la nueva — con cientos de
/// snapshots de undo de un documento grande, reconstruir el árbol entero por cada tecla
/// multiplicaba el uso de memoria en vez de compartir nodos.
///
/// Devuelve `(prefix_len, old_suffix_start, new_suffix_start)`: el cambio real es
/// sustituir `old[prefix_len..old_suffix_start]` por `new[prefix_len..new_suffix_start]`
/// (ambos límites son siempre índices válidos de carácter en su respectiva cadena).
pub fn common_prefix_suffix(old: &str, new: &str) -> (usize, usize, usize) {
    let old_b = old.as_bytes();
    let new_b = new.as_bytes();
    let min_len = old_b.len().min(new_b.len());

    let mut prefix = 0usize;
    while prefix < min_len && old_b[prefix] == new_b[prefix] {
        prefix += 1;
    }
    while prefix > 0 && !old.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let max_suffix = min_len - prefix;
    let mut suffix = 0usize;
    while suffix < max_suffix && old_b[old_b.len() - 1 - suffix] == new_b[new_b.len() - 1 - suffix] {
        suffix += 1;
    }
    // Los bytes del sufijo coincidente son idénticos en `old` y `new`, así que si
    // `old.len() - suffix` no es un límite de carácter válido, tampoco lo sería el
    // equivalente en `new` (mismo byte de continuación UTF-8); reducir `suffix` hasta
    // que sí lo sea nunca corta un carácter por la mitad.
    while suffix > 0 && !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }

    (prefix, old.len() - suffix, new.len() - suffix)
}

// ---------------------------------------------------------------------------
// Fin de línea (CRLF/LF) — hallazgo E11
// ---------------------------------------------------------------------------

/// Estilo de fin de línea de un documento cargado desde disco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
    /// El archivo mezcla estilos (`\r\n` y `\n` sueltos en el mismo documento) o contiene un
    /// `\r` suelto no seguido de `\n` (hallazgo L12). Antes, `normalize_newlines` colapsaba
    /// esto a un único estilo detectado y `restore_newlines` reescribía TODOS los saltos de
    /// línea al guardar — por ejemplo `"A\r\nB\nC"` se convertía en `"A\r\nB\r\nC"` (cambiando
    /// un archivo que el usuario nunca tocó), y `"A\r\r\nB"` perdía un byte. Con `Mixed`, el
    /// documento se trata como verbatim: no se toca ni al cargar ni al guardar.
    Mixed,
}

impl LineEnding {
    pub fn as_str(&self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
            LineEnding::Mixed => "Mixed",
        }
    }
}

/// Normaliza `text` a `\n` (el editor interno siempre trabaja en LF) y devuelve también el
/// estilo de fin de línea detectado, para poder restaurarlo al guardar (`restore_newlines`).
/// Si el documento mezcla `\r\n` con `\n` sueltos, o contiene algún `\r` no seguido de `\n`
/// (hallazgo L12), se considera `Mixed` y se devuelve SIN NORMALIZAR: reescribirlo a un único
/// estilo cambiaría bytes del archivo que el usuario nunca pidió tocar.
pub fn normalize_newlines(text: &str) -> (String, LineEnding) {
    match detect_line_ending_kind(text) {
        LineEnding::Mixed => (text.to_string(), LineEnding::Mixed),
        LineEnding::Crlf => (text.replace("\r\n", "\n"), LineEnding::Crlf),
        LineEnding::Lf => (text.to_string(), LineEnding::Lf),
    }
}

/// Clasifica el/los estilo(s) de fin de línea presentes en `text`.
fn detect_line_ending_kind(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    let mut has_crlf = false;
    let mut has_bare_lf = false;
    let mut has_bare_cr = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    has_crlf = true;
                    i += 2;
                } else {
                    has_bare_cr = true;
                    i += 1;
                }
            }
            b'\n' => {
                has_bare_lf = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if has_bare_cr || (has_crlf && has_bare_lf) {
        LineEnding::Mixed
    } else if has_crlf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

/// Inversa de `normalize_newlines`: si `ending` es CRLF, reintroduce `\r\n` en cada salto de
/// línea `\n` (el texto interno nunca debería contener ya `\r`, pero por seguridad no se
/// duplican los que ya estuvieran). Para `Lf` y `Mixed` es un no-op: un documento `Mixed` nunca
/// se normalizó al cargar, así que su texto ya es exactamente el original verbatim.
pub fn restore_newlines(text: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::Lf | LineEnding::Mixed => text.to_string(),
        LineEnding::Crlf => {
            let mut out = String::with_capacity(text.len() + text.len() / 40);
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\n' {
                    out.push('\r');
                    out.push('\n');
                } else if c == '\r' && chars.peek() == Some(&'\n') {
                    // Ya es CRLF: se copia tal cual (no se duplica).
                    out.push('\r');
                    out.push(chars.next().unwrap());
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- common_prefix_suffix (hallazgo W12) ---

    #[test]
    fn common_prefix_suffix_typing_at_end() {
        let (p, os, ns) = common_prefix_suffix("abc", "abcd");
        assert_eq!((p, os, ns), (3, 3, 4));
        assert_eq!(&"abc"[p..os], "");
        assert_eq!(&"abcd"[p..ns], "d");
    }

    #[test]
    fn common_prefix_suffix_typing_in_middle() {
        // "ab|cd" -> "ab X cd": prefijo "ab", sufijo "cd".
        let (p, os, ns) = common_prefix_suffix("abcd", "abXcd");
        assert_eq!(p, 2);
        assert_eq!(&"abcd"[os..], "cd");
        assert_eq!(&"abXcd"[ns..], "cd");
        assert_eq!(&"abXcd"[p..ns], "X");
    }

    #[test]
    fn common_prefix_suffix_no_common_boundary_cut_multibyte() {
        // 'ñ' es 2 bytes; el cambio ocurre justo antes/en medio de 'ñ', el diff no debe
        // cortarla por la mitad en ninguna de las dos cadenas.
        let old = "año";
        let new = "años";
        let (p, os, ns) = common_prefix_suffix(old, new);
        assert!(old.is_char_boundary(p) && old.is_char_boundary(os));
        assert!(new.is_char_boundary(p) && new.is_char_boundary(ns));
        // El cambio real es simplemente añadir "s" al final.
        assert_eq!(&new[p..ns], &new[old.len()..]);
    }

    #[test]
    fn common_prefix_suffix_identical_strings() {
        let (p, os, ns) = common_prefix_suffix("same", "same");
        assert_eq!(p, os);
        assert_eq!(p, ns);
    }

    #[test]
    fn common_prefix_suffix_total_replacement() {
        let (p, os, ns) = common_prefix_suffix("hello", "goodbye");
        assert_eq!(p, 0);
        assert_eq!(os, 5);
        assert_eq!(ns, 7);
    }

    #[test]
    fn duplicate_single_line() {
        let e = duplicate_lines("A\nB\nC", 2, 2);
        assert_eq!(e.text, "A\nB\nB\nC");
        assert_eq!(e.anchor, 4);
        assert_eq!(e.cursor, 4);
    }

    #[test]
    fn duplicate_last_line_no_trailing_newline() {
        let e = duplicate_lines("A\nB", 2, 3);
        assert_eq!(e.text, "A\nB\nB");
    }

    #[test]
    fn duplicate_empty_doc() {
        let e = duplicate_lines("", 0, 0);
        assert_eq!(e.text, "\n");
    }

    #[test]
    fn duplicate_multiline_selection() {
        let e = duplicate_lines("A\nB\nC\nD", 2, 5); // selects B\nC (start of B to start of C-line end... )
        assert_eq!(e.text, "A\nB\nC\nB\nC\nD");
    }

    #[test]
    fn delete_single_line() {
        let e = delete_lines("A\nB\nC", 2, 2);
        assert_eq!(e.text, "A\nC");
        assert_eq!(e.anchor, 2);
    }

    #[test]
    fn delete_last_line_no_trailing_newline() {
        let e = delete_lines("A\nB", 2, 3);
        assert_eq!(e.text, "A");
        assert_eq!(e.cursor, 1);
    }

    #[test]
    fn delete_only_line() {
        let e = delete_lines("A", 0, 1);
        assert_eq!(e.text, "");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn delete_empty_doc_noop() {
        let e = delete_lines("", 0, 0);
        assert_eq!(e.text, "");
    }

    #[test]
    fn delete_line_with_trailing_doc_newline() {
        let e = delete_lines("A\nB\n", 2, 2);
        assert_eq!(e.text, "A\n");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn move_up_basic() {
        let e = move_lines_up("A\nB\nC", 2, 2);
        assert_eq!(e.text, "B\nA\nC");
        assert_eq!(e.anchor, 0);
    }

    #[test]
    fn move_up_noop_at_top() {
        let e = move_lines_up("A\nB\nC", 0, 0);
        assert_eq!(e.text, "A\nB\nC");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn move_down_basic() {
        let e = move_lines_down("A\nB\nC", 2, 2);
        assert_eq!(e.text, "A\nC\nB");
    }

    #[test]
    fn move_down_noop_at_bottom() {
        let e = move_lines_down("A\nB\nC", 4, 4);
        assert_eq!(e.text, "A\nB\nC");
    }

    #[test]
    fn move_up_crlf() {
        let e = move_lines_up("A\r\nB\r\nC", 3, 3);
        assert_eq!(e.text, "B\r\nA\r\nC");
    }

    #[test]
    fn move_lines_up_multiline_block() {
        let e = move_lines_up("A\nB\nC\nD", 2, 5); // selects B,C
        assert_eq!(e.text, "B\nC\nA\nD");
    }

    #[test]
    fn toggle_comment_rust() {
        let e = toggle_comment("let x = 1;", 0, 0, "rust");
        assert_eq!(e.text, "// let x = 1;");
        let e2 = toggle_comment(&e.text, 0, 0, "rust");
        assert_eq!(e2.text, "let x = 1;");
    }

    #[test]
    fn toggle_comment_html() {
        let e = toggle_comment("<div>", 0, 0, "html");
        assert_eq!(e.text, "<!-- <div> -->");
        let e2 = toggle_comment(&e.text, 0, 0, "html");
        assert_eq!(e2.text, "<div>");
    }

    #[test]
    fn toggle_comment_multiline_mixed_becomes_all_commented() {
        let text = "a\n// b\nc";
        let e = toggle_comment(text, 0, text.len(), "rust");
        assert_eq!(e.text, "// a\n// b\n// c");
    }

    #[test]
    fn toggle_comment_preserves_indent() {
        let e = toggle_comment("    x = 1", 0, 0, "python");
        assert_eq!(e.text, "    # x = 1");
    }

    #[test]
    fn toggle_comment_ini_uses_semicolon() {
        let e = toggle_comment("key=1", 0, 0, "ini");
        assert_eq!(e.text, "; key=1");
    }

    #[test]
    fn indent_no_selection_inserts_unit() {
        let e = indent("abc", 1, 1, "  ");
        assert_eq!(e.text, "a  bc");
        assert_eq!(e.cursor, 3);
    }

    #[test]
    fn indent_multiline_selection_prefixes_each_line() {
        let e = indent("A\nB\nC", 0, 5, "  ");
        assert_eq!(e.text, "  A\n  B\n  C");
    }

    #[test]
    fn unindent_removes_spaces() {
        let e = unindent("    A\n    B", 0, 11, 4);
        assert_eq!(e.text, "A\nB");
    }

    #[test]
    fn unindent_removes_tab() {
        let e = unindent("\tA", 0, 0, 4);
        assert_eq!(e.text, "A");
    }

    #[test]
    fn unindent_partial_spaces() {
        let e = unindent("  A", 0, 0, 4);
        assert_eq!(e.text, "A");
    }

    #[test]
    fn join_two_lines_no_selection() {
        let e = join_lines("A\n  B", 0, 0);
        assert_eq!(e.text, "A B");
        assert_eq!(e.cursor, 1);
    }

    #[test]
    fn join_multiline_selection() {
        let e = join_lines("A\nB\nC", 0, 5);
        assert_eq!(e.text, "A B C");
    }

    #[test]
    fn join_last_line_noop() {
        let e = join_lines("A", 0, 0);
        assert_eq!(e.text, "A");
    }

    #[test]
    fn line_col_basic() {
        assert_eq!(line_col("abc\ndef", 5), (2, 2));
        assert_eq!(line_col("abc\ndef", 0), (1, 1));
    }

    #[test]
    fn line_col_multibyte() {
        // "á" ocupa 2 bytes en UTF-8; byte 3 cae justo antes de "b" (columna 3: á, espacio, b).
        assert_eq!(line_col("á b", 3), (1, 3));
    }

    #[test]
    fn line_start_offset_basic() {
        assert_eq!(line_start_offset("A\nB\nC", 1), 0);
        assert_eq!(line_start_offset("A\nB\nC", 2), 2);
        assert_eq!(line_start_offset("A\nB\nC", 3), 4);
    }

    #[test]
    fn line_start_offset_clamps_beyond_end() {
        assert_eq!(line_start_offset("A\nB", 99), 2);
    }

    #[test]
    fn line_count_cases() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("A"), 1);
        assert_eq!(line_count("A\n"), 2);
        assert_eq!(line_count("A\nB\nC"), 3);
    }

    #[test]
    fn detect_line_ending_cases() {
        assert_eq!(detect_line_ending("a\r\nb"), "CRLF");
        assert_eq!(detect_line_ending("a\nb"), "LF");
        assert_eq!(detect_line_ending("abc"), "LF");
    }

    #[test]
    fn normalize_then_restore_crlf_roundtrip() {
        let original = "A\r\nB\r\nC";
        let (norm, ending) = normalize_newlines(original);
        assert_eq!(norm, "A\nB\nC");
        assert_eq!(ending, LineEnding::Crlf);
        assert_eq!(restore_newlines(&norm, ending), original);
    }

    #[test]
    fn normalize_lf_stays_lf() {
        let (norm, ending) = normalize_newlines("A\nB");
        assert_eq!(norm, "A\nB");
        assert_eq!(ending, LineEnding::Lf);
        assert_eq!(restore_newlines(&norm, ending), "A\nB");
    }

    /// L12: un documento con `\r\n` Y `\n` sueltos mezclados no debe reescribirse a un único
    /// estilo — antes, `"A\r\nB\nC"` se normalizaba a LF (perdiendo la distinción) y al
    /// restaurar se convertía TODO a CRLF, dando `"A\r\nB\r\nC"` (distinto del original).
    #[test]
    fn mixed_crlf_and_bare_lf_is_left_verbatim() {
        let original = "A\r\nB\nC";
        let (norm, ending) = normalize_newlines(original);
        assert_eq!(ending, LineEnding::Mixed);
        assert_eq!(norm, original, "un documento Mixed no se toca al normalizar");
        assert_eq!(restore_newlines(&norm, ending), original);
    }

    /// Un `\r` suelto (no seguido de `\n`) también cuenta como Mixed, aunque no haya ningún
    /// `\r\n` en el documento (hallazgo L12: "o tiene \\r suelto").
    #[test]
    fn lone_cr_without_lf_is_mixed_and_left_verbatim() {
        let original = "A\rB";
        let (norm, ending) = normalize_newlines(original);
        assert_eq!(ending, LineEnding::Mixed);
        assert_eq!(norm, original);
        assert_eq!(restore_newlines(&norm, ending), original);
    }

    /// `"A\r\r\nB"`: un `\r` suelto inmediatamente antes de un `\r\n` real. La versión anterior
    /// perdía un byte al normalizar (colapsaba erróneamente parte de la secuencia); debe
    /// tratarse como Mixed y devolverse exactamente igual, byte a byte.
    #[test]
    fn cr_immediately_before_crlf_is_mixed_and_preserves_every_byte() {
        let original = "A\r\r\nB";
        let (norm, ending) = normalize_newlines(original);
        assert_eq!(ending, LineEnding::Mixed);
        assert_eq!(norm, original);
        assert_eq!(restore_newlines(&norm, ending), original);
    }

    #[test]
    fn trailing_crlf_is_pure_crlf_not_mixed() {
        let original = "A\r\n";
        let (norm, ending) = normalize_newlines(original);
        assert_eq!(ending, LineEnding::Crlf);
        assert_eq!(norm, "A\n");
        assert_eq!(restore_newlines(&norm, ending), original);
    }

    #[test]
    fn textops_ops_work_with_emoji_and_accents() {
        let e = duplicate_lines("t😀\ná b", 3, 3);
        assert_eq!(e.text, "t😀\nt😀\ná b");
        let e2 = toggle_comment("café", 0, 0, "rust");
        assert_eq!(e2.text, "// café");
    }

    #[test]
    fn duplicate_reversed_selection_anchor_after_cursor() {
        let e = duplicate_lines("A\nB\nC", 3, 2);
        assert_eq!(e.text, "A\nB\nB\nC");
    }

    #[test]
    fn delete_lines_reversed_selection() {
        let e = delete_lines("A\nB\nC", 3, 2);
        assert_eq!(e.text, "A\nC");
    }
}

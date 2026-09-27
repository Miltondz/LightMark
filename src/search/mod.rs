// `SearchResult::line/column/text` y `SearchOptions::wrap` quedan reservados para un
// futuro panel de resultados de búsqueda (lista tipo "3 coincidencias en la línea 12")
// y para una búsqueda no-envolvente opcional; el wiring actual solo usa `start`/`end`.
#![allow(dead_code)]

use ropey::Rope;

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub text: String,
    /// Grupos de captura (1-based, como en `$1`/`$2`) cuando la búsqueda es con regex; vacío
    /// en modo texto plano. Usado por `expand_replacement` para reemplazos con referencias
    /// (hallazgo D5).
    pub groups: Vec<Option<String>>,
}

#[derive(Clone, Copy)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub wrap: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            wrap: true,
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Tope de coincidencias devueltas por `search_in_rope` (hallazgo W5): sin límite, un
/// patrón con decenas de miles de coincidencias en un documento grande (p.ej. "e" en 1MB
/// de texto en inglés) sigue construyendo un `SearchResult` completo por cada una y
/// bloquea la UI de forma perceptible. 10000 resultados ya cubren con margen cualquier
/// uso real del panel de búsqueda.
pub const MAX_SEARCH_RESULTS: usize = 10_000;

/// Busca `query` en `rope` según `options`. Devuelve `Err` si `options.use_regex` y el
/// patrón no es una expresión regular válida (hallazgo W5: antes el llamador validaba el
/// regex por separado con su propio `RegexBuilder::build()` y luego esta función lo
/// volvía a compilar — el patrón se compilaba dos veces por cada tecleo en el campo de
/// búsqueda).
/// Indica si, dado `query`/`options`, la comprobación de límite de palabra debe hacerse
/// a mano tras el match (cuando la consulta empieza/termina en un carácter que no es de
/// palabra, `\b` no sirve — hallazgo D4).
fn needs_manual_boundary_check(query: &str, options: &SearchOptions) -> bool {
    if !options.whole_word {
        return false;
    }
    let starts_word = query.chars().next().map(is_word_char).unwrap_or(false);
    let ends_word = query.chars().last().map(is_word_char).unwrap_or(false);
    !(starts_word && ends_word)
}

/// Construye el `Regex` compilado para `query`/`options` (hallazgo W5 + L14/L15):
/// centraliza la construcción del patrón (escapado en modo texto plano, límites de
/// palabra con `\b` cuando aplica, `multi_line` para que `^`/`$` anclen por línea) para
/// que tanto `search_in_rope` como el reemplazo con expansión de grupos (`replace_*`)
/// usen exactamente el mismo `Regex`.
pub fn build_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, regex::Error> {
    let escaped_query = if options.use_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };

    let manual_boundary_check = needs_manual_boundary_check(query, &options);
    let pattern = if options.whole_word && !manual_boundary_check {
        format!(r"\b(?:{})\b", escaped_query)
    } else {
        escaped_query
    };

    let mut regex_builder = regex::RegexBuilder::new(&pattern);
    if !options.case_sensitive {
        regex_builder.case_insensitive(true);
    }
    // L14: sin esto, `^`/`$` en un patrón regex solo anclaban al principio/final de TODO
    // el documento (una sola cadena para `regex`), nunca al principio/final de cada
    // línea, que es el comportamiento esperado en un buscador de editor de texto.
    regex_builder.multi_line(true);
    regex_builder.build()
}

pub fn search_in_rope(
    rope: &Rope,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<SearchResult>, regex::Error> {
    search_in_rope_impl(rope, query, options, Some(MAX_SEARCH_RESULTS), false)
}

/// Como `search_in_rope`, pero sin el tope de `MAX_SEARCH_RESULTS` (hallazgo C1):
/// "Reemplazar todo" debe reemplazar TODAS las coincidencias del documento, no solo las
/// primeras 10000 (ese tope solo tiene sentido para el panel de búsqueda/navegación, que
/// no necesita listar más de eso). Sigue omitiendo coincidencias de ancho cero (L14): para
/// incluirlas también, usar `search_in_rope_for_replace_all`.
pub fn search_in_rope_unlimited(
    rope: &Rope,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<SearchResult>, regex::Error> {
    search_in_rope_impl(rope, query, options, None, false)
}

/// Variante usada específicamente por "Reemplazar todo" (hallazgo C1 + C4): sin tope de
/// resultados Y sin omitir coincidencias de ancho cero. La navegación/panel de búsqueda
/// (`search_in_rope`) sigue omitiendo los matches de ancho cero porque no hay nada útil
/// que resaltar/seleccionar para ellos, pero un reemplazo con un patrón como `^` (insertar
/// un prefijo en cada línea) SÍ debe aplicarse en cada una de esas posiciones.
pub fn search_in_rope_for_replace_all(
    rope: &Rope,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<SearchResult>, regex::Error> {
    search_in_rope_impl(rope, query, options, None, true)
}

fn search_in_rope_impl(
    rope: &Rope,
    query: &str,
    options: SearchOptions,
    limit: Option<usize>,
    include_empty: bool,
) -> Result<Vec<SearchResult>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let full_text = rope.to_string();
    let manual_boundary_check = needs_manual_boundary_check(query, &options);
    let re = build_regex(query, options)?;

    let mut results = Vec::new();
    // Hallazgo W5: recuento de línea/columna incremental en una sola pasada, en vez de
    // volver a escanear `full_text[..actual_start]` completo (`O(n)`) por CADA
    // coincidencia (`O(n·m)` en total: con decenas de miles de coincidencias en un
    // documento de 1MB esto colgaba la UI perceptiblemente). Como `captures_iter` entrega
    // los matches en orden creciente de posición, basta con contar los saltos de línea
    // entre el final del match anterior y el inicio del actual.
    let mut line = 0usize;
    let mut line_start = 0usize; // byte donde empieza `line` (tras el último '\n' visto)
    let mut scanned_to = 0usize; // hasta dónde ya se contaron saltos de línea

    for caps in re.captures_iter(&full_text) {
        if let Some(lim) = limit
            && results.len() >= lim
        {
            break;
        }
        let mat = caps.get(0).expect("el grupo 0 siempre existe en un match");
        let actual_start = mat.start();
        let actual_end = mat.end();

        // L14: una coincidencia de ancho cero (p.ej. `a*` en una posición sin "a", o `^`/`$`
        // con multi_line) no tiene nada que resaltar/seleccionar de forma útil en el panel
        // de búsqueda; se omite del listado de resultados en ese caso. `find_next`/
        // `next_index` siguen avanzando con normalidad porque operan sobre los resultados
        // reales restantes (hallazgo W25). Hallazgo C4: "Reemplazar todo" con un patrón de
        // ancho cero (p.ej. `^`) sí necesita estas coincidencias — `include_empty` lo
        // controla por llamada en vez de descartarlas siempre.
        if actual_start == actual_end && !include_empty {
            continue;
        }

        if manual_boundary_check {
            let before_is_word = full_text[..actual_start]
                .chars()
                .next_back()
                .map(is_word_char)
                .unwrap_or(false);
            let after_is_word = full_text[actual_end..]
                .chars()
                .next()
                .map(is_word_char)
                .unwrap_or(false);
            if before_is_word || after_is_word {
                continue;
            }
        }

        for (i, b) in full_text.as_bytes()[scanned_to..actual_start].iter().enumerate() {
            if *b == b'\n' {
                line += 1;
                line_start = scanned_to + i + 1;
            }
        }
        scanned_to = actual_start;
        let column = full_text[line_start..actual_start].chars().count();

        let matched_text: String = full_text[actual_start..actual_end].to_string();
        let groups: Vec<Option<String>> = if options.use_regex {
            (1..caps.len())
                .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                .collect()
        } else {
            Vec::new()
        };

        results.push(SearchResult {
            start: actual_start,
            end: actual_end,
            line,
            column,
            text: matched_text,
            groups,
        });
    }

    Ok(results)
}

/// Expande referencias de grupo en `replacement` contra la coincidencia `result`, usando
/// un `Regex` ya compilado y el texto COMPLETO del documento (hallazgo C3): antes se
/// re-matcheaba el patrón contra `result.text` (el texto ya extraído y aislado del
/// contexto), lo que rompe cualquier patrón que dependa de lo que hay alrededor del match
/// — límites de palabra (`\b`, `\B`), anclas de línea (`^`, `$` con `multi_line`), o
/// lookaround. Al re-matchear sobre el texto aislado esos patrones simplemente no vuelven
/// a encontrar nada (o encuentran algo distinto) y el reemplazo se degradaba al literal
/// sin expandir. Aquí se usa `captures_at` sobre el documento completo en la posición
/// exacta del match (`result.start`) y se comprueba que siga siendo el mismo rango
/// `start..end` antes de expandir.
fn expand_replacement_at(re: &regex::Regex, full_text: &str, replacement: &str, result: &SearchResult) -> String {
    match re.captures_at(full_text, result.start) {
        Some(caps)
            if caps
                .get(0)
                .map(|m| m.start() == result.start && m.end() == result.end)
                .unwrap_or(false) =>
        {
            let mut out = String::with_capacity(replacement.len());
            caps.expand(replacement, &mut out);
            out
        }
        // El patrón no vuelve a matchear exactamente el mismo rango (no debería pasar en
        // la práctica): se cae de vuelta al reemplazo literal en vez de perder el cambio.
        _ => replacement.to_string(),
    }
}

/// Expande referencias de grupo en `replacement` contra la coincidencia `result` de
/// `query`/`options` (hallazgo D5, L15, C3). Usa `regex::Captures::expand`, que además de
/// `$1`/`$2`/`$0`/`$$` entiende la forma con llaves `${1}` (necesaria para pegar un
/// grupo justo delante de texto que empieza por un dígito, p.ej. `${1}2`) y grupos con
/// nombre `(?P<nombre>...)` vía `$nombre`/`${nombre}`. En modo texto plano (sin regex) el
/// reemplazo es literal: se usa directamente sin expandir. `full_text` debe ser el
/// documento completo (no `result.text` aislado) para que patrones dependientes del
/// contexto (`\b`, `\B`, `^`, `$`) vuelvan a matchear correctamente (hallazgo C3).
pub fn expand_replacement(full_text: &str, query: &str, options: SearchOptions, replacement: &str, result: &SearchResult) -> String {
    if !options.use_regex {
        return replacement.to_string();
    }
    let Ok(re) = build_regex(query, options) else {
        return replacement.to_string();
    };
    expand_replacement_at(&re, full_text, replacement, result)
}

pub fn replace_in_rope(
    rope: &mut Rope,
    results: &[SearchResult],
    replacement: &str,
    replace_all: bool,
) -> usize {
    if results.is_empty() {
        return 0;
    }

    if replace_all {
        for result in results.iter().rev() {
            rope.remove(result.start..result.end);
            rope.insert(result.start, replacement);
        }
        results.len()
    } else if !results.is_empty() {
        let first = &results[0];
        rope.remove(first.start..first.end);
        rope.insert(first.start, replacement);
        1
    } else {
        0
    }
}

/// Como `replace_in_rope`, pero si `options.use_regex` es `true` expande referencias
/// `$1`/`${1}`/`$nombre`/... en `replacement` contra los grupos capturados de cada
/// resultado (hallazgo D5, L15) usando `regex::Captures::expand` (ver `expand_replacement`).
/// En modo texto plano el comportamiento es idéntico a `replace_in_rope`.
///
/// Hallazgo C3: el regex se compila UNA sola vez para toda la operación (antes se
/// recompilaba por cada coincidencia, vía `expand_replacement` → `build_regex`), y la
/// expansión se hace contra el texto completo del documento (capturado antes de empezar a
/// mutar el rope) en vez del texto aislado de cada resultado, para que patrones
/// dependientes del contexto (`\b`, `\B`, `^`, `$`) sigan funcionando.
pub fn replace_in_rope_expand(
    rope: &mut Rope,
    results: &[SearchResult],
    replacement: &str,
    replace_all: bool,
    query: &str,
    options: SearchOptions,
) -> usize {
    if results.is_empty() {
        return 0;
    }
    let targets: &[SearchResult] = if replace_all { results } else { &results[..1] };
    let full_text = if options.use_regex { Some(rope.to_string()) } else { None };
    let compiled = if options.use_regex { build_regex(query, options).ok() } else { None };
    let mut count = 0usize;
    for result in targets.iter().rev() {
        let text = match (&compiled, &full_text) {
            (Some(re), Some(full_text)) => expand_replacement_at(re, full_text, replacement, result),
            _ => replacement.to_string(),
        };
        rope.remove(result.start..result.end);
        rope.insert(result.start, &text);
        count += 1;
    }
    count
}

pub fn find_next(results: &[SearchResult], current_pos: usize, wrap: bool) -> Option<usize> {
    for (i, result) in results.iter().enumerate() {
        if result.start > current_pos {
            return Some(i);
        }
    }
    if wrap {
        results.first().map(|_| 0)
    } else {
        None
    }
}

pub fn find_prev(results: &[SearchResult], current_pos: usize, wrap: bool) -> Option<usize> {
    for (i, result) in results.iter().enumerate().rev() {
        if result.start < current_pos {
            return Some(i);
        }
    }
    if wrap {
        results.last().map(|_| results.len() - 1)
    } else {
        None
    }
}

/// Índice del resultado cuya selección coincide exactamente con `(sel_start, sel_end)`
/// (hallazgo W4): usado por "Reemplazar" para actuar sobre la coincidencia REALMENTE
/// seleccionada en pantalla. Antes se reutilizaba `nearest_index(cursor)`, pero tras
/// seleccionar una coincidencia el cursor queda en `r.end`, así que la primera con
/// `start >= cursor` es la coincidencia SIGUIENTE (k+1), no la seleccionada (k) —
/// "Reemplazar" sustituía la de después en vez de la resaltada.
pub fn find_selected_result(results: &[SearchResult], sel_start: usize, sel_end: usize) -> Option<usize> {
    results.iter().position(|r| r.start == sel_start && r.end == sel_end)
}

/// Índice del primer resultado cuyo inicio es `>= cursor_byte`; si no hay ninguno, vuelve al
/// primero (0). Devuelve `0` también cuando `results` está vacío (llamador debe comprobar
/// `is_empty()` antes de usar el índice).
pub fn nearest_index(results: &[SearchResult], cursor_byte: usize) -> usize {
    results
        .iter()
        .position(|r| r.start >= cursor_byte)
        .unwrap_or(0)
}

/// Índice del primer resultado con `start >= sel_end` (F3 / "buscar siguiente" desde el final
/// de la selección actual); si no hay ninguno, vuelve al primero (0), envolviendo (hallazgo
/// D3). Devuelve `0` también si `results` está vacío — el llamador debe comprobar
/// `is_empty()` antes de usar el índice.
///
/// Hallazgo W25: si la coincidencia ACTUALMENTE seleccionada es de ancho cero (p.ej.
/// `a*` sobre texto sin "a"), `start == end == sel_end`, así que un simple `start >=
/// sel_end` volvía a encontrar la MISMA coincidencia una y otra vez — F3 se quedaba
/// atascado sin avanzar nunca. Un resultado con `start == sel_end` solo cuenta como
/// "siguiente" si además `end > sel_end` (es decir, si no es exactamente la misma
/// coincidencia de ancho cero ya seleccionada): así siempre se avanza al menos 1 byte.
pub fn next_index(results: &[SearchResult], _sel_start: usize, sel_end: usize) -> usize {
    results
        .iter()
        .position(|r| r.start > sel_end || (r.start >= sel_end && r.end > sel_end))
        .unwrap_or(0)
}

/// Índice del último resultado con `start < sel_start` (Shift+F3 / "buscar anterior" desde el
/// inicio de la selección actual); si no hay ninguno, vuelve al último, envolviendo (hallazgo
/// D3). Devuelve `0` también si `results` está vacío.
pub fn prev_index(results: &[SearchResult], sel_start: usize) -> usize {
    results
        .iter()
        .rposition(|r| r.start < sel_start)
        .unwrap_or_else(|| results.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_sensitive_search() {
        let rope = Rope::from_str("Hello hello HELLO");
        let mut opts = SearchOptions::default();
        opts.case_sensitive = true;
        let results = search_in_rope(&rope, "hello", opts).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start, 6);
    }

    #[test]
    fn case_insensitive_search() {
        let rope = Rope::from_str("Hello hello HELLO");
        let opts = SearchOptions::default(); // case_insensitive by default
        let results = search_in_rope(&rope, "hello", opts).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn whole_word_search() {
        let rope = Rope::from_str("cat catalog cat");
        let mut opts = SearchOptions::default();
        opts.whole_word = true;
        let results = search_in_rope(&rope, "cat", opts).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn regex_search() {
        let rope = Rope::from_str("a1 b22 c333");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        let results = search_in_rope(&rope, r"\d+", opts).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[2].text, "333");
    }

    #[test]
    fn invalid_regex_returns_err_no_panic() {
        let rope = Rope::from_str("anything");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        let result = search_in_rope(&rope, "(unclosed", opts);
        assert!(result.is_err());
    }

    #[test]
    fn empty_query_returns_empty() {
        let rope = Rope::from_str("anything");
        let results = search_in_rope(&rope, "", SearchOptions::default()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn replace_single_and_all() {
        let mut rope = Rope::from_str("foo foo foo");
        let results = search_in_rope(&rope, "foo", SearchOptions::default()).unwrap();
        let n = replace_in_rope(&mut rope, &results, "bar", false);
        assert_eq!(n, 1);
        assert_eq!(rope.to_string(), "bar foo foo");

        let results2 = search_in_rope(&rope, "foo", SearchOptions::default()).unwrap();
        let n2 = replace_in_rope(&mut rope, &results2, "bar", true);
        assert_eq!(n2, 2);
        assert_eq!(rope.to_string(), "bar bar bar");
    }

    #[test]
    fn nearest_index_finds_at_or_after_cursor() {
        let results = vec![
            SearchResult { start: 2, end: 5, line: 0, column: 2, text: "a".into(), groups: Vec::new() },
            SearchResult { start: 10, end: 13, line: 0, column: 10, text: "b".into(), groups: Vec::new() },
        ];
        assert_eq!(nearest_index(&results, 0), 0);
        assert_eq!(nearest_index(&results, 3), 1);
        assert_eq!(nearest_index(&results, 10), 1);
        assert_eq!(nearest_index(&results, 999), 0); // wrap
    }

    #[test]
    fn next_index_from_selection_end_wraps() {
        let rope = Rope::from_str("cat cat cat");
        let results = search_in_rope(&rope, "cat", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(next_index(&results, 0, 3), 1); // sel [0,3): siguiente empieza en 4
        assert_eq!(next_index(&results, 4, 7), 2);
        assert_eq!(next_index(&results, 8, 11), 0); // último: envuelve al primero
    }

    #[test]
    fn next_index_advances_past_zero_width_matches() {
        // Hallazgo W25: F3 sobre coincidencias de ancho cero (p.ej. un regex "a*" que
        // matchea la cadena vacía en cada posición) no debe quedarse atascado
        // seleccionando la misma coincidencia una y otra vez.
        let results = vec![
            SearchResult { start: 0, end: 0, line: 0, column: 0, text: String::new(), groups: Vec::new() },
            SearchResult { start: 1, end: 1, line: 0, column: 1, text: String::new(), groups: Vec::new() },
            SearchResult { start: 2, end: 2, line: 0, column: 2, text: String::new(), groups: Vec::new() },
        ];
        // Seleccionada la coincidencia en 0 (sel_start=sel_end=0): la siguiente debe ser
        // la de la posición 1, nunca la misma de la posición 0 otra vez.
        assert_eq!(next_index(&results, 0, 0), 1);
        assert_eq!(next_index(&results, 1, 1), 2);
        // Tras la última, envuelve a la primera (no se queda atascada en la última).
        assert_eq!(next_index(&results, 2, 2), 0);
    }

    #[test]
    fn prev_index_from_selection_start_wraps() {
        let rope = Rope::from_str("cat cat cat");
        let results = search_in_rope(&rope, "cat", SearchOptions::default()).unwrap();
        assert_eq!(prev_index(&results, 8), 1);
        assert_eq!(prev_index(&results, 4), 0);
        assert_eq!(prev_index(&results, 0), 2); // primero: envuelve al último
    }

    #[test]
    fn whole_word_query_starting_with_non_word_char() {
        // "-cat" empieza en un carácter que no es de palabra: \b no aplicaría bien, se filtra
        // a mano que los vecinos no sean caracteres de palabra (hallazgo D4).
        let rope = Rope::from_str("a-cat b -cat c -catalog");
        let mut opts = SearchOptions::default();
        opts.whole_word = true;
        let results = search_in_rope(&rope, "-cat", opts).unwrap();
        // "a-cat" no cuenta (el carácter antes de "-" es "a", de palabra... en realidad antes
        // del match está 'a' pegado a '-': el vecino inmediato ANTES del match es 'a', que sí
        // es de palabra -> se descarta). " -cat " sí cuenta. " -catalog" se descarta (el
        // carácter siguiente al match, 'a', es de palabra).
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start, 8);
    }

    #[test]
    fn l14_multiline_anchors_match_each_line() {
        let rope = Rope::from_str("foo\nbar\nfoobar");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        // `^foo` con multi_line debe encontrar el "foo" al inicio de la línea 1 Y el
        // "foo" al inicio de la línea 3 ("foobar"), no solo el del principio del documento.
        let results = search_in_rope(&rope, "^foo", opts).unwrap();
        assert_eq!(results.len(), 2, "resultados: {:?}", results);
        assert_eq!(results[0].line, 0);
        assert_eq!(results[1].line, 2);
    }

    #[test]
    fn l14_zero_width_matches_are_skipped() {
        let rope = Rope::from_str("abc");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        // "x*" matchea la cadena vacía en cada posición de "abc" (ninguna "x" presente):
        // ninguno de esos matches de ancho cero debe aparecer en los resultados.
        let results = search_in_rope(&rope, "x*", opts).unwrap();
        assert!(results.is_empty(), "resultados: {:?}", results);
    }

    #[test]
    fn regex_replace_expands_capture_groups() {
        let mut rope = Rope::from_str("John Smith");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        let results = search_in_rope(&rope, r"(\w+) (\w+)", opts).unwrap();
        assert_eq!(results.len(), 1);
        let n = replace_in_rope_expand(&mut rope, &results, "$2 $1", false, r"(\w+) (\w+)", opts);
        assert_eq!(n, 1);
        assert_eq!(rope.to_string(), "Smith John");
    }

    #[test]
    fn l15_braced_group_reference_expands() {
        // ${1} permite pegar el grupo justo delante de texto que empieza por un dígito,
        // algo que "$1" no puede expresar sin ambigüedad ("$12" se leería como el grupo 12).
        let mut rope = Rope::from_str("v1");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        let query = r"v(\d+)";
        let results = search_in_rope(&rope, query, opts).unwrap();
        assert_eq!(results.len(), 1);
        let n = replace_in_rope_expand(&mut rope, &results, "${1}2", false, query, opts);
        assert_eq!(n, 1);
        assert_eq!(rope.to_string(), "12");
    }

    #[test]
    fn l15_named_group_reference_expands() {
        let mut rope = Rope::from_str("John Smith");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        let query = r"(?P<first>\w+) (?P<last>\w+)";
        let results = search_in_rope(&rope, query, opts).unwrap();
        assert_eq!(results.len(), 1);
        let n = replace_in_rope_expand(&mut rope, &results, "$last $first", false, query, opts);
        assert_eq!(n, 1);
        assert_eq!(rope.to_string(), "Smith John");
    }

    #[test]
    fn find_selected_result_matches_exact_selection_not_next() {
        // Hallazgo W4: tras seleccionar la coincidencia en [4,7), el cursor queda en 7
        // (el final de la selección). `nearest_index(7)` encontraría la SIGUIENTE
        // coincidencia (la que empieza en 8), no la seleccionada. `find_selected_result`
        // debe encontrar la de [4,7) usando la selección completa (anchor, cursor).
        let rope = Rope::from_str("cat cat cat");
        let results = search_in_rope(&rope, "cat", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(find_selected_result(&results, 4, 7), Some(1));
        // Selección que no coincide con ningún resultado (p.ej. tras editar el documento).
        assert_eq!(find_selected_result(&results, 0, 2), None);
    }

    #[test]
    fn plain_replace_expand_is_literal_even_with_dollar_signs() {
        let mut rope = Rope::from_str("foo");
        let opts = SearchOptions::default();
        let results = search_in_rope(&rope, "foo", opts).unwrap();
        let n = replace_in_rope_expand(&mut rope, &results, "$1 literal", false, "foo", opts);
        assert_eq!(n, 1);
        assert_eq!(rope.to_string(), "$1 literal");
    }

    #[test]
    fn search_large_document_completes_quickly_and_caps_results() {
        // Hallazgo W5: buscar un patrón muy frecuente ("e") en ~1MB de texto no debe
        // colgar la UI. Antes, el recuento de línea/columna volvía a escanear el prefijo
        // completo desde el principio del documento por CADA coincidencia (O(n·m)); con
        // decenas de miles de coincidencias en 1MB esto tardaba varios segundos.
        let line = "the quick brown fox jumps over the lazy dog\n"; // 45 bytes, varias 'e'
        let repeats = (1024 * 1024) / line.len() + 1;
        let text = line.repeat(repeats);
        assert!(text.len() >= 1024 * 1024);
        let rope = Rope::from_str(&text);

        let start = std::time::Instant::now();
        let results = search_in_rope(&rope, "e", SearchOptions::default()).unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "la búsqueda tardó demasiado: {:?}",
            elapsed
        );
        // Tope de resultados (hallazgo W5): el patrón aparece muchísimas más veces que
        // MAX_SEARCH_RESULTS en 1MB de texto, así que el resultado debe quedar capado.
        assert_eq!(results.len(), MAX_SEARCH_RESULTS);
    }

    #[test]
    fn c1_replace_all_is_not_capped_at_max_search_results() {
        // Hallazgo C1: "Reemplazar todo" debe reemplazar TODAS las coincidencias, no
        // detenerse en las primeras MAX_SEARCH_RESULTS (10000).
        let big = "a ".repeat(MAX_SEARCH_RESULTS + 2_000);
        let rope = Rope::from_str(&big);
        let mut opts = SearchOptions::default();
        opts.case_sensitive = true;

        // La búsqueda de navegación normal sí queda capada.
        let capped = search_in_rope(&rope, "a", opts).unwrap();
        assert_eq!(capped.len(), MAX_SEARCH_RESULTS);

        // La variante sin tope, usada por "Reemplazar todo", encuentra todas.
        let all = search_in_rope_unlimited(&rope, "a", opts).unwrap();
        assert_eq!(all.len(), MAX_SEARCH_RESULTS + 2_000);

        let mut out = rope.clone();
        let n = replace_in_rope(&mut out, &all, "b", true);
        assert_eq!(n, MAX_SEARCH_RESULTS + 2_000);
        assert_eq!(out.to_string().matches('a').count(), 0);
    }

    #[test]
    fn c3_expand_replacement_respects_context_dependent_regex() {
        // Hallazgo C3: `\B` (límite de NO-palabra) depende de los caracteres vecinos del
        // match en el documento completo. Re-matchear contra el texto ya aislado de cada
        // resultado (una sola "o") pierde ese contexto y `\B` deja de encontrar nada, así
        // que el reemplazo se degradaba al literal "[$1]" sin expandir el grupo.
        let mut rope = Rope::from_str("foo");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        opts.case_sensitive = true;
        let query = r"\B(o)";
        let results = search_in_rope(&rope, query, opts).unwrap();
        assert_eq!(results.len(), 2, "resultados: {:?}", results);
        let n = replace_in_rope_expand(&mut rope, &results, "[$1]", true, query, opts);
        assert_eq!(n, 2);
        assert_eq!(rope.to_string(), "f[o][o]");
    }

    #[test]
    fn c3_expand_replacement_single_result_uses_full_document_context() {
        // Mismo hallazgo C3 pero para la ruta de "Reemplazar" (uno solo), que usa
        // `expand_replacement` directamente con el texto completo del documento.
        let rope = Rope::from_str("foo");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;
        opts.case_sensitive = true;
        let query = r"\B(o)";
        let results = search_in_rope(&rope, query, opts).unwrap();
        assert_eq!(results.len(), 2);
        let full_text = rope.to_string();
        let expanded = expand_replacement(&full_text, query, opts, "[$1]", &results[0]);
        assert_eq!(expanded, "[o]");
    }

    #[test]
    fn c4_replace_all_includes_zero_width_matches() {
        // Hallazgo C4: regresión donde "Reemplazar todo" con un patrón de ancho cero como
        // `^` (prefijar cada línea) dejó de hacer ningún reemplazo ("Sin resultados"),
        // porque `search_in_rope` omite los matches de ancho cero para la navegación. La
        // variante para "Reemplazar todo" debe incluirlos.
        let rope = Rope::from_str("l1\nl2\nl3");
        let mut opts = SearchOptions::default();
        opts.use_regex = true;

        // La búsqueda de navegación sigue omitiendo los matches de ancho cero (L14).
        let nav_results = search_in_rope(&rope, "^", opts).unwrap();
        assert!(nav_results.is_empty(), "resultados: {:?}", nav_results);

        let replace_results = search_in_rope_for_replace_all(&rope, "^", opts).unwrap();
        assert_eq!(replace_results.len(), 3, "resultados: {:?}", replace_results);

        let mut out = rope.clone();
        let n = replace_in_rope(&mut out, &replace_results, "// ", true);
        assert_eq!(n, 3);
        assert_eq!(out.to_string(), "// l1\n// l2\n// l3");
    }
}

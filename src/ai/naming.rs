//! Nombres de archivo sugeridos por IA (WS-D núcleo; la integración en Guardar como es WS-D2):
//! constructor del prompt (AI_PROVIDERS §4), extracción del fragmento y saneado de candidatos.
#![allow(dead_code)]

pub const SYSTEM_PROMPT: &str =
    "Eres un asistente que propone nombres de archivo. Responde SOLO con JSON válido, sin explicaciones.";

/// Caracteres del documento que se envían como máximo.
pub const SNIPPET_CHARS: usize = 2000;
const MAX_WORDS: usize = 6;
const MAX_CHARS: usize = 60;

/// Primeros `max` caracteres (límite de char, nunca corta un UTF-8), recortados.
pub fn snippet(text: &str, max: usize) -> String {
    let end = text.char_indices().nth(max).map_or(text.len(), |(i, _)| i);
    text[..end].trim().to_string()
}

/// `(system, user)` para pedir `n` títulos. `lang` = tipo de documento (p. ej. "markdown").
pub fn build_prompt(n: u32, lang: &str, document: &str) -> (String, String) {
    let n = n.clamp(1, 5);
    let snip = snippet(document, SNIPPET_CHARS);
    let count = snip.chars().count();
    let example = match n {
        1 => "[\"título 1\"]".to_string(),
        _ => format!(
            "[{}]",
            (1..=n).map(|i| format!("\"título {i}\"")).collect::<Vec<_>>().join(", ")
        ),
    };
    let user = format!(
        "Propón {n} títulos cortos y descriptivos para el siguiente documento (tipo: {lang}).\n\
Reglas: en el idioma del documento; máximo 6 palabras; sin extensión de archivo; sin fechas;\n\
sin comillas, barras ni caracteres \\ / : * ? \" < > |.\n\
Formato de respuesta: {example}\n\n\
Documento (primeros {count} caracteres):\n<<<\n{snip}\n>>>"
    );
    (SYSTEM_PROMPT.to_string(), user)
}

const ILLEGAL: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Sanea un título para usarlo como nombre de archivo (sin extensión): quita caracteres
/// ilegales y de control, comillas/viñetas, colapsa espacios, trunca a 6 palabras y 60
/// caracteres y evita nombres reservados de Windows y puntos/espacios finales.
pub fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if ILLEGAL.contains(&c) || c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '\'' | '`' | '“' | '”' | '‘' | '’' | '«' | '»' | '*' | '-' | '•' | '#')
    });
    let words: Vec<&str> = cleaned.split_whitespace().take(MAX_WORDS).collect();
    let mut s = words.join(" ");
    if s.chars().count() > MAX_CHARS {
        s = s.chars().take(MAX_CHARS).collect();
        if let Some(pos) = s.rfind(' ')
            && pos > 10
        {
            s.truncate(pos);
        }
    }
    let s = s.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if is_reserved(&s) { format!("{s}_") } else { s }
}

fn is_reserved(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or(s).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn strip_line_decoration(line: &str) -> &str {
    let mut l = line.trim();
    loop {
        let before = l;
        l = l.trim_start_matches(['-', '*', '•', '·', '>', '[']).trim_end_matches(']').trim();
        // "1." / "1)" / "1 -"
        let digits = l.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && digits <= 2 {
            let rest = &l[digits..];
            if let Some(r) = rest.strip_prefix(['.', ')', ':']) {
                l = r.trim_start();
            }
        }
        l = l.trim_matches(['"', '\'', '“', '”', '`', ',']).trim();
        if l == before {
            return l;
        }
    }
}

/// Extrae candidatos de la respuesta cruda: primero el primer `[...]` como JSON; si falla,
/// una línea por candidato (viñetas/numeración/comillas fuera). Devuelve nombres SANEADOS,
/// sin vacíos ni duplicados (sin distinguir mayúsculas), como mucho `max`.
pub fn parse_candidates(raw: &str, max: usize) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut json_ok = false;
    if let (Some(a), Some(b)) = (raw.find('['), raw.rfind(']'))
        && a < b
        && let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(&raw[a..=b])
    {
        json_ok = true;
        items = v
            .into_iter()
            .filter_map(|x| match x {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect();
    }
    if items.is_empty() && !json_ok {
        items = raw
            .lines()
            .map(strip_line_decoration)
            .filter(|l| !l.is_empty() && !matches!(*l, "[" | "]" | "```" | "```json" | "json"))
            .map(str::to_string)
            .collect();
    }
    let mut out: Vec<String> = Vec::new();
    for it in items {
        let s = sanitize_name(&it);
        if !s.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&s)) {
            out.push(s);
        }
        if out.len() >= max {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_is_char_boundary_safe() {
        let s = "áé€😀".repeat(1000);
        let t = snippet(&s, 2000);
        assert_eq!(t.chars().count(), 2000);
        assert_eq!(snippet("  hola  ", 2000), "hola");
        assert_eq!(snippet("abc", 2), "ab");
        assert_eq!(snippet("", 5), "");
    }

    #[test]
    fn prompt_has_rules_and_snippet() {
        let (sys, user) = build_prompt(3, "markdown", "# Notas\nhola");
        assert!(sys.contains("SOLO con JSON"));
        assert!(user.contains("Propón 3 títulos"));
        assert!(user.contains("(tipo: markdown)"));
        assert!(user.contains("[\"título 1\", \"título 2\", \"título 3\"]"));
        assert!(user.contains("<<<\n# Notas\nhola\n>>>"));
        assert!(user.contains("primeros 12 caracteres"));
        let (_, one) = build_prompt(1, "txt", "x");
        assert!(one.contains("[\"título 1\"]"));
        let (_, clamp) = build_prompt(99, "txt", "x");
        assert!(clamp.contains("Propón 5"));
    }

    #[test]
    fn prompt_never_sends_more_than_2000_chars() {
        let big = build_prompt(3, "txt", &"a".repeat(10_000)).1;
        let exact = build_prompt(3, "txt", &"a".repeat(2000)).1;
        assert_eq!(big, exact);
        assert!(big.contains("primeros 2000 caracteres"));
    }

    #[test]
    fn sanitize_removes_illegal_and_limits() {
        assert_eq!(sanitize_name("  \"Plan: Q3/Q4?\"  "), "Plan Q3 Q4");
        assert_eq!(sanitize_name("a\\b|c<d>e*f"), "a b c d e f");
        assert_eq!(sanitize_name("uno dos tres cuatro cinco seis siete ocho"), "uno dos tres cuatro cinco seis");
        assert_eq!(sanitize_name("Título con extensión..."), "Título con extensión");
        assert_eq!(sanitize_name("   "), "");
        assert_eq!(sanitize_name("\"\""), "");
        let long = "abcdefghij ".repeat(2);
        assert!(sanitize_name(&long).chars().count() <= 60);
        let word = "x".repeat(200);
        assert_eq!(sanitize_name(&word).chars().count(), 60);
    }

    #[test]
    fn sanitize_avoids_reserved_names() {
        assert_eq!(sanitize_name("CON"), "CON_");
        assert_eq!(sanitize_name("com1"), "com1_");
        assert_eq!(sanitize_name("Nul"), "Nul_");
        assert_eq!(sanitize_name("Console"), "Console");
        assert_eq!(sanitize_name("COM0"), "COM0");
    }

    #[test]
    fn parse_json_array() {
        let r = parse_candidates(r#"["Notas de reunión", "Plan Q3", "Notas de reunión", ""]"#, 5);
        assert_eq!(r, vec!["Notas de reunión", "Plan Q3"]);
    }

    #[test]
    fn parse_json_inside_prose_and_fences() {
        let raw = "Claro:\n```json\n[\"Uno\", \"Dos\"]\n```\nListo";
        assert_eq!(parse_candidates(raw, 5), vec!["Uno", "Dos"]);
    }

    #[test]
    fn parse_bullets_numbered_and_quoted() {
        let raw = "1. \"Primer título\"\n2) Segundo título\n- Tercero: con dos puntos\n* “Cuarto”\n\n";
        assert_eq!(
            parse_candidates(raw, 5),
            vec!["Primer título", "Segundo título", "Tercero con dos puntos", "Cuarto"]
        );
    }

    #[test]
    fn parse_respects_max_and_sanitizes() {
        let r = parse_candidates(r#"["a/b", "c:d", "e*f"]"#, 2);
        assert_eq!(r, vec!["a b", "c d"]);
        assert!(parse_candidates("", 3).is_empty());
        assert!(parse_candidates("[]", 3).is_empty());
    }

    #[test]
    fn parse_broken_json_falls_back_to_lines() {
        let raw = "[\"Sin cerrar\", \"Otro";
        let r = parse_candidates(raw, 3);
        assert!(!r.is_empty());
        assert!(r.iter().all(|s| !s.contains('"')));
    }
}

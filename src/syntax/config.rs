//! Tokenizadores de configuración: YAML e INI/TOML.
use super::{find, quoted_end, Out, TokKind};

fn is_num_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b':' | b'+' | b'-')
}

fn looks_number(w: &[u8]) -> bool {
    let w = if matches!(w.first(), Some(b'+') | Some(b'-')) { &w[1..] } else { w };
    !w.is_empty() && w[0].is_ascii_digit() && w.iter().all(|&c| is_num_char(c))
}

// ---------------------------------------------------------------- YAML

pub(super) fn scan_yaml(line: &str, state: u16, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let (kind, param) = ((state & 0xff) as u8, (state >> 8) as usize);
    let ind = b.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
    if kind == 1 {
        if ind == n {
            return state;
        }
        if ind > param {
            o.push(0, n, TokKind::String);
            return state;
        }
    }
    if ind == n {
        return 0;
    }
    let mut i = ind;
    if b[i] == b'#' {
        o.push(i, n, TokKind::Comment);
        return 0;
    }
    if ind == 0 && (b.starts_with(b"---") || b.starts_with(b"...")) && b.get(3).is_none_or(|c| *c == b' ') {
        o.push(0, 3, TokKind::Meta);
        i = 3;
    }
    let mut key_col = i;
    // Marcadores de lista.
    loop {
        while i < n && b[i] == b' ' {
            i += 1;
        }
        if i < n && b[i] == b'-' && (i + 1 == n || b[i + 1] == b' ') {
            o.push(i, i + 1, TokKind::Punct);
            i += 1;
            key_col = i + 1;
        } else {
            break;
        }
    }
    if i >= n {
        return 0;
    }
    // ¿Clave?
    let mut key_end = None;
    if b[i] == b'"' || b[i] == b'\'' {
        let e = quoted_end(b, i, b[i]);
        let mut j = e;
        while j < n && b[j] == b' ' {
            j += 1;
        }
        if j < n && b[j] == b':' && (j + 1 == n || b[j + 1] == b' ') {
            key_end = Some((e, j));
        }
    } else if !matches!(b[i], b'[' | b'{' | b'&' | b'*' | b'!' | b'|' | b'>' | b'%' | b'@' | b'`' | b'#') {
        let mut p = i;
        while p < n {
            if b[p] == b':' && (p + 1 == n || b[p + 1] == b' ') {
                break;
            }
            if b[p] == b'#' && p > i && b[p - 1] == b' ' {
                p = n;
                break;
            }
            p += 1;
        }
        if p < n {
            let mut e = p;
            while e > i && b[e - 1] == b' ' {
                e -= 1;
            }
            key_end = Some((e, p));
        }
    }
    if let Some((e, colon)) = key_end {
        key_col = i;
        o.push(i, e, TokKind::Key);
        o.push(colon, colon + 1, TokKind::Punct);
        i = colon + 1;
    }
    // Valor.
    let mut flow = 0i32;
    while i < n {
        let c = b[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'#' if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => {
                o.push(i, n, TokKind::Comment);
                i = n;
            }
            b'"' | b'\'' => {
                let e = if c == b'"' { quoted_end(b, i, b'"') } else { single_end(b, i) };
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'&' | b'*' | b'!' => {
                let mut e = i + 1;
                while e < n && !matches!(b[e], b' ' | b'\t' | b',' | b']' | b'}') {
                    e += 1;
                }
                o.push(i, e, TokKind::Meta);
                i = e;
            }
            b'|' | b'>' => {
                let mut e = i + 1;
                while e < n && matches!(b[e], b'+' | b'-' | b'0'..=b'9') {
                    e += 1;
                }
                let tail_ok = b[e..].iter().all(|d| *d == b' ' || *d == b'\t')
                    || b[e..].iter().position(|d| *d != b' ' && *d != b'\t').is_some_and(|p| b[e + p] == b'#');
                o.push(i, e, TokKind::Punct);
                i = e;
                if tail_ok {
                    // el resto de la línea (comentario) se tokeniza en el siguiente giro
                    let mut st = 1u16 | ((key_col.min(126) as u16) << 8);
                    // Preservar comentario final.
                    while i < n && (b[i] == b' ' || b[i] == b'\t') {
                        i += 1;
                    }
                    if i < n && b[i] == b'#' {
                        o.push(i, n, TokKind::Comment);
                    }
                    if key_col > 126 {
                        st = 0;
                    }
                    return st;
                }
            }
            b'[' | b'{' => {
                flow += 1;
                o.push(i, i + 1, TokKind::Punct);
                i += 1;
            }
            b']' | b'}' => {
                flow -= 1;
                o.push(i, i + 1, TokKind::Punct);
                i += 1;
            }
            b',' if flow > 0 => {
                o.push(i, i + 1, TokKind::Punct);
                i += 1;
            }
            _ => {
                let mut e = i;
                while e < n {
                    if (b[e] == b'#' && e > i && b[e - 1] == b' ') || (flow > 0 && matches!(b[e], b',' | b']' | b'}')) {
                        break;
                    }
                    if flow > 0 && b[e] == b':' && (e + 1 == n || b[e + 1] == b' ') && e > i {
                        break;
                    }
                    e += 1;
                }
                let mut t = e;
                while t > i && (b[t - 1] == b' ' || b[t - 1] == b'\t') {
                    t -= 1;
                }
                let w = &b[i..t];
                let k = if looks_number(w) {
                    TokKind::Number
                } else if matches!(
                    w,
                    b"true" | b"false" | b"null" | b"~" | b"True" | b"False" | b"Null" | b"TRUE" | b"FALSE" | b"NULL"
                        | b"yes" | b"no" | b"on" | b"off" | b"Yes" | b"No"
                ) {
                    TokKind::Constant
                } else {
                    TokKind::String
                };
                o.push(i, t.max(i + 1), k);
                i = t.max(i + 1);
                if flow > 0 && i < n && b[i] == b':' {
                    o.push(i, i + 1, TokKind::Punct);
                    i += 1;
                }
            }
        }
    }
    0
}

fn single_end(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == b'\'' {
            if b.get(j + 1) == Some(&b'\'') {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

// ---------------------------------------------------------------- INI / TOML

/// Estado TOML: 1 = cadena `"""` abierta, 2 = cadena `'''` abierta.
pub(super) fn scan_ini(line: &str, state: u16, o: &mut Out, toml: bool) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let mut i = 0usize;
    if toml && (state & 0xff == 1 || state & 0xff == 2) {
        let pat: &[u8] = if state & 0xff == 1 { b"\"\"\"" } else { b"'''" };
        let mut from = 0;
        loop {
            match find(b, from, pat) {
                Some(p) if state & 0xff == 1 && p > 0 && b[p - 1] == b'\\' => from = p + 1,
                Some(p) => {
                    o.push(0, p + 3, TokKind::String);
                    i = p + 3;
                    break;
                }
                None => {
                    o.push(0, n, TokKind::String);
                    return state & 0xff;
                }
            }
        }
        return value(b, i, o, toml);
    }
    while i < n && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if i == n {
        return 0;
    }
    let c = b[i];
    if c == b'#' || (!toml && c == b';') {
        o.push(i, n, TokKind::Comment);
        return 0;
    }
    if c == b'[' {
        let dbl = b.get(i + 1) == Some(&b'[');
        let close: &[u8] = if dbl { b"]]" } else { b"]" };
        if let Some(p) = find(b, i, close) {
            let e = p + close.len();
            o.push(i, e, TokKind::Meta);
            return value(b, e, o, toml);
        }
        if !toml {
            o.push(i, n, TokKind::Meta);
            return 0;
        }
    }
    // clave = valor
    let mut p = i;
    if b[p] == b'"' || b[p] == b'\'' {
        p = quoted_end(b, p, b[p]);
    }
    while p < n && b[p] != b'=' {
        p += 1;
    }
    if p < n && b[p] == b'=' {
        let mut e = p;
        while e > i && (b[e - 1] == b' ' || b[e - 1] == b'\t') {
            e -= 1;
        }
        o.push(i, e, TokKind::Key);
        o.push(p, p + 1, TokKind::Operator);
        return value(b, p + 1, o, toml);
    }
    value(b, i, o, toml)
}

fn value(b: &[u8], from: usize, o: &mut Out, toml: bool) -> u16 {
    let n = b.len();
    let mut i = from;
    while i < n {
        let c = b[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'#' if toml || i == 0 || b[i - 1] == b' ' => {
                o.push(i, n, TokKind::Comment);
                return 0;
            }
            b';' if !toml && (i == 0 || b[i - 1] == b' ') => {
                o.push(i, n, TokKind::Comment);
                return 0;
            }
            b'"' | b'\'' => {
                if toml && b[i..].starts_with(&[c, c, c]) {
                    let pat = [c, c, c];
                    match find(b, i + 3, &pat) {
                        Some(p) => {
                            o.push(i, p + 3, TokKind::String);
                            i = p + 3;
                        }
                        None => {
                            o.push(i, n, TokKind::String);
                            return if c == b'"' { 1 } else { 2 };
                        }
                    }
                    continue;
                }
                let e = if c == b'"' { quoted_end(b, i, b'"') } else { find(b, i + 1, b"'").map(|p| p + 1).unwrap_or(n) };
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'[' | b']' | b'{' | b'}' | b',' if toml => {
                o.push(i, i + 1, TokKind::Punct);
                i += 1;
            }
            b'=' if toml => {
                o.push(i, i + 1, TokKind::Operator);
                i += 1;
            }
            _ => {
                let mut e = i;
                if toml {
                    while e < n && !matches!(b[e], b' ' | b'\t' | b',' | b']' | b'}' | b'#') {
                        e += 1;
                    }
                } else {
                    while e < n && !((b[e] == b';' || b[e] == b'#') && e > i && b[e - 1] == b' ') {
                        e += 1;
                    }
                    while e > i && (b[e - 1] == b' ' || b[e - 1] == b'\t') {
                        e -= 1;
                    }
                }
                if e <= i {
                    e = i + 1;
                }
                let w = &b[i..e];
                let k = if looks_number(w) {
                    TokKind::Number
                } else if matches!(w, b"true" | b"false") {
                    TokKind::Constant
                } else if toml {
                    TokKind::Text
                } else {
                    TokKind::String
                };
                o.push(i, e, k);
                i = e;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use crate::syntax::tests::*;
    use crate::syntax::{Lang, TokKind::*};

    #[test]
    fn yaml_keys_values() {
        let l = "  name: \"ñandú\" # c";
        assert!(has(Lang::Yaml, l, "name", Key));
        assert!(has(Lang::Yaml, l, "\"ñandú\"", String));
        assert!(has(Lang::Yaml, l, "# c", Comment));
        assert!(has(Lang::Yaml, "port: 8080", "8080", Number));
        assert!(has(Lang::Yaml, "on: true", "true", Constant));
        assert!(has(Lang::Yaml, "url: http://x.y/z", "http://x.y/z", String));
        assert!(has(Lang::Yaml, "- item: 1", "item", Key));
        assert!(has(Lang::Yaml, "# only comment", "# only comment", Comment));
        assert!(has(Lang::Yaml, "anchor: &a value", "&a", Meta));
    }
    #[test]
    fn yaml_block_scalar_state() {
        let r = multi(Lang::Yaml, "msg: |\n  línea ñ\n\n  otra: x\nnext: 1");
        assert_eq!(r[1], vec![("  línea ñ".into(), String)]);
        assert_eq!(r[3], vec![("  otra: x".into(), String)]);
        assert!(r[4].contains(&("next".into(), Key)));
    }
    #[test]
    fn ini_sections_keys() {
        assert!(has(Lang::Ini, "[Server]", "[Server]", Meta));
        let l = "host = localhost ; c";
        assert!(has(Lang::Ini, l, "host", Key));
        assert!(has(Lang::Ini, l, "localhost", String));
        assert!(has(Lang::Ini, l, "; c", Comment));
        assert!(has(Lang::Ini, "; nota", "; nota", Comment));
        assert!(has(Lang::Ini, "port=80", "80", Number));
    }
    #[test]
    fn toml_values_and_multiline() {
        assert!(has(Lang::Toml, "[[bin]]", "[[bin]]", Meta));
        assert!(has(Lang::Toml, "name = \"lm\" # c", "\"lm\"", String));
        assert!(has(Lang::Toml, "n = 1_000", "1_000", Number));
        assert!(has(Lang::Toml, "on = true", "true", Constant));
        assert!(has(Lang::Toml, "d = 1979-05-27T07:32:00Z", "1979-05-27T07:32:00Z", Number));
        let r = multi(Lang::Toml, "s = \"\"\"a\nb ñ\nc\"\"\" # x\nk = 'lit'");
        assert_eq!(r[1], vec![("b ñ".into(), String)]);
        assert!(r[2].contains(&("# x".into(), Comment)));
        assert!(r[3].contains(&("k".into(), Key)));
        assert!(has(Lang::Toml, "\"quoted key\" = 1", "\"quoted key\"", Key));
    }
}

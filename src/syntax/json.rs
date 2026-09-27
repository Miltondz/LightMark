//! Tokenizador JSON (sin estado; admite comentarios `//` de JSONC).
use super::{number_end, quoted_end, Out, TokKind};

pub(super) fn scan(line: &str, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let c = b[i];
        match c {
            b'"' => {
                let e = quoted_end(b, i, b'"');
                let mut j = e;
                while j < n && (b[j] == b' ' || b[j] == b'\t') {
                    j += 1;
                }
                let kind = if j < n && b[j] == b':' { TokKind::Key } else { TokKind::String };
                o.push(i, e, kind);
                i = e;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                o.push(i, n, TokKind::Comment);
                i = n;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                o.push(i, i + 1, TokKind::Punct);
                i += 1;
            }
            b'-' | b'0'..=b'9' => {
                if c == b'-' && !b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    i += 1;
                    continue;
                }
                let e = number_end(b, i + (c == b'-') as usize);
                o.push(i, e, TokKind::Number);
                i = e;
            }
            c if c.is_ascii_alphabetic() => {
                let mut e = i;
                while e < n && b[e].is_ascii_alphanumeric() {
                    e += 1;
                }
                if matches!(&line[i..e], "true" | "false" | "null") {
                    o.push(i, e, TokKind::Constant);
                }
                i = e;
            }
            _ => i += 1,
        }
        // bytes >= 0x80 caen en `_ => i += 1` sólo si no son inicio de token: seguro porque
        // los tokens sólo empiezan en bytes ASCII y los huecos se rellenan como `Text`.
    }
    0
}

#[cfg(test)]
mod tests {
    use crate::syntax::tests::*;
    use crate::syntax::{Lang, TokKind::*};

    #[test]
    fn key_vs_value() {
        let t = toks(Lang::Json, "  \"name\": \"ñandú\", \"n\": -12.5e3, \"ok\": true, \"z\": null");
        assert!(t.contains(&("\"name\"".into(), Key)));
        assert!(t.contains(&("\"ñandú\"".into(), String)));
        assert!(t.contains(&("-12.5e3".into(), Number)));
        assert!(t.contains(&("true".into(), Constant)));
        assert!(t.contains(&("null".into(), Constant)));
    }
    #[test]
    fn key_with_space_before_colon() {
        assert!(has(Lang::Json, "\"a\" : 1", "\"a\"", Key));
    }
    #[test]
    fn escaped_quote_inside_string() {
        let t = toks(Lang::Json, r#""a\"b": "c\\""#);
        assert_eq!(t[0], (r#""a\"b""#.into(), Key));
        assert!(t.iter().any(|(s, k)| s == r#""c\\""# && *k == String));
    }
    #[test]
    fn unicode_utf8_safe() {
        let t = toks(Lang::Json, "\"😀\": [\"漢字\"]");
        assert_eq!(t[0].0, "\"😀\"");
        assert!(has(Lang::Json, "\"😀\": [\"漢字\"]", "\"漢字\"", String));
    }
}

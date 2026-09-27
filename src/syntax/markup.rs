//! Tokenizador XML/HTML por líneas. Estados: 1 comentario, 2 CDATA, 3 dentro de una etiqueta,
//! 4 cuerpo de `<script>`/`<style>` (texto plano), 5/6 valor de atributo con comillas dobles/simples
//! que continúa en la línea siguiente. El parámetro (bits 8..) es 1 si la etiqueta abierta es
//! `script`/`style` (solo HTML).
use super::{find, Out, TokKind};

fn raw_end(b: &[u8], from: usize) -> Option<usize> {
    let mut j = from;
    while let Some(p) = find(b, j, b"</") {
        let rest = &b[p + 2..];
        let ok = |name: &[u8]| rest.len() >= name.len() && rest[..name.len()].eq_ignore_ascii_case(name);
        if ok(b"script") || ok(b"style") {
            return Some(p);
        }
        j = p + 2;
    }
    None
}

fn is_name(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.') || c >= 0x80
}

pub(super) fn scan(line: &str, state: u16, o: &mut Out, html: bool) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let (kind, param) = ((state & 0xff) as u8, state >> 8);
    let mut i = 0usize;
    let mut in_tag = false;
    let mut tagk: u16 = 0;
    let mut closing = false;
    let mut self_close = false;

    match kind {
        1 => match find(b, 0, b"-->") {
            Some(e) => {
                o.push(0, e + 3, TokKind::Comment);
                i = e + 3;
            }
            None => {
                o.push(0, n, TokKind::Comment);
                return 1;
            }
        },
        2 => match find(b, 0, b"]]>") {
            Some(e) => {
                o.push(0, e + 3, TokKind::String);
                i = e + 3;
            }
            None => {
                o.push(0, n, TokKind::String);
                return 2;
            }
        },
        4 => match raw_end(b, 0) {
            Some(p) => i = p,
            None => return 4,
        },
        3 => {
            in_tag = true;
            tagk = param;
        }
        5 | 6 => {
            in_tag = true;
            tagk = param;
            let q = if kind == 5 { b'"' } else { b'\'' };
            match b.iter().position(|&c| c == q) {
                Some(p) => {
                    o.push(0, p + 1, TokKind::String);
                    i = p + 1;
                }
                None => {
                    o.push(0, n, TokKind::String);
                    return state;
                }
            }
        }
        _ => {}
    }

    while i < n {
        let c = b[i];
        if in_tag {
            match c {
                b' ' | b'\t' => i += 1,
                b'>' => {
                    o.push(i, i + 1, TokKind::Punct);
                    i += 1;
                    in_tag = false;
                    if html && !closing && !self_close && tagk == 1 {
                        match raw_end(b, i) {
                            Some(p) => i = p,
                            None => return 4,
                        }
                    }
                }
                b'/' | b'?' => {
                    self_close = c == b'/';
                    o.push(i, i + 1, TokKind::Punct);
                    i += 1;
                }
                b'=' => {
                    o.push(i, i + 1, TokKind::Operator);
                    i += 1;
                }
                b'"' | b'\'' => match b[i + 1..].iter().position(|&d| d == c) {
                    Some(p) => {
                        o.push(i, i + p + 2, TokKind::String);
                        i += p + 2;
                    }
                    None => {
                        o.push(i, n, TokKind::String);
                        return (if c == b'"' { 5 } else { 6 }) | (tagk << 8);
                    }
                },
                _ => {
                    let mut e = i;
                    while e < n && !matches!(b[e], b' ' | b'\t' | b'=' | b'>' | b'/' | b'"' | b'\'' | b'?') {
                        e += 1;
                    }
                    if e == i {
                        e = i + 1;
                    }
                    o.push(i, e, TokKind::Attr);
                    i = e;
                }
            }
            continue;
        }
        match c {
            b'&' => {
                let mut e = i + 1;
                while e < n && (b[e].is_ascii_alphanumeric() || b[e] == b'#') {
                    e += 1;
                }
                if e < n && b[e] == b';' && e > i + 1 {
                    o.push(i, e + 1, TokKind::Entity);
                    i = e + 1;
                } else {
                    i += 1;
                }
            }
            b'<' => {
                let rest = &b[i..];
                if rest.starts_with(b"<!--") {
                    match find(b, i + 4, b"-->") {
                        Some(e) => {
                            o.push(i, e + 3, TokKind::Comment);
                            i = e + 3;
                        }
                        None => {
                            o.push(i, n, TokKind::Comment);
                            return 1;
                        }
                    }
                } else if rest.starts_with(b"<![CDATA[") {
                    match find(b, i + 9, b"]]>") {
                        Some(e) => {
                            o.push(i, e + 3, TokKind::String);
                            i = e + 3;
                        }
                        None => {
                            o.push(i, n, TokKind::String);
                            return 2;
                        }
                    }
                } else if rest.starts_with(b"<?") {
                    let e = find(b, i + 2, b"?>").map(|e| e + 2).unwrap_or(n);
                    o.push(i, e, TokKind::Meta);
                    i = e;
                } else if rest.starts_with(b"<!") {
                    let e = b[i..].iter().position(|&d| d == b'>').map(|p| i + p + 1).unwrap_or(n);
                    o.push(i, e, TokKind::Meta);
                    i = e;
                } else {
                    let slash = rest.get(1) == Some(&b'/');
                    let s = i + 1 + slash as usize;
                    if b.get(s).is_some_and(|&d| d.is_ascii_alphabetic() || d == b'_' || d >= 0x80) {
                        let mut e = s;
                        while e < n && is_name(b[e]) {
                            e += 1;
                        }
                        o.push(i, s, TokKind::Punct);
                        o.push(s, e, TokKind::Tag);
                        let name = &b[s..e];
                        tagk = (name.eq_ignore_ascii_case(b"script") || name.eq_ignore_ascii_case(b"style")) as u16;
                        closing = slash;
                        self_close = false;
                        in_tag = true;
                        i = e;
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    if in_tag { 3 | (tagk << 8) } else { 0 }
}

#[cfg(test)]
mod tests {
    use crate::syntax::tests::*;
    use crate::syntax::{Lang, TokKind::*};

    #[test]
    fn tag_attr_value_entity() {
        let l = "<a href=\"x.html\" data-ñ='y'>é &amp; ü</a>";
        assert!(has(Lang::Xml, l, "a", Tag));
        assert!(has(Lang::Xml, l, "href", Attr));
        assert!(has(Lang::Xml, l, "\"x.html\"", String));
        assert!(has(Lang::Xml, l, "'y'", String));
        assert!(has(Lang::Xml, l, "&amp;", Entity));
        assert!(has(Lang::Xml, l, "<", Punct));
    }
    #[test]
    fn comment_across_lines() {
        let r = multi(Lang::Xml, "<a><!-- c1\nc2 -->\n<b/>");
        assert!(r[0].contains(&("<!-- c1".into(), Comment)));
        assert_eq!(r[1][0], ("c2 -->".into(), Comment));
        assert!(r[2].contains(&("b".into(), Tag)));
    }
    #[test]
    fn cdata_and_pi_and_doctype() {
        let r = multi(Lang::Xml, "<![CDATA[ x\ny ]]><b/>");
        assert_eq!(r[0][0].1, String);
        assert_eq!(r[1][0], ("y ]]>".into(), String));
        assert!(has(Lang::Xml, "<?xml version=\"1.0\"?>", "<?xml version=\"1.0\"?>", Meta));
        assert!(has(Lang::Html, "<!DOCTYPE html>", "<!DOCTYPE html>", Meta));
    }
    #[test]
    fn tag_spanning_lines() {
        let r = multi(Lang::Xml, "<div\n  class=\"a\n b\"\n  id=x>");
        assert!(r[1].contains(&("class".into(), Attr)));
        assert_eq!(r[2][0], (" b\"".into(), String));
        assert!(r[3].contains(&("id".into(), Attr)));
    }
    #[test]
    fn html_script_body_is_plain() {
        let r = multi(Lang::Html, "<script>\nvar a = \"<b>\";\n</script><p>x</p>");
        assert!(r[0].contains(&("script".into(), Tag)));
        assert!(!r[1].iter().any(|(_, k)| *k != Text));
        assert!(r[2].contains(&("script".into(), Tag)));
        assert!(r[2].contains(&("p".into(), Tag)));
    }
}

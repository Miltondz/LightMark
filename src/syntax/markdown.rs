//! Tokenizador Markdown por líneas. Estado: 1 = cerca con ``` abierta, 2 = cerca con ~~~;
//! el parámetro (bits 8..) es la longitud de la cerca de apertura.
use super::{find, Out, TokKind};

fn run_len(b: &[u8], i: usize, c: u8) -> usize {
    b[i..].iter().take_while(|&&d| d == c).count()
}

pub(super) fn scan(line: &str, state: u16, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let (kind, param) = ((state & 0xff) as u8, (state >> 8) as usize);
    let ind = b.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
    let rest = &b[ind..];

    if kind == 1 || kind == 2 {
        let fc = if kind == 1 { b'`' } else { b'~' };
        let r = run_len(b, ind, fc);
        if ind <= 3 + 4 && r >= param.max(3) && b[ind + r..].iter().all(|c| c.is_ascii_whitespace()) {
            o.push(0, n, TokKind::Meta);
            return 0;
        }
        o.push(0, n, TokKind::Code);
        return state;
    }
    if rest.is_empty() {
        return 0;
    }
    // Apertura de cerca.
    for (fc, k) in [(b'`', 1u16), (b'~', 2u16)] {
        let r = run_len(b, ind, fc);
        if r >= 3 && (fc == b'~' || !b[ind + r..].contains(&b'`')) {
            o.push(0, n, TokKind::Meta);
            return k | ((r.min(127) as u16) << 8);
        }
    }
    // Encabezado ATX.
    if ind <= 3 {
        let h = run_len(b, ind, b'#');
        if (1..=6).contains(&h) && (ind + h == n || b[ind + h] == b' ' || b[ind + h] == b'\t') {
            o.push(0, n, TokKind::Heading);
            return 0;
        }
    }
    // Regla horizontal / subrayado setext.
    {
        let t: Vec<u8> = rest.iter().copied().filter(|c| !c.is_ascii_whitespace()).collect();
        if t.len() >= 3 && matches!(t[0], b'-' | b'*' | b'_' | b'=') && t.iter().all(|&c| c == t[0]) {
            o.push(0, n, TokKind::Meta);
            return 0;
        }
    }
    let mut i = ind;
    // Citas.
    while i < n && b[i] == b'>' {
        o.push(i, i + 1, TokKind::Meta);
        i += 1;
        while i < n && b[i] == b' ' {
            i += 1;
        }
    }
    // Marcador de lista.
    if i < n && matches!(b[i], b'-' | b'*' | b'+') && b.get(i + 1) == Some(&b' ') {
        o.push(i, i + 1, TokKind::Meta);
        i += 2;
    } else if i < n && b[i].is_ascii_digit() {
        let d = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if d <= 9 && matches!(b.get(i + d), Some(b'.') | Some(b')')) && b.get(i + d + 1) == Some(&b' ') {
            o.push(i, i + d + 1, TokKind::Meta);
            i += d + 2;
        }
    }
    inline(b, i, o);
    0
}

fn inline(b: &[u8], from: usize, o: &mut Out) {
    let n = b.len();
    let mut i = from;
    while i < n {
        let c = b[i];
        match c {
            b'\\' => i += 2,
            b'`' => {
                let r = run_len(b, i, b'`');
                let mut j = i + r;
                let mut end = None;
                while let Some(p) = find(b, j, b"`") {
                    let rr = run_len(b, p, b'`');
                    if rr == r {
                        end = Some(p + rr);
                        break;
                    }
                    j = p + rr;
                }
                match end {
                    Some(e) => {
                        o.push(i, e, TokKind::Code);
                        i = e;
                    }
                    None => i += r,
                }
            }
            b'*' | b'_' => {
                let r = run_len(b, i, c).min(3);
                let prev_ok = i == 0 || !(c == b'_' && b[i - 1].is_ascii_alphanumeric());
                let next_ok = b.get(i + r).is_some_and(|d| !d.is_ascii_whitespace() && *d != c);
                if prev_ok && next_ok {
                    let pat = vec![c; r];
                    let mut j = i + r;
                    let mut end = None;
                    while let Some(p) = find(b, j, &pat) {
                        let close_ok = !b[p - 1].is_ascii_whitespace()
                            && (c == b'*' || !b.get(p + r).is_some_and(|d| d.is_ascii_alphanumeric()));
                        if p > i + r && close_ok {
                            end = Some(p + r);
                            break;
                        }
                        j = p + 1;
                    }
                    if let Some(e) = end {
                        o.push(i, e, TokKind::Emphasis);
                        i = e;
                        continue;
                    }
                }
                i += run_len(b, i, c);
            }
            b'[' | b'!' => {
                let s = if c == b'!' { i + 1 } else { i };
                if b.get(s) == Some(&b'[') {
                    if let Some(rb) = find(b, s + 1, b"]") {
                        if b.get(rb + 1) == Some(&b'(') {
                            if let Some(rp) = find(b, rb + 2, b")") {
                                o.push(i, rp + 1, TokKind::Link);
                                i = rp + 1;
                                continue;
                            }
                        }
                    }
                }
                i += 1;
            }
            b'<' => {
                if b[i..].starts_with(b"<http") || b[i..].starts_with(b"<mailto:") {
                    if let Some(e) = find(b, i, b">") {
                        o.push(i, e + 1, TokKind::Link);
                        i = e + 1;
                        continue;
                    }
                }
                i += 1;
            }
            b'h' if (b[i..].starts_with(b"http://") || b[i..].starts_with(b"https://"))
                && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) =>
            {
                let mut e = i;
                while e < n && !b[e].is_ascii_whitespace() && !matches!(b[e], b')' | b'>' | b'<' | b'"') {
                    e += 1;
                }
                o.push(i, e, TokKind::Link);
                i = e;
            }
            _ => i += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::tests::*;
    use crate::syntax::{Lang, TokKind::*};

    #[test]
    fn headings_and_hr() {
        assert_eq!(toks(Lang::Markdown, "## Título ñ")[0], ("## Título ñ".into(), Heading));
        assert_eq!(toks(Lang::Markdown, "#nospace")[0].1, Text);
        assert_eq!(toks(Lang::Markdown, "---")[0].1, Meta);
    }
    #[test]
    fn fence_state() {
        let r = multi(Lang::Markdown, "text\n```rust\nlet **x** = 1;\n# not heading\n```\n# real");
        assert_eq!(r[1][0].1, Meta);
        assert_eq!(r[2], vec![("let **x** = 1;".into(), Code)]);
        assert_eq!(r[3], vec![("# not heading".into(), Code)]);
        assert_eq!(r[4][0].1, Meta);
        assert_eq!(r[5][0].1, Heading);
        let r = multi(Lang::Markdown, "~~~~\na\n~~~\nb\n~~~~\nc");
        assert_eq!(r[2][0].1, Code); // cierre más corto que la apertura no cierra
        assert_eq!(r[4][0].1, Meta);
        assert_eq!(r[5][0].1, Text);
    }
    #[test]
    fn inline_elements() {
        let l = "Hola **negrita ñ** y *cursiva* con `código 😀` y [enlace](http://x.y) fin";
        assert!(has(Lang::Markdown, l, "**negrita ñ**", Emphasis));
        assert!(has(Lang::Markdown, l, "*cursiva*", Emphasis));
        assert!(has(Lang::Markdown, l, "`código 😀`", Code));
        assert!(has(Lang::Markdown, l, "[enlace](http://x.y)", Link));
    }
    #[test]
    fn lists_quotes_and_non_emphasis() {
        assert_eq!(toks(Lang::Markdown, "- item *a*")[0], ("-".into(), Meta));
        assert_eq!(toks(Lang::Markdown, "12. dos")[0], ("12.".into(), Meta));
        assert_eq!(toks(Lang::Markdown, "> cita")[0], (">".into(), Meta));
        assert!(!has(Lang::Markdown, "a * b * c", "* b *", Emphasis));
        assert!(!has(Lang::Markdown, "snake_case_name", "_case_", Emphasis));
    }
    #[test]
    fn bare_url() {
        assert!(has(Lang::Markdown, "ver https://ex.com/a?b=1 ok", "https://ex.com/a?b=1", Link));
    }
}

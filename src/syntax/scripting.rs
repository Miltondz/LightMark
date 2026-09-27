//! Tokenizadores de lenguajes de script: Python, Shell (sh/bash) y PowerShell.
use super::{find, is_ident, is_ident_start, number_end, quoted_end, Out, TokKind};

fn punct(b: &[u8], i: usize, o: &mut Out) {
    match b[i] {
        b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',' | b';' | b'.' => o.push(i, i + 1, TokKind::Punct),
        b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^' | b'~' | b'?' | b':'
        | b'@' => o.push(i, i + 1, TokKind::Operator),
        _ => {}
    }
}

fn next_is_call(b: &[u8], e: usize) -> bool {
    b.get(e) == Some(&b'(')
}

// ---------------------------------------------------------------- Python

static PY_KW: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else",
    "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not",
    "or", "pass", "raise", "return", "try", "while", "with", "yield", "match", "case",
];
static PY_TY: &[&str] = &["int", "str", "float", "bool", "list", "dict", "set", "tuple", "bytes", "object", "type"];
static PY_CONST: &[&str] = &["True", "False", "None", "self", "cls"];

/// Estado Python: 1 = `"""` abierta, 2 = `'''` abierta.
pub(super) fn scan_python(line: &str, state: u16, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let kind = (state & 0xff) as u8;
    let mut i = 0usize;
    if kind == 1 || kind == 2 {
        let pat: &[u8] = if kind == 1 { b"\"\"\"" } else { b"'''" };
        let mut from = 0;
        loop {
            match find(b, from, pat) {
                Some(p) if p > 0 && b[p - 1] == b'\\' => from = p + 1,
                Some(p) => {
                    o.push(0, p + 3, TokKind::String);
                    i = p + 3;
                    break;
                }
                None => {
                    o.push(0, n, TokKind::String);
                    return kind as u16;
                }
            }
        }
    }
    let first = b.iter().position(|c| !matches!(c, b' ' | b'\t')).unwrap_or(n);
    let mut after_def: Option<TokKind> = None;
    while i < n {
        let c = b[i];
        if c == b' ' || c == b'\t' {
            i += 1;
            continue;
        }
        if c == b'#' {
            o.push(i, n, TokKind::Comment);
            return 0;
        }
        if c == b'@' && i == first {
            let mut e = i + 1;
            while e < n && (is_ident(b[e]) || b[e] == b'.') {
                e += 1;
            }
            o.push(i, e, TokKind::Meta);
            i = e;
            continue;
        }
        let mut str_from = None; // (inicio de token, posición de la comilla)
        if c == b'"' || c == b'\'' {
            str_from = Some((i, i));
        } else if is_ident_start(c) {
            let mut e = i + 1;
            while e < n && is_ident(b[e]) {
                e += 1;
            }
            let w = &line[i..e];
            if e - i <= 2
                && w.bytes().all(|d| matches!(d, b'r' | b'R' | b'b' | b'B' | b'u' | b'U' | b'f' | b'F'))
                && matches!(b.get(e), Some(b'"') | Some(b'\''))
            {
                str_from = Some((i, e));
            } else {
                let k = if let Some(k) = after_def.take() {
                    after_def = None;
                    k
                } else if PY_KW.contains(&w) {
                    after_def = match w {
                        "def" => Some(TokKind::Function),
                        "class" => Some(TokKind::Type),
                        _ => None,
                    };
                    TokKind::Keyword
                } else if PY_CONST.contains(&w) {
                    TokKind::Constant
                } else if PY_TY.contains(&w) {
                    TokKind::Type
                } else if next_is_call(b, e) {
                    TokKind::Function
                } else if w.as_bytes()[0].is_ascii_uppercase() && w.bytes().any(|d| d.is_ascii_lowercase()) {
                    TokKind::Type
                } else {
                    TokKind::Text
                };
                o.push(i, e, k);
                i = e;
                continue;
            }
        }
        if let Some((s, q)) = str_from {
            let qc = b[q];
            let triple = b[q..].starts_with(&[qc, qc, qc]);
            if triple {
                let pat = [qc, qc, qc];
                let mut from = q + 3;
                loop {
                    match find(b, from, &pat) {
                        Some(p) if b[p - 1] == b'\\' => from = p + 1,
                        Some(p) => {
                            o.push(s, p + 3, TokKind::String);
                            i = p + 3;
                            break;
                        }
                        None => {
                            o.push(s, n, TokKind::String);
                            return if qc == b'"' { 1 } else { 2 };
                        }
                    }
                }
            } else {
                let e = quoted_end(b, q, qc);
                o.push(s, e, TokKind::String);
                i = e;
            }
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let e = number_end(b, i);
            o.push(i, e, TokKind::Number);
            i = e;
            continue;
        }
        punct(b, i, o);
        i += 1;
    }
    0
}

// ---------------------------------------------------------------- Shell

static SH_KW: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "in",
    "function", "select", "return", "exit", "break", "continue", "local", "export", "readonly", "declare",
    "unset", "source", "alias", "set", "shift", "trap", "eval", "exec", "time",
];

pub(super) fn scan_shell(line: &str, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let mut i = 0usize;
    while i < n {
        let c = b[i];
        let tok_start = i == 0 || matches!(b[i - 1], b' ' | b'\t' | b';' | b'&' | b'|' | b'(');
        match c {
            b' ' | b'\t' => i += 1,
            b'#' if tok_start => {
                o.push(i, n, TokKind::Comment);
                return 0;
            }
            b'"' => {
                let e = quoted_end(b, i, b'"');
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'\'' => {
                let e = find(b, i + 1, b"'").map(|p| p + 1).unwrap_or(n);
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'$' => {
                let mut e = i + 1;
                match b.get(e) {
                    Some(b'{') => e = find(b, e, b"}").map(|p| p + 1).unwrap_or(n),
                    Some(b'(') => {
                        o.push(i, i + 2, TokKind::Operator);
                        i += 2;
                        continue;
                    }
                    Some(d) if is_ident_start(*d) => {
                        while e < n && is_ident(b[e]) {
                            e += 1;
                        }
                    }
                    Some(d) if d.is_ascii_digit() || matches!(d, b'@' | b'?' | b'#' | b'$' | b'!' | b'*' | b'-') => {
                        e += 1
                    }
                    _ => {}
                }
                o.push(i, e.max(i + 1), TokKind::Key);
                i = e.max(i + 1);
            }
            b'-' if tok_start && b.get(i + 1).is_some_and(|d| d.is_ascii_alphabetic() || *d == b'-') => {
                let mut e = i + 1;
                while e < n && !matches!(b[e], b' ' | b'\t' | b'=' | b';' | b'|' | b'&' | b')') {
                    e += 1;
                }
                o.push(i, e, TokKind::Attr);
                i = e;
            }
            _ if c.is_ascii_digit() && tok_start => {
                let e = number_end(b, i);
                if e >= n || matches!(b[e], b' ' | b'\t' | b';' | b'|' | b'&' | b')' | b'>' | b'<') {
                    o.push(i, e, TokKind::Number);
                }
                i = e;
            }
            _ if is_ident_start(c) => {
                let mut e = i + 1;
                while e < n && (is_ident(b[e]) || b[e] == b'-' || b[e] == b'.') {
                    e += 1;
                }
                let w = &line[i..e];
                let k = if SH_KW.contains(&w) {
                    TokKind::Keyword
                } else if b.get(e) == Some(&b'=') && tok_start {
                    TokKind::Key
                } else if b.get(e) == Some(&b'(') && b.get(e + 1) == Some(&b')') {
                    TokKind::Function
                } else {
                    TokKind::Text
                };
                o.push(i, e, k);
                i = e;
            }
            _ => {
                match c {
                    b'{' | b'}' | b'[' | b']' | b'(' | b')' => o.push(i, i + 1, TokKind::Punct),
                    b'|' | b'&' | b';' | b'>' | b'<' | b'=' | b'`' | b'!' => o.push(i, i + 1, TokKind::Operator),
                    _ => {}
                }
                i += 1;
            }
        }
    }
    0
}

// ---------------------------------------------------------------- PowerShell

static PS_KW: &[&str] = &[
    "function", "filter", "if", "else", "elseif", "foreach", "for", "while", "do", "until", "switch",
    "param", "begin", "process", "end", "return", "try", "catch", "finally", "throw", "break", "continue",
    "in", "class", "using", "enum", "trap", "exit", "data", "dynamicparam", "workflow", "hidden", "static",
];
static PS_OPS: &[&str] = &[
    "eq", "ne", "gt", "ge", "lt", "le", "like", "notlike", "match", "notmatch", "contains", "notcontains",
    "in", "notin", "and", "or", "not", "xor", "is", "isnot", "as", "replace", "split", "join", "band", "bor",
    "bxor", "bnot", "shl", "shr", "f", "ceq", "cne", "cgt", "clt", "cge", "cle", "clike", "cmatch",
];

/// Estado PowerShell: 1 = `<# #>` abierto, 2 = here-string `@"`, 3 = here-string `@'`.
pub(super) fn scan_powershell(line: &str, state: u16, o: &mut Out) -> u16 {
    let b = line.as_bytes();
    let n = b.len();
    let kind = (state & 0xff) as u8;
    let mut i = 0usize;
    match kind {
        1 => match find(b, 0, b"#>") {
            Some(p) => {
                o.push(0, p + 2, TokKind::Comment);
                i = p + 2;
            }
            None => {
                o.push(0, n, TokKind::Comment);
                return 1;
            }
        },
        2 | 3 => {
            let q = if kind == 2 { b'"' } else { b'\'' };
            if b.len() >= 2 && b[0] == q && b[1] == b'@' {
                o.push(0, 2, TokKind::String);
                i = 2;
            } else {
                o.push(0, n, TokKind::String);
                return kind as u16;
            }
        }
        _ => {}
    }
    while i < n {
        let c = b[i];
        let tok_start = i == 0 || matches!(b[i - 1], b' ' | b'\t' | b'(' | b'{' | b';' | b'|' | b',');
        match c {
            b' ' | b'\t' => i += 1,
            b'<' if b.get(i + 1) == Some(&b'#') => match find(b, i + 2, b"#>") {
                Some(p) => {
                    o.push(i, p + 2, TokKind::Comment);
                    i = p + 2;
                }
                None => {
                    o.push(i, n, TokKind::Comment);
                    return 1;
                }
            },
            b'#' => {
                o.push(i, n, TokKind::Comment);
                return 0;
            }
            b'@' if matches!(b.get(i + 1), Some(b'"') | Some(b'\''))
                && b[i + 2..].iter().all(|d| d.is_ascii_whitespace()) =>
            {
                o.push(i, n, TokKind::String);
                return if b[i + 1] == b'"' { 2 } else { 3 };
            }
            b'"' => {
                let mut j = i + 1;
                while j < n {
                    match b[j] {
                        b'`' => j += 2,
                        b'"' if b.get(j + 1) == Some(&b'"') => j += 2,
                        b'"' => {
                            j += 1;
                            break;
                        }
                        _ => j += 1,
                    }
                }
                let e = j.min(n);
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'\'' => {
                let mut j = i + 1;
                while j < n {
                    if b[j] == b'\'' {
                        if b.get(j + 1) == Some(&b'\'') {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                let e = j.min(n);
                o.push(i, e, TokKind::String);
                i = e;
            }
            b'$' => {
                let mut e = i + 1;
                if b.get(e) == Some(&b'{') {
                    e = find(b, e, b"}").map(|p| p + 1).unwrap_or(n);
                } else if b.get(e) == Some(&b'(') {
                    o.push(i, i + 2, TokKind::Operator);
                    i += 2;
                    continue;
                } else if b.get(e).is_some_and(|d| matches!(d, b'$' | b'?' | b'^')) {
                    e += 1;
                } else {
                    while e < n && (is_ident(b[e]) || b[e] == b':') {
                        e += 1;
                    }
                }
                let e = e.max(i + 1);
                let w = line[i + 1..e].to_ascii_lowercase();
                let k = if matches!(w.as_str(), "true" | "false" | "null") { TokKind::Constant } else { TokKind::Key };
                o.push(i, e, k);
                i = e;
            }
            b'[' if b.get(i + 1).is_some_and(|d| d.is_ascii_alphabetic()) => {
                let mut e = i + 1;
                while e < n && (is_ident(b[e]) || matches!(b[e], b'.' | b',' | b'[' | b']')) && b[e] != b']' {
                    e += 1;
                }
                if e < n && b[e] == b']' && e - i <= 48 {
                    o.push(i, e + 1, TokKind::Type);
                    i = e + 1;
                } else {
                    o.push(i, i + 1, TokKind::Punct);
                    i += 1;
                }
            }
            b'-' if tok_start && b.get(i + 1).is_some_and(|d| d.is_ascii_alphabetic()) => {
                let mut e = i + 1;
                while e < n && (is_ident(b[e]) || b[e] == b'-') {
                    e += 1;
                }
                let w = line[i + 1..e].to_ascii_lowercase();
                let k = if PS_OPS.contains(&w.as_str()) { TokKind::Operator } else { TokKind::Attr };
                o.push(i, e, k);
                i = e;
            }
            _ if c.is_ascii_digit() => {
                let e = number_end(b, i);
                o.push(i, e, TokKind::Number);
                i = e;
            }
            _ if is_ident_start(c) => {
                let mut e = i + 1;
                while e < n && (is_ident(b[e]) || b[e] == b'-') {
                    e += 1;
                }
                // No absorber '-' final.
                while e > i + 1 && b[e - 1] == b'-' {
                    e -= 1;
                }
                let w = &line[i..e];
                let lw = w.to_ascii_lowercase();
                let k = if PS_KW.contains(&lw.as_str()) {
                    TokKind::Keyword
                } else if w.contains('-') && w.as_bytes()[0].is_ascii_uppercase() {
                    TokKind::Function
                } else if next_is_call(b, e) {
                    TokKind::Function
                } else {
                    TokKind::Text
                };
                o.push(i, e, k);
                i = e;
            }
            _ => {
                punct(b, i, o);
                i += 1;
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
    fn python_basics() {
        let l = "def foo(x: int) -> str:  # c";
        assert!(has(Lang::Python, l, "def", Keyword));
        assert!(has(Lang::Python, l, "foo", Function));
        assert!(has(Lang::Python, l, "int", Type));
        assert!(has(Lang::Python, l, "# c", Comment));
        assert!(has(Lang::Python, "class Foo(Base):", "Foo", Type));
        assert!(has(Lang::Python, "x = None", "None", Constant));
        assert!(has(Lang::Python, "@app.route('/')", "@app.route", Meta));
        assert!(has(Lang::Python, "y = f\"a{b} ñ\" + 3.5", "f\"a{b} ñ\"", String));
        assert!(has(Lang::Python, "y = f\"a{b} ñ\" + 3.5", "3.5", Number));
    }
    #[test]
    fn python_triple_quote_state() {
        let r = multi(Lang::Python, "x = \"\"\"doc\nsegunda # no comment\n\"\"\" + y\nz = 1");
        assert!(r[0].contains(&("\"\"\"doc".into(), String)));
        assert_eq!(r[1], vec![("segunda # no comment".into(), String)]);
        assert_eq!(r[2][0], ("\"\"\"".into(), String));
        assert!(r[3].contains(&("1".into(), Number)));
        let r = multi(Lang::Python, "'''a\nb'''");
        assert_eq!(r[1][0], ("b'''".into(), String));
        let r = multi(Lang::Python, "s = '''one line''' # ok");
        assert!(r[0].contains(&("# ok".into(), Comment)));
    }
    #[test]
    fn shell_basics() {
        let l = "if [ -f \"$HOME/x\" ]; then echo ${VAR} $1 # hi";
        assert!(has(Lang::Shell, l, "if", Keyword));
        assert!(has(Lang::Shell, l, "then", Keyword));
        assert!(has(Lang::Shell, l, "\"$HOME/x\"", String));
        assert!(has(Lang::Shell, l, "${VAR}", Key));
        assert!(has(Lang::Shell, l, "$1", Key));
        assert!(has(Lang::Shell, l, "# hi", Comment));
        assert!(has(Lang::Shell, l, "-f", Attr));
        assert!(has(Lang::Shell, "FOO=bar", "FOO", Key));
        assert!(!has(Lang::Shell, "echo a#b", "#b", Comment));
    }
    #[test]
    fn powershell_basics() {
        let l = "function Get-Foo { param([string]$Name) Get-ChildItem -Path $env:PATH -Recurse | Where { $_ -eq $true } # c";
        assert!(has(Lang::PowerShell, l, "function", Keyword));
        assert!(has(Lang::PowerShell, l, "Get-ChildItem", Function));
        assert!(has(Lang::PowerShell, l, "[string]", Type));
        assert!(has(Lang::PowerShell, l, "$Name", Key));
        assert!(has(Lang::PowerShell, l, "$env:PATH", Key));
        assert!(has(Lang::PowerShell, l, "-Path", Attr));
        assert!(has(Lang::PowerShell, l, "-eq", Operator));
        assert!(has(Lang::PowerShell, l, "$true", Constant));
        assert!(has(Lang::PowerShell, l, "# c", Comment));
        assert!(has(Lang::PowerShell, "'it''s'", "'it''s'", String));
    }
    #[test]
    fn powershell_block_comment_and_here_string() {
        let r = multi(Lang::PowerShell, "<# a\nb #> $x = 1\n$s = @\"\nline $x\n\"@\n$y");
        assert_eq!(r[0][0], ("<# a".into(), Comment));
        assert_eq!(r[1][0], ("b #>".into(), Comment));
        assert!(r[1].contains(&("$x".into(), Key)));
        assert_eq!(r[3], vec![("line $x".into(), String)]);
        assert_eq!(r[4][0], ("\"@".into(), String));
        assert!(r[5].contains(&("$y".into(), Key)));
    }
}

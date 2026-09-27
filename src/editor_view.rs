//! Modelo de filas visuales del editor (WS-A): `RowLayout`, `Metrics`, `wrap_line`.
//!
//! Reproduce en Rust exactamente el ajuste de línea (word-wrap) que hace el `TextInput`
//! de Slint, para poder dibujar un gutter de números de línea estilo Sublime (un número
//! solo en la primera fila visual de cada línea lógica) y para posicionar el overlay de
//! resaltado de sintaxis con las mismas coordenadas x/y que usaría `TextInput`.
#![allow(dead_code)]

use std::collections::HashMap;

/// Métricas de fuente medidas a través del `Text` oculto de `CodeEditor`.
pub struct Metrics {
    pub family: String,
    pub size_px: f32,
    pub line_h: f32,
    pub tab_adv: f32,
    /// Ancho de avance para 0x20..=0x7E si TODOS comparten el mismo ancho (fuente monoespaciada
    /// típica); permite un camino rápido para líneas puramente ASCII.
    pub ascii_adv: [f32; 128],
    pub ascii_uniform: Option<f32>,
    pub other: HashMap<char, f32>,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            family: String::new(),
            size_px: 0.0,
            line_h: 0.0,
            tab_adv: 0.0,
            ascii_adv: [0.0; 128],
            ascii_uniform: None,
            other: HashMap::new(),
        }
    }
}

impl Metrics {
    /// Clave que identifica si hace falta reconstruir las métricas (cambió fuente o tamaño).
    pub fn key_matches(&self, family: &str, size_px: f32) -> bool {
        self.family == family && (self.size_px - size_px).abs() < 0.01
    }

    /// Ancho de un carácter ya medido (o 0 si aún no se ha medido; `measure_char` lo rellena).
    pub fn width_of(&self, c: char) -> f32 {
        if c == '\t' {
            return self.tab_adv;
        }
        if (c as u32) < 128 {
            return self.ascii_adv[c as usize];
        }
        *self.other.get(&c).unwrap_or(&0.0)
    }

    /// Mide (con el closure `measure`, que llama a la medición síncrona de Slint) todos los
    /// caracteres ASCII imprimibles + el tabulador, y detecta si la fuente es monoespaciada.
    pub fn init_ascii(&mut self, family: &str, size_px: f32, measure: &mut dyn FnMut(char) -> f32) {
        self.family = family.to_string();
        self.size_px = size_px;
        self.other.clear();
        for c in 0x20u8..=0x7e {
            self.ascii_adv[c as usize] = measure(c as char);
        }
        let a_a = self.ascii_adv[b'a' as usize];
        let uniform = (0x21u8..=0x7e).all(|c| (self.ascii_adv[c as usize] - a_a).abs() < 0.01);
        self.ascii_uniform = if uniform { Some(a_a) } else { None };
        self.tab_adv = measure('\u{1}').max(0.0); // placeholder; sobreescrito abajo por el llamador
    }

    /// Ancho de un carácter fuera de ASCII, midiendo (y cacheando) si hace falta.
    pub fn measure_char(&mut self, c: char, measure: &mut dyn FnMut(char) -> f32) -> f32 {
        if c == '\t' {
            return self.tab_adv;
        }
        if (c as u32) < 128 {
            return self.ascii_adv[c as usize];
        }
        if let Some(&w) = self.other.get(&c) {
            return w;
        }
        let w = measure(c);
        self.other.insert(c, w);
        w
    }

    fn width_str(&mut self, s: &str, measure: &mut dyn FnMut(char) -> f32) -> f32 {
        s.chars().map(|c| self.measure_char(c, measure)).sum()
    }
}

/// Filas visuales por línea lógica (para el gutter con ajuste de línea).
#[derive(Default)]
pub struct RowLayout {
    pub wrap: bool,
    pub wrap_width: f32,
    pub rows_per_line: Vec<u32>,
    /// Longitud `líneas + 1`; `row_prefix[i]` = fila visual (0-based) donde empieza la línea `i`.
    pub row_prefix: Vec<u32>,
}

impl RowLayout {
    pub fn total_rows(&self) -> u32 {
        self.row_prefix.last().copied().unwrap_or(0)
    }

    fn recompute_prefix(&mut self) {
        self.row_prefix.clear();
        self.row_prefix.reserve(self.rows_per_line.len() + 1);
        let mut acc = 0u32;
        self.row_prefix.push(0);
        for &r in &self.rows_per_line {
            acc += r;
            self.row_prefix.push(acc);
        }
    }

    /// Reconstruye por completo a partir de las líneas del `Rope`.
    pub fn rebuild(&mut self, lines: &[&str], m: &mut Metrics, measure: &mut dyn FnMut(char) -> f32) {
        self.rows_per_line.clear();
        self.rows_per_line.reserve(lines.len());
        for line in lines {
            let r = if self.wrap {
                wrap_line(line, self.wrap_width, m, measure).len().max(1) as u32
            } else {
                1
            };
            self.rows_per_line.push(r);
        }
        self.recompute_prefix();
    }

    /// Recalcula solo `[first_line, first_line + old_count)` -> reemplazado por `new_lines`
    /// (longitud `new_count`), y reconstruye el prefijo desde `first_line`.
    pub fn update_lines(
        &mut self,
        first_line: usize,
        old_count: usize,
        new_lines: &[&str],
        m: &mut Metrics,
        measure: &mut dyn FnMut(char) -> f32,
    ) {
        let end = (first_line + old_count).min(self.rows_per_line.len());
        let replacement: Vec<u32> = new_lines
            .iter()
            .map(|line| {
                if self.wrap {
                    wrap_line(line, self.wrap_width, m, measure).len().max(1) as u32
                } else {
                    1
                }
            })
            .collect();
        self.rows_per_line.splice(first_line.min(self.rows_per_line.len())..end, replacement);
        self.recompute_prefix();
    }

    /// `(línea, fila_dentro_de_la_línea)` a partir de una fila visual global (0-based).
    pub fn line_of_row(&self, row: u32) -> (usize, u32) {
        if self.row_prefix.len() <= 1 {
            return (0, row);
        }
        // último índice `i` tal que row_prefix[i] <= row
        let idx = match self.row_prefix.binary_search(&row) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let idx = idx.min(self.row_prefix.len() - 2);
        (idx, row - self.row_prefix[idx])
    }
}

/// Simula el ajuste de línea de `TextInput` (`word-wrap`) para `line` con ancho máximo `max_w`,
/// devolviendo los rangos de bytes (uno por fila visual). Algoritmo (probado contra Slint):
/// se avanza por las oportunidades de ruptura UAX#14 (`unicode_linebreak`); si el segmento
/// (sin espacios finales) no cabe en la fila actual, se cierra la fila; si el segmento por sí
/// solo no cabe nunca, se rompe por carácter ("emergency break"). Los espacios finales de fila
/// cuentan para decidir si rompe, pero no se cuentan para el ancho ya acumulado de la fila
/// siguiente (se comen, igual que hace `TextInput`).
pub fn wrap_line(line: &str, max_w: f32, m: &mut Metrics, measure: &mut dyn FnMut(char) -> f32) -> Vec<(usize, usize)> {
    if line.is_empty() {
        return vec![(0, 0)];
    }
    if max_w <= 0.0 {
        return vec![(0, line.len())];
    }
    let mut segs: Vec<(usize, usize)> = Vec::new();
    let mut prev = 0usize;
    for (i, _) in unicode_linebreak::linebreaks(line) {
        if i > prev {
            segs.push((prev, i));
        }
        prev = i;
    }
    if segs.is_empty() {
        segs.push((0, line.len()));
    }

    let seg_w = |s: &str, m: &mut Metrics, measure: &mut dyn FnMut(char) -> f32| -> f32 {
        s.chars().map(|c| m.measure_char(c, measure)).sum()
    };

    let mut rows: Vec<(usize, usize)> = Vec::new();
    let (mut rs, mut re, mut rw) = (0usize, 0usize, 0f32);
    for (a, b) in segs {
        let seg = &line[a..b];
        let trimmed = seg.trim_end_matches([' ', '\t']);
        let wt = seg_w(trimmed, m, measure);
        let wf = seg_w(seg, m, measure);
        if rw + wt > max_w + 0.01 && re > rs {
            rows.push((rs, re));
            rs = a;
            rw = 0.0;
        }
        if rw + wt > max_w + 0.01 {
            // El segmento (incluso solo) no cabe: rotura de emergencia por carácter.
            let mut cw = rw;
            let mut start = rs.max(a);
            if start > a {
                start = a;
            }
            let mut local_start = a;
            for (ci, ch) in seg.char_indices() {
                let w = m.measure_char(ch, measure);
                if cw + w > max_w + 0.01 && a + ci > local_start {
                    rows.push((local_start, a + ci));
                    local_start = a + ci;
                    cw = 0.0;
                }
                cw += w;
            }
            rs = local_start;
            rw = cw;
            re = b;
            let _ = start;
            continue;
        }
        rw += wf;
        re = b;
    }
    rows.push((rs, re.max(rs)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_metrics() -> Metrics {
        let mut m = Metrics::default();
        // `measure_char` sirve caracteres ASCII directamente desde `ascii_adv` (poblada por
        // `init_ascii`), así que las pruebas deben inicializarla igual que el código real
        // (`ensure_metrics`) en vez de confiar en que el closure `measure` se llame para ASCII.
        let mut meas = |_c: char| 1.0f32;
        m.init_ascii("test", 10.0, &mut meas);
        m.tab_adv = 4.0;
        m
    }
    fn unit_measure() -> impl FnMut(char) -> f32 {
        |_c: char| 1.0
    }

    #[test]
    fn wrap_basic_breaks_at_spaces() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let rows = wrap_line("hello world foo", 6.0, &mut m, &mut meas);
        // "hello " (6 con espacio final trimmeado=5<=6), "world " -> next "foo" etc.
        let texts: Vec<&str> = rows.iter().map(|&(a, b)| &"hello world foo"[a..b]).collect();
        assert_eq!(texts.concat(), "hello world foo");
        assert!(rows.len() >= 2);
    }

    #[test]
    fn wrap_emergency_break_long_word() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let rows = wrap_line("abcdefghij", 4.0, &mut m, &mut meas);
        assert_eq!(rows, vec![(0, 4), (4, 8), (8, 10)]);
    }

    #[test]
    fn wrap_trailing_spaces_do_not_force_extra_row() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        // "abcd" (4) + espacios finales (2) = 6 > max 5, pero trimmed "abcd" = 4 <= 5 => no rompe por eso.
        let rows = wrap_line("abcd  ", 5.0, &mut m, &mut meas);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn wrap_empty_line_one_row() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        assert_eq!(wrap_line("", 100.0, &mut m, &mut meas), vec![(0, 0)]);
    }

    #[test]
    fn wrap_zero_width_no_panic() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let rows = wrap_line("abc", 0.0, &mut m, &mut meas);
        assert_eq!(rows, vec![(0, 3)]);
    }

    #[test]
    fn rebuild_and_update_lines_agree() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let doc = "hello world foo bar\nabcdefghijklmno\nshort\n";
        let mut layout = RowLayout { wrap: true, wrap_width: 5.0, ..Default::default() };
        let lines: Vec<&str> = doc.split('\n').collect();
        layout.rebuild(&lines, &mut m, &mut meas);
        let full = layout.rows_per_line.clone();

        // Ahora reconstruimos incrementalmente: partimos de un layout vacío con solo la 1a línea,
        // y hacemos update_lines para simular una edición que sustituye toda la línea 1 (idx 1..2)
        // por sí misma; debe llegar al mismo resultado final para esa línea.
        let mut layout2 = RowLayout { wrap: true, wrap_width: 5.0, ..Default::default() };
        layout2.rebuild(&lines, &mut m, &mut meas);
        let repl = ["abcdefghijklmno"];
        layout2.update_lines(1, 1, &repl, &mut m, &mut meas);
        assert_eq!(layout2.rows_per_line, full);
    }

    #[test]
    fn update_lines_changing_line_count() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let mut layout = RowLayout { wrap: false, wrap_width: 100.0, ..Default::default() };
        let lines = ["a", "b", "c"];
        layout.rebuild(&lines, &mut m, &mut meas);
        assert_eq!(layout.rows_per_line, vec![1, 1, 1]);
        // Sustituir la línea del medio por dos líneas nuevas.
        layout.update_lines(1, 1, &["x", "y"], &mut m, &mut meas);
        assert_eq!(layout.rows_per_line, vec![1, 1, 1, 1]);
        assert_eq!(layout.total_rows(), 4);
    }

    #[test]
    fn line_of_row_round_trips() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let mut layout = RowLayout { wrap: true, wrap_width: 4.0, ..Default::default() };
        let lines = ["abcdefgh", "x", "yz"];
        layout.rebuild(&lines, &mut m, &mut meas);
        // línea 0 tiene 2 filas (0,1), línea 1 tiene 1 fila (2), línea 2 tiene 1 fila (3).
        assert_eq!(layout.line_of_row(0), (0, 0));
        assert_eq!(layout.line_of_row(1), (0, 1));
        assert_eq!(layout.line_of_row(2), (1, 0));
        assert_eq!(layout.line_of_row(3), (2, 0));
    }

    #[test]
    fn no_wrap_always_one_row_per_line() {
        let mut m = fake_metrics();
        let mut meas = unit_measure();
        let mut layout = RowLayout { wrap: false, wrap_width: 1.0, ..Default::default() };
        let lines = ["a very long line indeed", "b"];
        layout.rebuild(&lines, &mut m, &mut meas);
        assert_eq!(layout.rows_per_line, vec![1, 1]);
    }
}

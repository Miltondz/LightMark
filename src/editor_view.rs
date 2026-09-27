//! Modelo de filas visuales del editor (WS-A): `RowLayout`, `Metrics`, `wrap_line`.
//! Wave 0: solo estructuras vacías; el algoritmo lo rellena WS-A.
#![allow(dead_code)] // wave-1 fills (WS-A)

use std::collections::HashMap;

/// Métricas de fuente medidas a través del `Text` oculto de `CodeEditor`.
#[derive(Default)]
pub struct Metrics {
    pub family: String,
    pub size_px: f32,
    pub line_h: f32,
    pub tab_adv: f32,
    pub ascii_uniform: Option<f32>,
    pub other: HashMap<char, f32>,
}

/// Filas visuales por línea lógica (para el gutter con ajuste de línea).
#[derive(Default)]
pub struct RowLayout {
    pub wrap: bool,
    pub wrap_width: f32,
    pub rows_per_line: Vec<u32>,
    /// Longitud `líneas + 1`.
    pub row_prefix: Vec<u32>,
}

impl RowLayout {
    pub fn total_rows(&self) -> u32 {
        self.row_prefix.last().copied().unwrap_or(0)
    }
}

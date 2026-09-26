# LightMark 📝

Un editor de texto, código y Markdown nativo, ultra-rápido y minimalista para Windows, construido con **Rust** y **Slint GUI**.

Inspirado en la inmediatez y filosofía de **Sublime Text** y la elegancia visual de **VS Code**: apertura instantánea, borradores persistentes automáticos con cero pérdida de datos, y panel de vista previa interactivo en vivo.

---

## ✨ Características Principales

- **Borradores Persistentes Automáticos (Zero Data Loss)**: Cada documento abierto se guarda en segundo plano en tiempo real. Puedes cerrar la aplicación en cualquier momento y al volver todo tu trabajo estará intacto.
- **Vista Previa Integrada Seleccionable y Copiable**:
  - Panel lateral de 2 columnas para previsualizar **Markdown**, **HTML** y **Análisis Léxico/Sintaxis** en vivo.
  - Modo `TextEdit` de solo lectura que permite seleccionar con el ratón cualquier fragmento (palabras, líneas, tablas completas) y copiarlo con `Ctrl+C` o `Ctrl+A`.
  - Botón directo `📋 Copiar` para enviar todo el contenido formateado al portapapeles con un solo clic.
  - Alternancia fluida entre vista de texto seleccionable y vista lista coloreada.
  - Botón `Web` para abrir la vista previa en el navegador predeterminado.
- **Edición Avanzada y Estándares de Editores Modernos**:
  - **Duplicar Línea**: `Ctrl+Shift+D`
  - **Eliminar Línea**: `Ctrl+Shift+K`
  - **Mover Línea Arriba / Abajo**: `Alt+Up` / `Alt+Down`
  - **Comentar / Descomentar**: `Ctrl+/` (detecta automáticamente el lenguaje: `//`, `#`, `--`, `<!-- -->`)
  - **Indentar / Desindentar**: `Tab` / `Shift+Tab` (+4 / -4 espacios)
  - **Unir Líneas**: `Ctrl+J`
  - **Deshacer / Rehacer Ilimitado**: Pila `UndoStack` basada en snapshots inmutables de `Rope` con debouncing para no saturar memoria.
- **Sistema de Pestañas y Grupos**:
  - Pestañas con áreas táctiles independientes para activación y cierre (`×`).
  - Soporte de 1 a 4 grupos de edición (`Alt+1` a `Alt+4`).
- **Barra de Herramientas Minimalista**:
  - Botones compactos con etiquetas claras y legibles, sin emojis corruptos ni dependencias de fuentes externas.
- **Panel de Búsqueda y Reemplazo**:
  - Búsqueda incremental, reemplazar coincidencia actual y reemplazar todas las ocurrencias.
- **Formateador de Código**:
  - Formato y embellecimiento para **JSON**, **XML**, **HTML** y **SQL** con un solo atajo (`Ctrl+Shift+F`).
- **Explorador de Archivos y Proyectos**:
  - Panel lateral desplegable (`Ctrl+B`) para abrir carpetas enteras de trabajo.

---

## ⌨️ Atajos de Teclado

| Atajo | Acción |
|---|---|
| `Ctrl+N` | Nuevo Archivo / Borrador |
| `Ctrl+O` | Abrir Archivo |
| `Ctrl+S` | Guardar (Abre Guardar Como si no tiene archivo asignado) |
| `Ctrl+Shift+S` | Guardar Como... |
| `Ctrl+Shift+A` | Guardar Todo |
| `Ctrl+W` | Cerrar Pestaña Actual |
| `Ctrl+Z` | Deshacer |
| `Ctrl+Y` | Rehacer |
| `Ctrl+C` | Copiar Selección / Copiar Todo |
| `Ctrl+V` | Pegar desde el Portapapeles |
| `Ctrl+Shift+D` | Duplicar Línea Actual |
| `Ctrl+Shift+K` | Eliminar Línea Actual |
| `Alt+Up` | Mover Línea Hacia Arriba |
| `Alt+Down` | Mover Línea Hacia Abajo |
| `Ctrl+/` | Comentar / Descomentar Línea |
| `Ctrl+J` | Unir Líneas |
| `Ctrl+F` | Mostrar / Ocultar Panel de Búsqueda y Reemplazo |
| `Ctrl+Shift+F` | Formatear Documento |
| `Ctrl+M` | Alternar Vista Previa (Markdown / HTML) |
| `Ctrl+D` | Alternar Modo 2 Columnas |
| `Ctrl+B` | Alternar Explorador de Archivos |
| `F11` | Pantalla Completa |
| `Alt+1` .. `Alt+4` | Cambiar entre Grupos de Edición 1 a 4 |

---

## 🚀 Compilación e Instalación

### Requisitos
- Windows 10/11 x64
- Rust (toolchain `stable-x86_64-pc-windows-msvc`)
- Visual Studio C++ Build Tools 2022

### Compilación desde código fuente
```bash
cargo build --release
```
El binario resultante se encontrará en `target\release\lightmark.exe`.

### Pruebas unitarias
```bash
cargo test
```

### Generación del Instalador (Inno Setup)
```bash
iscc installer.iss
```

---

## 📄 Licencia

MIT License — consulta el archivo [LICENSE.txt](LICENSE.txt) para más detalles.
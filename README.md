# LightMark 📝

Un editor de texto, código y Markdown nativo, ultra-rápido y minimalista para Windows, construido con **Rust** y **Slint GUI**.

Inspirado en la inmediatez de **Sublime Text** (borradores persistentes, "hot exit" sin
preguntas, cero pérdida de datos) y la elegancia visual de **VS Code** (menú nativo,
barra de herramientas, panel de vista previa en vivo, explorador de carpetas).

---

## ✨ Características principales

- **Borradores persistentes automáticos (zero data loss)**: cada pestaña sin archivo
  asignado ("Sin título N") se autoguarda sola en segundo plano. Los archivos reales solo
  se guardan con `Ctrl+S` (o automáticamente si activas esa opción en Configuración).
- **"Hot exit"**: cerrar la ventana nunca pregunta nada. Los borradores se autoguardan,
  y el contenido sin guardar de archivos con cambios pendientes queda embebido en la
  sesión para restaurarse como "sucio" (●) la próxima vez que abras LightMark.
- **Tres modos de vista** (botón segmentado de la barra de herramientas, menú Ver o
  `Ctrl+M`):
  - **Solo editor** (0): pantalla completa para el `TextInput`.
  - **Dividido** (1): editor y vista previa lado a lado, 50/50.
  - **Solo vista previa** (2): la vista previa ocupa toda la pestaña.
  - La vista previa (renderizado Markdown/HTML) solo está disponible para esos dos
    lenguajes; para el resto, los botones de vista previa/navegador se deshabilitan y la
    vista vuelve a "Solo editor" sin perder tu preferencia guardada.
  - Cabecera del panel con alternancia **Renderizado / Texto seleccionable**, botón
    **Copiar** (al portapapeles) y **Abrir en navegador** (HTML temporal).
- **Edición consciente de cursor/selección** (nunca del documento entero):
  - Duplicar línea `Ctrl+Shift+D`, eliminar línea `Ctrl+Shift+K`
  - Mover línea arriba/abajo `Alt+↑` / `Alt+↓`
  - Comentar/descomentar `Ctrl+/` (detecta el lenguaje: `//`, `#`, `--`, `<!-- -->`, `;`)
  - Indentar/desindentar `Tab` / `Shift+Tab` (respeta tamaño de tabulación y
    espacios-vs-tabs de Configuración)
  - Unir líneas `Ctrl+J`
  - Deshacer/rehacer con snapshots de `Rope` y restauración de cursor (`Ctrl+Z` /
    `Ctrl+Y`, sin doble-deshacer)
- **Pestañas y grupos**: hasta 4 grupos independientes de pestañas (`Alt+1`…`Alt+4`),
  barra de pestañas con scroll horizontal, indicador de "sucio" (●), tooltip con la ruta
  completa, cierre con confirmación solo para archivos reales con cambios sin guardar.
- **Búsqueda y reemplazo** incremental, con mayúsculas/minúsculas, palabra completa y
  expresión regular; `F3` / `Shift+F3` para saltar entre coincidencias; reemplazar una o
  todas con un único paso de deshacer.
- **Formateador de código** para JSON, XML, HTML y SQL (`Ctrl+Shift+F`).
- **Explorador de carpetas** (`Ctrl+B`): navega subcarpetas, sube a la carpeta padre,
  abre archivos con un clic.
- **Exportar borradores a ZIP** (ZIP real, con CRC-32 propio, verificable con cualquier
  descompresor).
- **Asociación de archivos**: `lightmark.exe archivo.md` (o abrir un `.md`/`.txt` desde el
  Explorador de Windows tras instalar) abre ese archivo directamente.

---

## ⌨️ Atajos de teclado

| Categoría | Acción | Atajo |
|---|---|---|
| Archivo | Nuevo | `Ctrl+N` |
| Archivo | Abrir… | `Ctrl+O` |
| Archivo | Abrir carpeta… | `Ctrl+Shift+O` |
| Archivo | Guardar | `Ctrl+S` |
| Archivo | Guardar como… | `Ctrl+Shift+S` |
| Archivo | Guardar todo | `Ctrl+Alt+S` |
| Archivo | Cerrar pestaña | `Ctrl+W` |
| Archivo | Configuración… | `Ctrl+,` |
| Editar | Deshacer / Rehacer | `Ctrl+Z` / `Ctrl+Y` |
| Editar | Cortar / Copiar / Pegar / Seleccionar todo | `Ctrl+X` / `Ctrl+C` / `Ctrl+V` / `Ctrl+A` |
| Editar | Buscar / Reemplazar | `Ctrl+F` / `Ctrl+H` |
| Editar | Buscar siguiente / anterior | `F3` / `Shift+F3` |
| Editar | Formatear documento | `Ctrl+Shift+F` |
| Selección | Duplicar línea | `Ctrl+Shift+D` |
| Selección | Eliminar línea | `Ctrl+Shift+K` |
| Selección | Mover línea arriba/abajo | `Alt+↑` / `Alt+↓` |
| Selección | Comentar/descomentar | `Ctrl+/` |
| Selección | Indentar / Desindentar | `Tab` / `Shift+Tab` |
| Selección | Unir líneas | `Ctrl+J` |
| Ver | Explorador | `Ctrl+B` |
| Ver | Ajuste de línea | `Alt+Z` |
| Ver | Alternar vista previa | `Ctrl+M` |
| Ver | Acercar / Alejar / Restablecer zoom | `Ctrl+=` / `Ctrl+-` / `Ctrl+0` |
| Ver | Pantalla completa | `F11` |
| Ir | Ir a línea… | `Ctrl+G` |
| Ir | Pestaña siguiente / anterior | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Ir | Grupo 1 a 4 | `Alt+1` … `Alt+4` |
| Ayuda | Atajos de teclado | `F1` |

(Esta tabla coincide exactamente con los `shortcut: @keys(...)` declarados en
`ui/app.slint` y con el diálogo "Atajos de teclado" de la propia app.)

---

## 🚀 Compilación e instalación

### Requisitos
- Windows 10/11 x64
- Rust (toolchain `stable-x86_64-pc-windows-msvc`)
- **Visual Studio Build Tools 2022** (componente "Desktop development with C++"): el
  enlazador de MSVC necesita las variables de entorno de `vcvars64.bat`. Si `cargo build`
  falla al enlazar desde una terminal normal, ejecuta antes:
  ```bat
  call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
  ```
  y compila dentro de esa misma sesión de terminal.

### Compilación desde código fuente
```bash
cargo build --release
```
El binario resultante se encontrará en `target\release\lightmark.exe`.

### Pruebas unitarias
```bash
cargo test
```

### Generación del instalador (Inno Setup)
```bash
iscc installer.iss
```

---

## 🗂️ Ubicación de datos

Toda la configuración y los borradores viven en:

```
%LOCALAPPDATA%\LightMark\
├── settings.json        Configuración persistente
├── scratch\              Borradores autoguardados (uno por pestaña "Sin título N")
│   └── scratch-index.json
└── sessions\
    └── auto-restore.json Sesión automática ("hot exit")
```

Puedes abrir esta carpeta directamente desde **Configuración → Abrir carpeta de datos**.

---

## 📄 Licencia

MIT License — consulta el archivo [LICENSE.txt](LICENSE.txt) para más detalles.

# LightMark 📝

Editor de texto, código y Markdown nativo para Windows, construido con **Rust** y **Slint GUI**. Rápido de abrir, minimalista en apariencia, y diseñado para que nunca pierdas trabajo: todo se autoguarda en segundo plano.

Inspirado en la inmediatez de **Sublime Text** (borradores persistentes, "hot exit" sin preguntas, varias ventanas con pestañas movibles entre ellas) y la elegancia visual de **VS Code** (menú nativo, barra de herramientas con iconos, resaltado de sintaxis, panel de vista previa en vivo).

---

## ✨ Características principales

### Edición
- **Resaltado de sintaxis** en el propio editor (no solo en la vista previa) para JSON, XML, HTML, SQL, JavaScript/TypeScript, CSS, Rust, Python, TOML, YAML, Shell/PowerShell, INI, C/C++/C#/Java/Go/Lua y Markdown.
- **Números de línea siempre visibles**, estilo Sublime — también con ajuste de línea activado (el número aparece solo en la primera línea visual de cada línea lógica).
- **Edición consciente de cursor/selección** (nunca del documento entero): duplicar línea, eliminar línea, mover línea arriba/abajo, comentar/descomentar (detecta el marcador según el lenguaje: `//`, `#`, `--`, `<!-- -->`, `;`), indentar/desindentar (respeta tamaño de tabulación y espacios-vs-tabs de Configuración), unir líneas.
- **Deshacer/rehacer** por comandos discretos (cada operación es un paso independiente), con restauración de cursor.
- **Búsqueda y reemplazo** incremental, con mayúsculas/minúsculas, palabra completa y expresión regular (incluye grupos `$1`/`${1}`); `F3` / `Shift+F3` para saltar entre coincidencias.
- **Formateador de código** para JSON, XML, HTML y SQL, sin alterar el contenido (solo reindenta).
- Tamaño de fuente ajustable con iconos A+/A- o `Ctrl++`/`Ctrl+-`, zoom persistente.

### Cero pérdida de datos
- **Borradores persistentes automáticos**: cada pestaña sin archivo asignado ("Sin título N") se autoguarda sola en segundo plano. Los archivos reales solo se guardan con `Ctrl+S`, o automáticamente si activas esa opción en Configuración.
- **"Hot exit"**: cerrar la ventana nunca pregunta nada. Los borradores se autoguardan y el contenido sin guardar de archivos con cambios pendientes queda embebido en la sesión, para restaurarse como "sucio" (●) la próxima vez que abras LightMark — incluso con varias ventanas abiertas a la vez.
- **Nombre automático de borradores**: se deriva del contenido (primer encabezado o primera línea), con fecha opcional en el nombre propuesto (formato `dd-mm-aaaa` o `aaaa-mm-dd`, configurable). El identificador interno del borrador nunca cambia; el nombre es solo metadato.
- **Exportar borradores a ZIP** (ZIP real, con CRC-32 propio, verificable con cualquier descompresor).

### Ventanas, pestañas y vistas
- **Varias ventanas a la vez** (`Ctrl+Shift+N`), cada una con sus propias pestañas y grupos. Mover una pestaña a otra ventana, o a una ventana nueva, desde el menú Archivo. Abrir un archivo ya abierto en otra ventana activa esa ventana en vez de duplicarlo. Al cerrar una ventana con borradores o cambios sin guardar, se ofrece moverlos a otra ventana antes de perderlos.
- **Pestañas y grupos**: hasta 4 grupos independientes de pestañas (`Alt+1`…`Alt+4`), barra con scroll horizontal, títulos recortados con "…" (nunca se superponen), reordenar arrastrando, tooltip con la ruta completa, numeración de "Sin título N" que reutiliza los números libres.
- **Tres modos de vista** (botón segmentado de la barra de herramientas, menú Ver o `Ctrl+M`): solo editor, dividido (editor y vista previa lado a lado), o solo vista previa. Disponible únicamente para Markdown y HTML; para el resto de lenguajes los controles se deshabilitan sin perder la preferencia guardada.
- **Explorador de carpetas** (`Ctrl+B`): navega subcarpetas, sube a la carpeta padre, abre archivos con un clic.
- **Temas**: Oscuro y **Océano** (degradado azul petróleo), con cambio instantáneo desde el icono de la barra de herramientas o Configuración.

### Inteligencia artificial (opcional)
- **Sugerir nombre con archivo con IA**: genera hasta 5 propuestas de nombre a partir del contenido del documento, usando el modelo más económico configurado.
- **Nombrado automático de borradores**: opcional, renombra solos los borradores inactivos tras unos segundos sin cambios (sin diálogo, respetando cualquier nombre que ya hayas puesto).
- **Proveedores soportados**: Google Gemini, OpenRouter, NVIDIA Build, OpenAI, Anthropic, Groq, Mistral, DeepSeek, xAI, Ollama, LM Studio, y cualquier endpoint compatible con OpenAI.
- **La clave de API se guarda en el Administrador de credenciales de Windows** — nunca en `settings.json`, en logs ni en mensajes de error. Solo se envía contenido del documento al proveedor cuando tú disparas una acción de IA (consentimiento explícito en Configuración → Configurar IA).

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
| Archivo | Nueva ventana | `Ctrl+Shift+N` |
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
| Ver | Aumentar / reducir / restablecer tamaño de fuente | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |
| Ver | Pantalla completa | `F11` |
| Ir | Ir a línea… | `Ctrl+G` |
| Ir | Pestaña siguiente / anterior | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Ir | Grupo 1 a 4 | `Alt+1` … `Alt+4` |
| Ayuda | Atajos de teclado | `F1` |

(Esta tabla coincide con los `shortcut: @keys(...)` declarados en `ui/app.slint` y con el diálogo "Atajos de teclado" de la propia app. Acciones sin atajo fijo — mover pestaña entre ventanas, comentar en un formateador de lenguaje sin comentarios, sugerir nombre con IA — están en sus menús correspondientes.)

---

## 🚀 Compilación e instalación

### Requisitos
- Windows 10/11 x64
- Rust (toolchain `stable-x86_64-pc-windows-msvc`)
- **Visual Studio Build Tools 2022** (componente "Desktop development with C++"): el enlazador de MSVC necesita las variables de entorno de `vcvars64.bat`. Si `cargo build` falla al enlazar desde una terminal normal, ejecuta antes:
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

Toda la configuración, los borradores y las sesiones viven en:

```
%LOCALAPPDATA%\LightMark\
├── settings.json          Configuración persistente (nunca contiene claves de API)
├── instance.lock          Bloqueo de instancia única (un solo proceso, N ventanas)
├── scratch\                Borradores autoguardados (uno por pestaña "Sin título N")
│   └── scratch-index.json
└── sessions\
    └── auto-restore.json   Sesión automática ("hot exit"), con todas las ventanas abiertas
```

Las claves de API de los proveedores de IA se guardan aparte, en el **Administrador de credenciales de Windows** (`cmdkey /list` las muestra como entradas `LightMark/<proveedor>`), nunca en esta carpeta.

Puedes abrir la carpeta de datos directamente desde **Configuración → Abrir carpeta de datos**.

---

## 📄 Licencia

MIT License — consulta el archivo [LICENSE.txt](LICENSE.txt) para más detalles.

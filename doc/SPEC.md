# Especificación integral — Editor nativo ultraligero de texto y Markdown
## Documento de producto, UX/UI, arquitectura técnica y guía de implementación para un agente AI

**Estado:** Especificación base para desarrollo  
**Fecha:** 2026-09-24  
**Nombre:** TBD — nombre provisional `LightMark` utilizado únicamente dentro de este documento  
**Plataforma primaria:** Windows 10/11 x64 en la primera entrega  
**Arquitectura:** Rust nativo, preparada para macOS/Linux  
**Principio rector:** editor profesional, extremadamente rápido, visualmente minimalista y orientado a manejar tanto documentos reales como una gran cantidad de notas temporales.

---

# 0. INSTRUCCIONES DIRECTAS PARA EL AGENTE DE DESARROLLO

Este documento es la fuente principal de requisitos del proyecto. El agente debe tratarlo como una especificación de producto + arquitectura + criterios de aceptación.

## 0.1 Prioridades

Orden de prioridad obligatorio:

1. Corrección y conservación de datos.
2. Fluidez de edición y navegación.
3. Rendimiento con archivos grandes.
4. UX limpia y predecible.
5. Markdown Raw/View/Split.
6. Scratch/Inbox/Archive.
7. Explorador de carpetas y workspaces.
8. Search/Replace.
9. Formateadores.
10. IA opcional.
11. Funciones secundarias.

Nunca sacrificar las primeras prioridades para agregar funcionalidades adicionales.

## 0.2 Restricciones

No utilizar como arquitectura principal:

- Electron.
- Chromium embebido.
- una aplicación web empaquetada.
- una base de datos pesada para cada documento.
- un parser completo ejecutado sobre todo el archivo después de cada pulsación.
- un render de todas las líneas del documento.
- un servicio cloud obligatorio.
- una API key compartida por la aplicación.
- telemetría obligatoria.

La aplicación debe funcionar completamente offline para edición, navegación, búsqueda local, Markdown, formateado local y Scratch/Archive.

La IA es opcional y debe poder desactivarse completamente.

## 0.3 Regla de implementación

No declarar una funcionalidad como terminada porque "funciona" con documentos pequeños. Toda funcionalidad relacionada con texto debe probarse también con los fixtures de archivos grandes definidos en este documento.

Cada milestone debe cerrar con:

```text
cargo fmt --all
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

y los benchmarks asociados al milestone.

---

# 1. VISIÓN DEL PRODUCTO

La aplicación será un editor de texto/Markdown de escritorio de estilo profesional, inspirado en la eficiencia de Sublime Text, pero construido alrededor de una necesidad adicional:

> El usuario puede abrir muchas pestañas y documentos temporales durante el desarrollo, escribir información sin querer invertir tiempo en clasificarlos y, posteriormente, convertir todo ese trabajo en un conjunto organizado de archivos sin perder información.

Por tanto, el producto no debe intentar eliminar el comportamiento de "abrir pestañas para anotar cosas". Debe diseñarse para soportarlo.

El flujo principal es:

```text
Abrir
  ↓
Escribir rápidamente
  ↓
Abrir más tabs / grupos
  ↓
Trabajar
  ↓
No perder nada por falta de guardado
  ↓
Park / Archive
  ↓
Generar nombres automáticos
  ↓
Guardar como carpeta o ZIP
  ↓
Cerrar la sesión
  ↓
Restaurar posteriormente si es necesario
```

La aplicación debe sentirse como una herramienta de escritorio nativa, no como una web app.

---

# 2. POSICIONAMIENTO

La descripción corta del producto debe ser:

> Editor nativo ultraligero para texto, código y Markdown, diseñado para trabajar rápidamente con muchos documentos y convertir el caos de pestañas temporales en archivos recuperables y organizados.

No debe posicionarse como:

- IDE.
- sustituto completo de un IDE.
- procesador de textos.
- plataforma colaborativa.
- gestor de proyectos.
- aplicación de notas basada en nube.

El usuario principal puede ser desarrollador, diseñador técnico, escritor técnico o cualquier persona que trabaje con texto estructurado.

---

# 3. OBJETIVOS

## 3.1 Objetivos funcionales

La primera versión debe permitir:

- editar texto.
- editar Markdown.
- visualizar Markdown renderizado.
- trabajar en Raw / View / Split.
- abrir múltiples pestañas.
- usar 1–4 grupos de editor.
- buscar.
- reemplazar.
- utilizar regex.
- navegar rápidamente.
- abrir carpetas.
- explorar archivos.
- abrir y editar SQL.
- abrir y editar HTML, JavaScript, JSON, XML y TXT.
- resaltar sintaxis.
- formatear JSON, XML, HTML y SQL.
- manejar Scratch documents con autosave.
- crear un Inbox de documentos temporales.
- archivar tabs como carpeta o ZIP.
- restaurar sesiones.
- detectar cambios externos.
- guardar de forma segura.
- usar IA de forma opcional para nombrar/organizar/resumir/transformar.

## 3.2 Objetivos no funcionales

La aplicación debe:

- iniciar rápidamente.
- utilizar poca memoria cuando trabaja con documentos pequeños/medios.
- no degradar severamente al tener muchas pestañas.
- abrir archivos grandes sin congelar la UI.
- permitir navegación rápida dentro de archivos SQL de miles o millones de líneas.
- procesar syntax highlighting incrementalmente.
- mantener la UI reactiva durante búsqueda/indexación/formateo.
- guardar de forma atómica.
- no perder documentos Scratch por un cierre inesperado.
- funcionar sin conexión para todo lo que no necesite IA.

---

# 4. NO OBJETIVOS DE LA V1

No implementar en la primera versión:

- LSP.
- autocompletado semántico completo.
- debugger.
- terminal integrada.
- Git UI.
- colaboración.
- sincronización cloud.
- edición multiusuario.
- plugins de terceros.
- gestor de proyectos complejo.
- soporte de decenas de lenguajes.
- navegador web embebido.
- chat AI permanente.
- vector database.
- indexing cloud.
- publicación directa a Internet.

La arquitectura debe permitir agregarlos después sin contaminar el núcleo.

---

# 5. PLATAFORMAS

## V1

Windows 10/11 x64.

## Arquitectura desde el inicio

Evitar APIs exclusivas de Windows salvo la integración del sistema operativo necesaria para:

- filesystem.
- credenciales.
- menú contextual.
- notifications opcionales.
- integración de ventana.

El código de dominio debe mantenerse multiplataforma.

## Futuro

macOS y Linux deben ser posibles sin reescribir el core.

---

# 6. STACK TÉCNICO RECOMENDADO

## 6.1 Lenguaje

Rust estable.

No utilizar nightly salvo que una dependencia lo exija temporalmente y exista una justificación documentada.

## 6.2 UI

**Slint** es la opción principal recomendada.

Razones:

- UI nativa declarativa.
- integración directa con Rust.
- enfoque cross-platform.
- menor dependencia de runtime web.
- adecuado para una aplicación pequeña.
- las versiones actuales están mejorando específicamente el manejo de grandes cantidades de texto.

Como referencia de actualidad, Slint 1.18 fue publicado el 16 de septiembre de 2026 y documenta mejoras en compilación, binarios y rendimiento de renderizado/edición de grandes cantidades de texto.

La elección debe validarse con un spike de editor de texto antes de congelar la arquitectura.

## 6.3 Buffer de texto

No diseñar el editor alrededor de un `String` gigante mutable.

Definir una abstracción:

```rust
trait TextBuffer {
    fn len_bytes(&self) -> u64;
    fn len_lines(&self) -> u64;
    fn line(&self, line: u64) -> BufferSlice;
    fn slice(&self, range: ByteRange) -> BufferSlice;
    fn insert(&mut self, pos: u64, text: &str) -> Result<()>;
    fn delete(&mut self, range: ByteRange) -> Result<()>;
    fn iter_chunks(&self, range: Option<ByteRange>) -> ChunkIterator;
}
```

Backend recomendado:

### Normal/medium

Rope o estructura equivalente.

`ropey` es una referencia válida para este backend. Proporciona estructura rope, soporte Unicode, operaciones por líneas y construcción incremental desde streams grandes.

### Large/Huge

El backend debe poder cambiar a un modelo paginado/piece-table/chunked buffer que no exija cargar todo el archivo a RAM.

Arquitectura recomendada:

```text
Original file
    ↓
Memory mapped / paged source
    +
append-only edited chunks
    ↓
Piece map / block rope
    ↓
logical document
```

El `TextBuffer` debe ocultar la implementación.

No aceptar una arquitectura donde abrir un archivo de 1 GB implique crear obligatoriamente un `String` de 1 GB + copias para parsing + copias para UI.

## 6.4 Syntax parsing

Usar Tree-sitter o una arquitectura equivalente para syntax structure.

Tree-sitter es especialmente apropiado porque proporciona parsing incremental y mantiene un árbol actualizable.

No usar un parser global en cada pulsación.

## 6.5 Markdown

Parser Markdown local.

Baseline recomendado:

- CommonMark.
- tablas.
- task lists.
- strikethrough.
- footnotes opcionales.
- YAML frontmatter opcional.

`pulldown-cmark` es un candidato válido.

Los offsets del parser deben permitir enlazar el bloque renderizado con el rango fuente.

## 6.6 SQL

`sqlparser` es un candidato recomendado para parsing/formateado estructural.

No depender de un único dialecto. El sistema debe modelar:

```text
SQL dialect:
- Generic / ANSI
- PostgreSQL
- MySQL
- SQLite
- SQL Server
```

No es requisito de V1 implementar semántica completa de cada dialecto, pero sí permitir seleccionar un dialecto para parsing/formateado cuando sea necesario.

## 6.7 JSON

`serde_json`.

Para documentos normales puede parsearse a estructura para validación/formateado.

Para documentos grandes, evitar cargar todo el archivo solo para pintar syntax.

## 6.8 XML

`quick-xml`.

Es especialmente apropiado para archivos grandes porque dispone de API de lectura/escritura streaming.

## 6.9 HTML

`html5ever` o parser equivalente.

La V1 necesita principalmente:

- syntax highlighting.
- navegación de tags.
- matching de tags cuando sea viable.
- formato local.

No necesita un navegador/preview web embebido.

## 6.10 Search

Para workspace:

- `ignore` para recorrer carpetas respetando `.gitignore`/`.ignore`.
- motor tipo ripgrep/grep-searcher o implementación equivalente.

ripgrep debe tomarse como referencia de diseño: utiliza el motor regex de Rust, optimizaciones, memoria mapeada cuando es conveniente y búsqueda incremental cuando es mejor para árboles grandes.

Para el documento abierto, implementar un buscador que opere sobre el `TextBuffer`.

## 6.11 File watching

`notify` o equivalente para detectar cambios externos.

## 6.12 Archivos ZIP

`zip` como baseline.

El exportador debe estar desacoplado del workspace para poder sustituir la librería si fuera necesario.

## 6.13 Persistencia

Usar:

- `serde`
- `serde_json`

para configuración, sesiones e índices de Scratch.

No utilizar una base de datos para el núcleo de la aplicación en V1.

## 6.14 Secret storage

Usar `keyring` o capa equivalente que utilice:

- Windows Credential Manager.
- macOS Keychain.
- Linux Secret Service.

Nunca almacenar una API key de IA en texto plano por defecto.

## 6.15 HTTP para AI

La capa AI puede usar `ureq` o un cliente HTTP ligero equivalente.

La dependencia de red debe ser opcional desde el punto de vista arquitectónico.

---

# 7. MODELO DE VENTANA

Por defecto la aplicación abre con:

**1 solo grupo.**

No mostrar dos, tres o cuatro columnas hasta que el usuario las active.

```text
┌─────────────────────────────────────────────────────────────┐
│ tabs                                                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                       EDITOR                                │
│                                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ status                                                      │
└─────────────────────────────────────────────────────────────┘
```

## Grupos disponibles

Máximo visible inicialmente:

- 1 grupo.
- 2 grupos.
- 3 grupos.
- 4 grupos.

Cada grupo contiene:

- tabs.
- editor.
- opcionalmente Raw/View/embedded preview.
- scrollbar.

## Comportamiento

Debe permitir:

- dividir.
- cerrar grupo.
- mover tab entre grupos.
- duplicar tab en otro grupo.
- cambiar orientación si se decide añadirlo posteriormente.
- recordar layout en sesiones.

Atajos:

```text
Ctrl+1  = 1 grupo
Ctrl+2  = 2 grupos
Ctrl+3  = 3 grupos
Ctrl+4  = 4 grupos
```

Los grupos no deben ocupar espacio cuando están desactivados.

---

# 8. TABS

Las tabs son uno de los elementos principales.

Cada tab puede representar:

- archivo real.
- Scratch.
- documento recuperado.
- documento archivado abierto.
- documento temporal sin path.

Estados visuales:

```text
Normal
Modified
Scratch
Pinned
Read-only
Large file
External change
```

El tab modificado debe tener un indicador discreto, no un gran icono.

Debe soportar:

- cierre.
- cierre del resto.
- cierre de tabs a la derecha.
- reabrir último tab.
- fijar.
- mover.
- duplicar.
- revelar archivo.
- copiar path.
- guardar como.
- convertir Scratch en archivo real.

---

# 9. RAW MODE

Raw es el editor de texto real.

El usuario debe poder escribir Markdown directamente.

Syntax highlighting obligatorio para:

- Markdown.
- HTML.
- JavaScript.
- SQL.
- JSON.
- XML.
- TXT.

TXT no tiene syntax highlighting semántico, pero mantiene:

- números de línea.
- selección.
- matching de brackets cuando corresponda.
- búsqueda.

## Markdown Raw

No renderizar el documento.

Mostrar Markdown como fuente, pero aplicar colores y jerarquía visual.

Ejemplo:

```markdown
# Título

Texto con **negrita** y *cursiva*.

- Item
- Item

[Link](https://example.com)
```

Debe verse como código fuente legible, con:

- headings diferenciados.
- delimitadores Markdown menos prominentes.
- links claramente reconocibles.
- bloques de código con colores.
- listas.
- quotes.
- tablas.

No usar exceso de colores.

---

# 10. VIEW MODE

View muestra Markdown renderizado.

Debe ocultar:

- `#`.
- `**`.
- `*`.
- delimitadores de código Markdown.
- sintaxis estructural.

Debe renderizar:

- headings.
- párrafos.
- listas.
- task lists.
- links.
- imágenes.
- blockquotes.
- tablas.
- code blocks.
- separadores.
- footnotes si están habilitados.

## Navegación Raw ↔ View

Al hacer clic en un elemento de View, cuando sea posible:

```text
View block
    ↓
source range
    ↓
Raw cursor/selection
```

Al hacer clic en un heading en Raw:

```text
source heading
    ↓
View scroll
```

---

# 11. SPLIT MODE

Split debe mostrar:

```text
RAW                       VIEW
─────────────────         ─────────────────

# Title                   Title

Text **bold**             Text bold

- One                     • One
- Two                     • Two
```

Debe existir sincronización de scroll aproximada/semántica.

No exigir sincronización perfecta en documentos sintácticamente inválidos; debe utilizar el mejor anchor disponible.

---

# 12. SEARCH

Buscar:

```text
Ctrl+F
```

Debe tener:

- texto literal.
- case sensitive.
- whole word.
- regex.
- wrap.
- siguiente/anterior.
- número de coincidencias.
- búsqueda en selección.

Resultado visual:

```text
Search: timeout
3 / 27 matches
```

Debe permitir:

- Enter = siguiente.
- Shift+Enter = anterior.
- Escape = cerrar panel manteniendo selección.

---

# 13. REPLACE

```text
Ctrl+H
```

Campos:

```text
Find
Replace
```

Opciones:

- literal.
- regex.
- case sensitive.
- whole word.
- current selection.
- current file.
- workspace.

Acciones:

- Replace.
- Replace & Find Next.
- Replace All.

Antes de Replace All debe existir confirmación cuando la operación afecta muchos documentos.

---

# 14. SEARCH EN WORKSPACE

La aplicación debe poder buscar una expresión dentro de la carpeta abierta.

Resultados:

```text
SEARCH RESULTS

42 matches in 8 files

src/database.sql
    1201  SELECT ...
    1299  SELECT ...

docs/API.md
     42  database timeout

config.json
     18  timeout
```

Al seleccionar resultado:

- abrir archivo.
- mover cursor.
- seleccionar match.
- mantener resultados abiertos hasta nueva búsqueda.

Debe respetar `.gitignore` por defecto, con opción:

```text
Search hidden files
Search ignored files
```

---

# 15. NAVEGACIÓN PROFESIONAL

Implementar:

## Quick Open

`Ctrl+P`

Busca:

- archivos.
- carpetas.
- tabs.
- Scratch.
- sesiones recientes.

## Go to Line

`Ctrl+G`

Permite:

```text
123
123:42
```

## Go to Heading

Especialmente útil para Markdown.

## Go to Symbol

Inicialmente limitado a estructuras que puedan obtenerse localmente:

- headings Markdown.
- funciones JavaScript si el parser lo permite.
- tags HTML.
- objetos JSON de nivel superior.
- statements SQL seleccionables.

No es un LSP.

---

# 16. MÚLTIPLES CURSORES Y EDICIÓN

Base tipo Sublime:

- múltiples cursores.
- selección múltiple.
- selección de línea.
- selección por columna.
- duplicar línea.
- mover línea.
- borrar línea.
- unir líneas.
- indent/outdent.
- comentar selección cuando el lenguaje lo soporte.
- copiar path.
- auto pair:
  - `()`
  - `[]`
  - `{}`
  - `""`
  - `''`
  - backticks.

Matching visual:

```text
(
)
```

y:

```html
<div>
</div>
```

cuando sea seguro detectarlo.

---

# 17. FORMATEADORES

Comando principal:

```text
Shift+Alt+F
```

Comandos relacionados:

```text
Format Document
Format Selection
Format on Save
```

## JSON

Debe convertir:

```json
{"name":"Ana","roles":["admin","dev"]}
```

en:

```json
{
  "name": "Ana",
  "roles": [
    "admin",
    "dev"
  ]
}
```

Configuración:

- indentation.
- spaces/tabs.
- sort keys opcional.
- trailing newline.

No ordenar keys por defecto.

## XML

Debe ajustar:

- indentation.
- nesting.
- atributos.
- self-closing tags cuando sea seguro.

Preservar semántica.

Nunca hacer una transformación destructiva silenciosa.

## HTML

Formatear:

- nesting.
- indentation.
- tags.
- atributos cuando sea seguro.

No convertir HTML a una estructura diferente solo para "verse bonito".

## SQL

Formatear:

- keywords.
- SELECT fields.
- FROM.
- JOIN.
- ON.
- WHERE.
- GROUP BY.
- HAVING.
- ORDER BY.
- INSERT/UPDATE/DELETE.
- subqueries.

Configuración:

```text
Keyword case:
  Uppercase
  Lowercase
  Preserve

Indent:
  2
  4

Comma style:
  Leading / Trailing
```

## JavaScript

V1 puede ofrecer syntax highlighting y edición. El formateador JS puede quedar detrás de una implementación basada en parser/formatter confiable y desacoplada del core. No inventar un formatter incompleto.

---

# 18. ARCHIVOS GRANDES

Esta sección es crítica.

La aplicación debe diseñarse para abrir SQL con miles, cientos de miles o millones de líneas sin convertir el proceso de edición en una operación pesada.

## Principio

La UI nunca necesita:

```text
render(file)
```

Debe hacer:

```text
render(viewport)
```

## Virtualización

Solo renderizar:

```text
visible lines
+
small overscan
```

Ejemplo:

```text
visible: 40 lines
overscan: 100 lines
```

No crear elementos UI para cientos de miles de líneas.

## Line index

Implementar un índice jerárquico/sparse:

```text
line checkpoint every N lines
byte offsets
```

Propuesta:

```text
checkpoint every 4096 lines
```

Después, realizar scan local desde el checkpoint para obtener la línea exacta.

## Indexación progresiva

Al abrir un archivo grande:

```text
Opened
Indexing lines...
```

El editor debe ser utilizable antes de completar el índice.

Mientras el índice no está completo:

- scroll aproximado.
- búsqueda incremental.
- Ctrl+G puede forzar indexing hasta el punto requerido.
- no bloquear la UI.

## Scrollbar

Debe mostrar un thumb estable.

Si el índice exacto todavía no está completo, puede utilizar estimación.

## Large File Mode

Activación automática según tamaño y/o coste estimado.

Este modo debe:

- evitar AST completo.
- limitar syntax analysis al viewport y regiones relevantes.
- evitar preview completa innecesaria.
- utilizar búsqueda streaming/chunked.
- mantener edición.
- mantener save.

## No hard limit

No imponer un máximo artificial de:

```text
10 MB
50 MB
100 MB
```

La aplicación debe soportar archivos cuyo tamaño solo está limitado por el entorno operativo.

Sin embargo, debe mostrar un modo Large/Extreme para proteger memoria.

---

# 19. OBJETIVOS DE RENDIMIENTO

Los números siguientes son objetivos de ingeniería y deben validarse con benchmarks reales.

## Edición normal

Objetivo:

- input → visual feedback P95 menor a ~20 ms en máquina de referencia.
- scroll sin pausas perceptibles.
- tab switching inmediato para documentos normales.

## Apertura

Objetivo de UX:

- archivo pequeño: usable prácticamente inmediatamente.
- archivo de 100 MB: UI disponible sin esperar un parse completo.
- archivo de 500 MB: acceso temprano a contenido.
- 1 GB+: acceso temprano y navegación progresiva.

Los tiempos exactos se deben registrar en benchmark, no asumir.

## Memoria

Registrar:

```text
RSS after idle
RSS after 10 tabs
RSS after 100 tabs
RSS with 100 MB file
RSS with 500 MB file
RSS with 1 GB file
```

La arquitectura no debe duplicar innecesariamente el archivo en:

- buffer.
- parser.
- syntax tokens.
- render tree.
- undo snapshot.

## Tabs

Probar:

- 20.
- 50.
- 100.
- 250 Scratch documents.

La aplicación no debe degradar severamente solo por la cantidad de tabs.

---

# 20. ENCODING Y LINE ENDINGS

Soporte prioritario:

- UTF-8.
- UTF-8 BOM.
- UTF-16 LE/BE cuando sea viable.
- detección segura.
- preservación del encoding original al guardar cuando sea posible.

Line endings:

- LF.
- CRLF.
- CR.

Mostrar:

```text
LF
CRLF
```

en status bar.

No normalizar line endings automáticamente.

Opción:

```text
Convert line endings
```

---

# 21. DETECCIÓN DE TIPO DE ARCHIVO

Por defecto usar:

1. extensión.
2. contenido cuando sea necesario.
3. override manual.

Extensiones:

```text
.md / markdown
.html / htm
.js / mjs / cjs
.sql
.json
.xml
.txt
```

El usuario debe poder forzar lenguaje manualmente desde status bar.

Ejemplo:

```text
Plain Text ▾
```

---

# 22. EXPLORADOR DE ARCHIVOS

Sidebar opcional:

```text
EXPLORER

▼ project
  ▼ src
    app.js
    database.sql
  ▼ docs
    README.md
  package.json
  config.json
```

Debe iniciar oculto.

Atajo:

```text
Ctrl+Shift+E
```

Funciones:

- open file.
- open folder.
- new file.
- new folder.
- rename.
- delete.
- reveal in system file explorer.
- copy path.
- copy relative path.
- open in new tab.
- refresh.
- collapse all.
- expand folder.

No mostrar panel gigante por defecto.

---

# 23. WORKSPACE

Un workspace es opcional.

Se puede abrir:

```text
File → Open Folder
```

y después navegar.

Guardar workspace debe recordar:

- root folder.
- tabs.
- grupos.
- layout.
- active tab.
- cursor positions.
- scroll positions.
- language overrides.
- pinned tabs.

Debe utilizar un archivo de estado independiente del contenido del proyecto.

No modificar el proyecto del usuario agregando archivos de metadata salvo que se le indique.

---

# 24. SESSION

Las sesiones representan un contexto de trabajo.

Ejemplo:

```text
2026-09-24 13:42 — Development
```

Guardar:

```json
{
  "version": 1,
  "name": "Development",
  "created_at": "...",
  "groups": [
    {
      "id": 0,
      "tabs": [
        {
          "document_id": "...",
          "path": "...",
          "cursor": 1204,
          "scroll_line": 140
        }
      ]
    }
  ],
  "active_group": 0
}
```

Nunca guardar contenido completo de archivos reales dentro de `session.json`.

Para Scratch, usar archivos separados.

---

# 25. SCRATCH SYSTEM

Esta es una característica de producto central.

## Nuevo Scratch

`Ctrl+N` puede crear un Scratch.

No debe aparecer inmediatamente:

```text
Save As...
```

El documento se guarda automáticamente en almacenamiento local de Scratch.

Ejemplo conceptual:

```text
Scratch #1
Scratch #2
Scratch #3
```

Aunque el usuario cierre la aplicación, deben recuperarse.

## Storage

Separar:

```text
Application Data/
  scratch/
    <document-id>.md
  sessions/
    <session-id>.json
  metadata/
    scratch-index.json
```

No mezclar este almacenamiento con proyectos del usuario.

## Conversión a archivo

Comando:

```text
Save As
```

convierte Scratch → archivo real.

---

# 26. INBOX

La UI debe proporcionar una sección opcional:

```text
INBOX
  17 Scratch documents
```

No es una carpeta normal.

Es una colección de documentos temporales que esperan clasificación.

Mostrar:

- nombre actual.
- fecha.
- última modificación.
- preview corto.
- lenguaje detectado.
- tamaño.
- estado.

---

# 27. PARK

"Park" significa:

> conservar el documento de forma segura para dejar de cargarlo mentalmente en las tabs activas.

Acciones:

```text
Park
Park & Close
Park Selected
Park All Scratch
```

Park no debe borrar.

Debe cambiar el estado del documento a:

```text
Inbox / Archived
```

---

# 28. ARCHIVE

Comando principal:

```text
Park & Archive
```

Debe permitir:

```text
Destination:
  Default archive location
  Choose folder
```

Salida:

```text
Archives/
  2026-09-24/
    13-42 Development/
      01-oauth-login.md
      02-database-timeout.md
      03-query.sql
      04-ui-idea.md
      session.json
```

## Nombre automático

Prioridades:

1. Heading principal.
2. nombre inferido del contenido.
3. keyword dominante.
4. tipo.
5. IA si está habilitada y el usuario lo permite.
6. fallback seguro.

Nunca generar nombres vacíos.

## Conflictos

Si ya existe:

```text
database-timeout.md
```

usar:

```text
database-timeout-2.md
```

o timestamp configurable.

---

# 29. ARCHIVE COMO ZIP

Al exportar:

```text
Save as:
  Folder
  ZIP
```

El ZIP debe contener:

```text
archive-name/
  01-name.md
  02-name.sql
  ...
  session.json
  manifest.json
```

`manifest.json` debe incluir:

- versión del formato.
- fecha.
- documentos.
- tipos.
- original scratch IDs.
- hashes opcionales.
- metadata mínima.

Nunca incluir API keys.

---

# 30. MERGE DE SCRATCH DOCUMENTS

Seleccionar:

```text
note-1
note-2
note-3
```

Comando:

```text
Merge
```

Resultado:

```text
combined-notes.md
```

Separar documentos con:

```markdown
---

# Original: note-1
...
```

En V1 puede usar reglas deterministas.

Una futura capa AI puede estructurar el contenido.

---

# 31. REVIEW

Función opcional:

```text
Review Inbox
```

Abrir documentos uno a uno y permitir:

```text
Keep
Archive
Save as file
Merge
Delete
Skip
```

La UI debe evitar que Review se convierta en una tarea administrativa pesada.

---

# 32. AUTOSAVE Y RECUPERACIÓN

## Archivos reales

Autosave opcional.

No sobrescribir directamente de forma insegura.

Proceso:

```text
write temp
flush
fsync cuando corresponda
atomic replace
```

## Scratch

Autosave obligatorio.

Debe escribir cambios después de un pequeño debounce.

Ejemplo:

```text
300–1000 ms
```

configurable.

## Crash recovery

Al arrancar:

```text
Recovered documents
```

con:

```text
Restore all
Review
Discard
```

Nunca borrar automáticamente un Scratch modificado si existe duda.

---

# 33. CAMBIOS EXTERNOS

Si el archivo fue modificado externamente:

```text
This file changed outside the editor.

[Reload]
[Compare]
[Keep my changes]
```

No sobrescribir automáticamente el cambio externo.

En conflicto:

```text
External version
Current version
```

Mostrar diff.

---

# 34. DIFF

V1 puede proporcionar un diff sencillo local:

```text
OLD
- timeout = 30

NEW
+ timeout = 60
```

Usos:

- cambio externo.
- comparar versiones.
- historial local.
- review.

---

# 35. HISTORIAL LOCAL

Mantener opcionalmente versiones recientes de Scratch y archivos editados.

Debe tener límites de espacio.

Configuración:

```text
History:
  Off
  1 day
  7 days
  30 days
  90 days
```

No sustituye Git.

---

# 36. COMMAND PALETTE

`Ctrl+Shift+P`

Debe ser una de las principales herramientas de navegación.

Ejemplos:

```text
> New Scratch
> Open File
> Open Folder
> Toggle Explorer
> Toggle Outline
> Split Right
> Split Left
> One Group
> Two Groups
> Three Groups
> Four Groups
> Toggle Raw
> Toggle View
> Toggle Split
> Format Document
> Format Selection
> Search in Workspace
> Park Tab
> Park All
> Archive Session
> Restore Session
> Theme: Graphite
> Theme: Ivory
> Theme: Aurora
> AI: Rename Document
> AI: Summarize Selection
```

Debe buscar comandos fuzzy.

---

# 37. QUICK ACTIONS

Cuando haya una selección, una pequeña toolbar opcional puede aparecer:

```text
B   I   S   Code   Link   Quote   List
```

Debe ser discreta y no permanente.

---

# 38. MARKDOWN QUICK EDIT

Atajos:

```text
Ctrl+B  -> **selection**
Ctrl+I  -> *selection*
Ctrl+K  -> [selection](url)
```

Otros:

```text
Ctrl+Shift+8 -> code block / depende de conflicto de plataforma
```

No imponer atajos ambiguos; todos deben ser configurables.

---

# 39. OUTLINE

Sidebar opcional:

```text
DOCUMENT

Introduction
Architecture
Database
API
Roadmap
```

Para Markdown:

- headings H1–H6.

Para JS:

- funciones cuando se puedan identificar localmente.

Para HTML:

- estructura de headings/tags seleccionados.

Para SQL:

- statements principales cuando el parser pueda identificarlos.

Click:

```text
outline item
    ↓
cursor + scroll
```

---

# 40. TEMAS VISUALES

La V1 debe incluir tres estilos.

## 40.1 Graphite

Personalidad:

- oscuro.
- profesional.
- técnico.
- sobrio.

Tokens base sugeridos:

```text
Background        #111315
Surface           #171A1D
Surface Elevated  #1D2125
Border            #292E33
Text              #E9ECEF
Text Secondary    #A2A9B0
Text Muted        #727A82
Accent            #7AA2F7
Selection         #30415E
Cursor            #DCE7FF
```

## 40.2 Ivory

Personalidad:

- claro.
- editorial.
- cómodo para escritura.

```text
Background        #F7F4EE
Surface           #FFFDF9
Surface Elevated  #FFFFFF
Border            #DDD8CE
Text              #262626
Text Secondary    #5F5C57
Text Muted        #858078
Accent            #4F6B5B
Selection         #DCE7D9
Cursor            #27342C
```

## 40.3 Aurora

Personalidad:

- oscuro.
- moderno.
- tecnológico.
- con carácter, sin apariencia gamer.

```text
Background        #0D1117
Surface           #131922
Surface Elevated  #19202B
Border            #252E3A
Text              #E8EDF5
Text Secondary    #A8B2C0
Text Muted        #748091
Accent            #8B7CFF
Accent Secondary  #5ED6E8
Selection         #30304C
Cursor            #EEE9FF
```

Los colores son tokens orientativos y deben ajustarse mediante pruebas de contraste.

---

# 41. ACCESIBILIDAD VISUAL

Objetivos:

- texto normal con buen contraste.
- estados no comunicados solo mediante color.
- focus visible.
- selección claramente visible.
- no depender exclusivamente de rojo/verde.
- soporte para escalado UI.

Configuraciones:

```text
UI Scale
Editor Font Size
Editor Line Height
Sidebar Font Size
Tab Size
```

---

# 42. DENSIDAD

Tres niveles:

```text
Compact
Comfortable
Spacious
```

Default:

```text
Comfortable
```

Compact debe aproximarse a la densidad de un editor profesional técnico.

---

# 43. LAYOUT

La aplicación debe tener:

```text
Top bar
Tabs
Editor area
Optional sidebars
Status bar
```

No agregar un dashboard grande.

No introducir tarjetas, widgets o paneles de productividad en la vista de edición.

La pantalla vacía debe seguir siendo minimalista.

---

# 44. TIPOGRAFÍA

No empaquetar fuentes externas obligatoriamente en V1.

Usar:

- fuente UI del sistema.
- fuente monospace del sistema por defecto.

Permitir seleccionar fuente del editor.

Soportar:

- tamaño.
- peso cuando sea posible.
- line height.
- ligaduras opcionales.

---

# 45. STATUS BAR

Debe mostrar información útil sin ruido.

Ejemplo:

```text
UTF-8   LF   Markdown       Ln 142, Col 18       1,482 words
```

Cuando corresponda:

```text
Read Only
Large File
Indexing 72%
```

El lado derecho debe reservar espacio para estado contextual.

---

# 46. GESTIÓN DE IMÁGENES EN MARKDOWN

Arrastrar una imagen sobre un Markdown debe poder insertar:

```markdown
![description](relative/path/image.png)
```

Preferencia:

- guardar en ruta relativa.
- nunca convertir automáticamente a base64 salvo comando explícito.

Clipboard:

```text
Paste Image
```

debe permitir crear un archivo en una carpeta `assets`.

---

# 47. DRAG AND DROP

Comportamiento:

### Archivo

Abrir.

### Carpeta

Abrir como workspace.

### Imagen sobre Markdown

Insertar Markdown image.

### Texto

Insertar contenido o abrir como documento, según origen.

Arrastrar tab entre grupos debe cambiar su pertenencia.

---

# 48. MANEJO DE MUCHOS TABS

No crear una representación pesada por cada tab.

Cada tab debe contener referencias ligeras:

```text
DocumentHandle
ViewState
```

La UI debe cargar recursos derivados cuando sea necesario.

Ejemplo:

```text
Tab metadata
   ├── document_id
   ├── title
   ├── status
   ├── group
   └── view state
```

Los buffers de documentos pueden mantenerse calientes si caben en un presupuesto de memoria y descargarse/reconstruirse cuando no.

No perder cambios al hacer eviction.

---

# 49. MODELO DE DOCUMENTO

Propuesta:

```rust
struct Document {
    id: DocumentId,
    path: Option<PathBuf>,
    kind: DocumentKind,
    language: Language,
    encoding: Encoding,
    line_ending: LineEnding,
    buffer: Box<dyn TextBuffer>,
    dirty: bool,
    read_only: bool,
    scratch: bool,
    external_state: ExternalState,
    history: UndoManager,
}
```

`DocumentId` debe ser estable durante la sesión.

---

# 50. MODELO DE VIEW

```rust
struct ViewState {
    cursor: Position,
    selections: Vec<Selection>,
    scroll_top: LogicalPosition,
    soft_wrap: bool,
    zoom: f32,
}
```

Guardar esto dentro de sesión.

---

# 51. MODELO DE GRUPO

```rust
struct EditorGroup {
    id: GroupId,
    tabs: Vec<DocumentView>,
    active: usize,
}
```

Un máximo de 4 grupos visibles en V1.

---

# 52. MODELO DE WORKSPACE

```rust
struct Workspace {
    root: Option<PathBuf>,
    groups: Vec<EditorGroup>,
    explorer_state: ExplorerState,
    recent_files: Vec<PathBuf>,
}
```

No incluir contenido completo de documentos.

---

# 53. PERSISTENCIA DE CONFIGURACIÓN

Config:

```text
settings.json
```

contiene:

- theme.
- UI scale.
- editor settings.
- keybindings.
- formatter settings.
- autosave settings.
- scratch settings.
- archive root.
- AI provider configuration sans secrets.
- recent workspaces.

La API key queda fuera de este JSON.

---

# 54. CONFIGURACIÓN AI

Ejemplo:

```json
{
  "enabled": true,
  "provider": "gemini",
  "model": "user-selected",
  "send_mode": "ask"
}
```

Nunca:

```json
{
  "api_key": "..."
}
```

---

# 55. AI PROVIDER ABSTRACTION

Definir:

```rust
trait AiProvider {
    fn name(&self) -> &'static str;
    fn models(&self) -> Result<Vec<Model>>;
    fn generate(&self, request: AiRequest) -> Result<AiResponse>;
}
```

Providers:

```text
None
Gemini
Future: OpenAI
Future: OpenRouter
Future: Local model
```

La UI no debe saber detalles específicos de Gemini.

---

# 56. GEMINI BYOK

El usuario puede introducir su propia credencial.

Debe:

1. validarla.
2. almacenarla en OS secure storage.
3. nunca loguearla.
4. nunca incluirla en crash reports.
5. nunca incluirla en exports.
6. nunca incluirla en Git.
7. permitir eliminarla.

La implementación debe consultar la documentación vigente de Google al momento de integrar la API. A fecha de este documento, Google documenta una transición de standard API keys hacia auth keys durante 2026; el agente debe verificar el esquema vigente antes de escribir el cliente final.

---

# 57. SEGURIDAD AI

La aplicación debe advertir:

```text
AI requests send selected content to the configured provider.
```

Nunca enviar el documento completo sin una acción explícita cuando el usuario solo seleccionó un fragmento.

Modos:

```text
Selection only
Current document
Current tab group
Ask every time
```

Default:

```text
Selection only
```

para operaciones de texto.

Para Rename del Scratch puede enviar el documento completo, pero debe ser una acción explícita del usuario al configurar AI naming.

---

# 58. AI NAMING

Entrada:

```text
document content
language
existing filename if any
```

Salida obligatoria estructurada:

```json
{
  "filename": "database-timeout",
  "extension": ".md",
  "confidence": 0.91
}
```

No aceptar texto libre como resultado final.

Validar:

- caracteres inválidos.
- longitud.
- extensión.
- path traversal.
- nombres reservados.
- duplicados.

Fallback si AI falla:

```text
heading-based naming
keyword naming
timestamp fallback
```

La IA jamás debe ser necesaria para archivar.

---

# 59. AI ACTIONS V1

Implementar primero:

1. Rename document.
2. Summarize selection.
3. Rewrite selection.
4. Extract TODOs.
5. Extract ideas.
6. Organize scratch documents.
7. Merge selected documents.
8. Convert notes into Markdown structure.
9. Explain selection.

Nunca ejecutar acciones destructivas directamente.

Las transformaciones deben presentarse como:

```text
Preview
Apply
Cancel
```

---

# 60. AI ORGANIZE SCRATCH

Dado:

```text
30 Scratch documents
```

la IA puede proponer:

```text
Authentication
Database
UI
Deployment
Misc
```

Resultado estructurado obligatorio:

```json
{
  "groups": [
    {
      "name": "Database",
      "documents": ["id1", "id7"]
    }
  ]
}
```

La aplicación debe permitir revisar antes de mover.

---

# 61. AI Y PRIVACIDAD

No ejecutar AI:

- en cada pulsación.
- al abrir cualquier archivo.
- automáticamente sin indicador.

Mostrar estado cuando haga una petición:

```text
AI naming...
```

Nunca bloquear la edición.

AI requests deben ejecutarse fuera del hilo de UI.

---

# 62. UI THREAD

Regla estricta:

El hilo de UI jamás debe realizar directamente:

- lectura completa de archivo grande.
- indexación.
- search en workspace.
- parse largo.
- format large file.
- ZIP creation.
- AI network request.
- autosave pesado.

Todos van a background workers.

---

# 63. JOB SYSTEM

Crear un pequeño sistema de jobs con:

```text
Job
CancellationToken
Progress
Result
```

Ejemplos:

```text
OpenFileJob
IndexLinesJob
SearchWorkspaceJob
FormatDocumentJob
ArchiveJob
AiRequestJob
```

Debe permitir cancelar:

```text
Search...
Format...
Archive...
```

cuando sea razonable.

---

# 64. ERROR HANDLING

Nunca hacer:

```rust
unwrap()
```

en código de producción para operaciones de usuario.

Usar errores tipados.

Mensajes legibles:

```text
Could not save file.
The file may have been changed by another program.
```

y opción:

```text
Retry
Save As
Compare
Cancel
```

Los logs técnicos van a un archivo de diagnóstico, nunca al UI salvo que sean útiles.

---

# 65. LOGGING

Logging por niveles:

```text
error
warn
info
debug
trace
```

Default producción:

```text
warn/error
```

No registrar:

- API keys.
- documento completo.
- contenido privado.
- passwords.
- tokens.

---

# 66. RECENT FILES

Menú:

```text
File → Recent
```

Debe mostrar:

- archivos.
- workspaces.
- sesiones.

Permitir:

```text
Clear recent
```

No almacenar contenido, solamente metadata/path y configuración necesaria.

---

# 67. FILE MENU

Base:

```text
New Scratch
New File
Open File
Open Folder
Open Recent
Save
Save As
Save All
Close
Close Others
Close All
Park
Archive
Export
Quit
```

---

# 68. VIEW MENU

```text
Raw
View
Split
Explorer
Outline
Status Bar
Word Wrap
Focus Mode
Group Layout
```

---

# 69. EDIT MENU

```text
Undo
Redo
Cut
Copy
Paste
Select All
Duplicate Line
Move Line
Indent
Outdent
Find
Replace
```

---

# 70. FORMAT MENU

```text
Format Document
Format Selection
Format on Save
```

---

# 71. AI MENU

Solo aparece si AI está habilitada.

```text
Rename
Summarize
Rewrite
Explain
Extract TODOs
Organize Scratch
Merge
```

---

# 72. FOCUS MODE

`F11` o shortcut configurable.

Oculta:

- sidebar.
- status bar.
- elementos secundarios.
- tabs opcionalmente.

Mantener el editor centrado.

---

# 73. TYPEWRITER MODE

Futura o V1 si no complica el core.

La línea activa se mantiene aproximadamente centrada verticalmente.

No mover el cursor horizontalmente de manera intrusiva.

---

# 74. MODO DE VISTA VACÍA

Al iniciar sin documentos:

```text
LightMark

Open File        Ctrl+O
Open Folder      Ctrl+Shift+O
New Scratch      Ctrl+N

Drop files here
```

Muy limpio.

No usar dashboard pesado.

---

# 75. ICONOGRAFÍA

Usar iconos lineales minimalistas.

Características:

- 16–18 px base.
- sin gradientes.
- sin sombras decorativas.
- stroke consistente.
- estados por contraste.
- tooltips.

Nunca utilizar iconos enormes en la barra principal.

---

# 76. MICROINTERACCIONES

Permitidas:

- hover.
- focus.
- apertura de paneles.
- cambio de mode.
- aparición del Command Palette.

Duración aproximada:

```text 80–160 ms
```

No animar scroll del editor por defecto.

No hacer transiciones que retrasen acciones.

---

# 77. FORMATO DE ATajos

Los atajos deben estar centralizados:

```text
KeyMap
```

y configurables.

No hardcodear teclas dentro de cada componente.

---

# 78. TESTING DE EDITOR

Crear tests para:

- insert.
- delete.
- replace.
- undo.
- redo.
- multicursor.
- line mapping.
- Unicode.
- CRLF.
- LF.
- UTF-8 BOM.
- encoding.
- save/reload.
- external changes.
- Scratch recovery.

---

# 79. TESTING DE PARSERS

Fixtures:

### Markdown

- headings.
- emphasis.
- links.
- lists.
- tables.
- code.
- invalid Markdown.
- nested blocks.

### SQL

- SELECT.
- JOIN.
- CTE.
- subquery.
- INSERT.
- UPDATE.
- DELETE.
- PostgreSQL style.
- SQLite style.
- malformed SQL.

### JSON

- valid.
- invalid.
- huge.
- deeply nested.

### XML

- valid.
- namespaces.
- attributes.
- malformed.
- huge streaming.

### HTML

- malformed HTML.
- nested tags.
- scripts/styles.
- large document.

### JS

- functions.
- strings.
- template literals.
- comments.
- malformed code.

---

# 80. LARGE FILE TEST FIXTURES

Generar automáticamente:

```text
fixture-10mb.txt
fixture-100mb.txt
fixture-500mb.txt
fixture-1gb.txt
fixture-5m-lines.sql
fixture-10m-lines.sql
fixture-large.json
fixture-large.xml
```

SQL ejemplo de generación:

```sql
SELECT id, name, created_at
FROM users
WHERE active = true
ORDER BY created_at DESC;
```

repetido con variación suficiente para representar un archivo real.

No introducir datos personales reales.

---

# 81. LARGE FILE ACCEPTANCE TESTS

## Test A

Abrir 100 MB.

Debe:

- mostrar editor.
- permitir mover cursor.
- scroll.
- búsqueda.
- edición.
- save.

## Test B

Abrir 500 MB.

Debe:

- mostrar contenido sin congelar la UI durante segundos completos.
- permitir cancelar indexing.

## Test C

Abrir 1 GB.

Debe:

- iniciar en Large File Mode.
- no cargar obligatoriamente todo a una String.
- permitir navegar por viewport.
- permitir búsqueda.
- permitir pequeñas ediciones.
- guardar mediante streaming/chunked write.

## Test D

5 millones de líneas SQL.

Debe:

- activar SQL highlighting.
- ir a una línea alta.
- editar.
- hacer search.
- mantener UI reactiva.

---

# 82. BENCHMARK HARNESS

Crear crate:

```text
crates/benchmarks
```

Benchmarks:

```text
open_file
line_lookup
slice
insert
delete
search_literal
search_regex
format_json
format_xml
format_sql
render_visible_lines
load_session
archive
```

Medir:

```text
p50
p95
p99
memory
```

Guardar resultados por commit o release candidate.

---

# 83. SEARCH PERFORMANCE

Para workspace:

- paralelizar búsqueda cuando tenga sentido.
- respetar ignore.
- no recorrer binarios.
- permitir cancelar.
- mostrar resultados incrementalmente.

Para archivo grande:

- usar streaming/chunks.
- no bloquear UI.
- resultados parciales.
- mantener posición.

---

# 84. UNDO/REDO

No guardar una copia completa del documento por operación.

Usar operaciones:

```text
Insert
Delete
Replace
```

y agrupar escritura continua cuando sea razonable.

Ejemplo:

```text
typing:
hello
```

puede ser una unidad de undo, no cinco.

En archivos muy grandes, aplicar presupuesto configurable de memoria.

---

# 85. GUARDADO

Guardar:

```text
buffer
→ encoder
→ temp file
→ flush
→ atomic replace
```

Si falla:

```text
temp recovery file
```

no perder el contenido.

---

# 86. FILE TYPES Y BINARY SAFETY

Si el archivo contiene muchos bytes no válidos para texto:

```text
This file may be binary.

Open as text?
```

No intentar mostrar un ejecutable como texto sin advertencia.

---

# 87. HEX VIEW

No es requisito V1.

Podría añadirse más adelante.

---

# 88. ZIP IMPORT

V1:

- exportar ZIP.

Futuro:

- abrir ZIP como workspace virtual.
- explorar entradas.
- editar temporalmente.
- reempaquetar.

No implementarlo hasta que el core de archivo sea estable.

---

# 89. SETTINGS

Pantallas:

```text
General
Editor
Appearance
Files
Search
Formatting
Scratch
Archive
AI
Keybindings
Advanced
```

No mostrar settings en forma de dashboard complejo.

---

# 90. SETTINGS — EDITOR

```text
Font
Font Size
Line Height
Tab Size
Insert Spaces
Word Wrap
Show Line Numbers
Show Whitespace
Highlight Current Line
Bracket Matching
Smooth Scrolling
Cursor Style
```

---

# 91. SETTINGS — FILES

```text
Auto Save
Confirm Close Dirty Files
Detect External Changes
Encoding
Line Ending
Trim Trailing Whitespace on Save
Insert Final Newline
```

Por defecto no modificar silenciosamente contenido existente.

---

# 92. SETTINGS — SCRATCH

```text
Autosave delay
Keep deleted scratch
Scratch history
Default archive location
Default naming strategy
AI naming enabled
```

---

# 93. SETTINGS — ARCHIVE

```text
Folder destination
ZIP default
Date organization
Naming collision strategy
Include session.json
Include manifest.json
```

---

# 94. SETTINGS — AI

```text
Enabled
Provider
Model
API key
Default send scope
Ask before sending
Timeout
```

La API key debe aparecer enmascarada.

---

# 95. DEFAULTS

Defaults recomendados:

```text
Theme: Graphite
Groups: 1
Explorer: Hidden
Outline: Hidden
Status Bar: Visible
Word Wrap: Off for code, configurable for Markdown/TXT
Line Numbers: On
Autosave Scratch: On
AI: Off until configured
Search ignores binaries: On
Respect .gitignore: On
History: 7 days
```

---

# 96. DESIGN SYSTEM

Crear tokens centralizados:

```text
colors
spacing
radius
typography
icon sizes
control heights
focus
selection
borders
```

No colocar hex colors dispersos por todos los componentes.

---

# 97. SPACING

Base:

```text
4
8
12
16
24
32
```

Usar sistema consistente.

Tabs compactos.

Sidebar con padding reducido.

Editor con padding suficiente para lectura.

---

# 98. BORDERS

Preferir:

- 1 px sutil.
- cambios de superficie.
- separación por espacio.

No bordear cada elemento.

---

# 99. SHADOWS

Prácticamente inexistentes.

Usar sombras solo si una overlay realmente lo necesita:

- Command Palette.
- context menu.
- quick action bar.

---

# 100. RESPONSIVE RESIZING

Aunque sea escritorio, el layout debe soportar:

- ventana pequeña.
- 1 grupo.
- 2 grupos estrechos.
- sidebar abierto.

Si el espacio no alcanza, favorecer editor sobre chrome.

---

# 101. STARTUP

Startup debe:

1. crear ventana.
2. cargar theme.
3. mostrar UI.
4. restaurar sesión en background.
5. cargar documentos progresivamente.

No esperar a cargar todos los tabs para mostrar la ventana.

---

# 102. RESTORE SESSION

Si hay una sesión previa:

```text
Restore previous session?
```

Configurable:

```text
Always
Never
Ask
```

Para Scratch recuperado debe ser seguro.

---

# 103. DOCUMENT CACHE

Crear una política:

```text
hot
warm
cold
```

Hot:

- tab activa.

Warm:

- tabs recientes.

Cold:

- tabs abiertas pero no utilizadas.

Evitar mantener estructuras derivadas innecesarias para cold tabs.

---

# 104. PARSER CACHE

Guardar cache en memoria por documento.

Invalidar por rangos después de edición.

No almacenar AST completo para Large File Mode si no es necesario.

---

# 105. SYNTAX HIGHLIGHTING

Tokens semánticos:

```text
keyword
type
function
variable
string
number
comment
operator
punctuation
property
tag
attribute
heading
link
emphasis
code
quote
```

Cada tema asigna colores.

Syntax highlighting debe ser estable y no usar 20 colores intensos.

---

# 106. MARKDOWN RAW ESTILO

El objetivo visual:

```text
# Título
```

Debe destacar como título, mientras `#` sea relativamente tenue.

```text
**importante**
```

El texto debe destacar más que los `**`.

```text
[Documentation](...)
```

El texto debe ser prominente; URL secundaria.

---

# 107. SQL VISUAL

Colores para:

```text
SELECT
FROM
JOIN
WHERE
GROUP BY
ORDER BY
```

Strings, numbers y comments separados.

No usar un arcoíris de colores.

---

# 108. JSON VISUAL

Distinguir:

```text
key
string
number
boolean
null
punctuation
```

En modo Pretty View no reemplazar Raw.

---

# 109. HTML/XML VISUAL

Distinguir:

```text
tag name
attribute
attribute value
comment
text
punctuation
```

Matching de tags visual.

---

# 110. STATUS DE INDEXACIÓN

Para grandes archivos:

```text
SQL
Indexing lines 62%
```

El indicador debe desaparecer al terminar.

No mostrar progreso permanente en documentos pequeños.

---

# 111. CONTEXT MENUS

Tab:

```text
Close
Close Others
Close to the Right
Pin
Move to Group
Clone to Group
Save As
Park
Archive
Reveal in Explorer
Copy Path
```

Explorer:

```text
Open
Open in New Tab
New File
New Folder
Rename
Delete
Copy Path
Reveal in Explorer
```

Editor:

```text
Cut
Copy
Paste
Select All
Format Selection
AI actions
```

---

# 112. DRAG & DROP DE TABS

Visualización durante drag:

- indicador sutil del destino.
- si se arrastra hacia otra división, permitir drop como nuevo grupo cuando la zona lo indique.

No crear división accidental al mover dentro del mismo grupo.

---

# 113. FILE OPEN DIALOG

Debe soportar:

- multiple selection.
- drag & drop.
- filtros.

---

# 114. EXPORT

Menú:

```text
Export
  Current Document
  Current Group
  Workspace
  Scratch Selection
```

Formatos:

```text
Folder
ZIP
```

No crear PDF en V1 salvo que se decida después. PDF no es parte del núcleo de este producto.

---

# 115. CURRENT GROUP EXPORT

Caso:

```text
Group 2
  API.md
  SQL.sql
  notes.txt
```

Comando:

```text
Export Current Group
```

crea una carpeta o ZIP con exactamente esos documentos y su manifest.

---

# 116. WORKSPACE EXPORT

Exportar el workspace completo puede ser pesado.

Debe:

- copiar archivos seleccionados.
- preservar estructura relativa.
- incluir manifest.
- opcionalmente incluir sesión.

Nunca enviar un proyecto completo a AI durante exportación.

---

# 117. MANIFEST FORMAT

Ejemplo:

```json
{
  "format_version": 1,
  "created_at": "2026-09-24T13:42:00-03:00",
  "name": "Development",
  "documents": [
    {
      "filename": "01-oauth-login.md",
      "language": "markdown",
      "source": "scratch",
      "id": "..."
    }
  ]
}
```

---

# 118. FILE NAMING ENGINE

Funciones deterministas:

```text
from_heading()
from_filename()
from_first_nonempty_lines()
from_keywords()
sanitize_filename()
ensure_unique()
```

Pipeline:

```text
document
 ↓
heading candidate
 ↓
keyword candidate
 ↓
AI candidate if enabled
 ↓
sanitize
 ↓
deduplicate
```

---

# 119. SANITIZE FILENAMES

Eliminar/reemplazar:

```text
/
\
:
*
?
"
<
>
|
```

Mantener:

- Unicode seguro.
- espacios opcionales.
- guiones.

Longitud razonable:

```text
<= 80 chars
```

antes de extensión.

---

# 120. APP ID

Definir un identificador único estable, por ejemplo:

```text
com.<developer>.LightMark
```

No cambiarlo después de publicar, ya que afecta:

- settings.
- secure storage.
- OS integration.
- caches.

---

# 121. DIRECTORY LAYOUT DE LA APP

Conceptual:

```text
application data/
  config/
    settings.json
  scratch/
  sessions/
  history/
  logs/
  cache/
```

No guardar claves en esos directorios.

---

# 122. CRATE LAYOUT PROPUESTO

```text
/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── docs/
│   ├── SPEC.md
│   └── architecture.md
├── crates/
│   ├── app/
│   ├── core/
│   ├── document/
│   ├── buffer/
│   ├── language/
│   ├── search/
│   ├── workspace/
│   ├── persistence/
│   ├── archive/
│   ├── ai/
│   └── ui/
├── tests/
│   ├── fixtures/
│   └── integration/
└── benches/
```

No crear crates sin responsabilidad clara.

---

# 123. RESPONSABILIDAD DE CADA CRATE

## `core`

IDs, comandos, errores, eventos, common types.

## `document`

Document lifecycle, metadata, state.

## `buffer`

TextBuffer, normal buffer, large buffer, line indexing.

## `language`

Language detection, syntax engines, parser abstraction.

## `search`

Search/replace, regex, workspace search.

## `workspace`

Folders, explorer, tabs, groups, sessions.

## `persistence`

Settings, scratch, history, recovery.

## `archive`

Folder/ZIP export.

## `ai`

Provider abstraction and Gemini implementation.

## `ui`

Slint components and UI binding.

## `app`

Executable, dependency wiring, startup/shutdown.

---

# 124. EVENT MODEL

Usar eventos/comandos claros:

```text
Command
DocumentEvent
WorkspaceEvent
UiEvent
JobEvent
```

Ejemplos:

```text
Command::OpenFile
Command::SplitGroup
Command::ParkDocument
Command::ArchiveGroup
Command::FormatDocument
Command::SearchWorkspace
```

El UI no debe manipular directamente detalles del filesystem.

---

# 125. STATE MANAGEMENT

No construir un enorme objeto global mutable.

Separar:

```text
AppState
WorkspaceState
DocumentStore
UiState
Settings
JobState
```

Usar message passing o un patrón de comandos donde ayude a mantener el código testeable.

---

# 126. THREADING

UI thread:

```text
input
render
command dispatch
```

Worker pool:

```text
filesystem
search
parsing
formatting
AI
archive
indexing
```

Evitar demasiados threads permanentes.

Usar workers bajo demanda o pool controlado.

---

# 127. CANCELLATION

Todo job largo debe aceptar cancelación.

Ejemplo:

```text
Search:
[Cancel]
```

Si el usuario cambia la consulta, cancelar la anterior.

No permitir que 10 búsquedas viejas continúen consumiendo CPU.

---

# 128. SMART DEBOUNCE

Aplicar debounce a:

- autosave.
- syntax parsing.
- AI naming si algún día se automatiza.
- file tree refresh.
- search-as-you-type cuando sea costoso.

Search input corto puede usar debounce ~50–150 ms.

---

# 129. GPU / RENDERING

La UI debe usar el camino gráfico proporcionado por el framework.

No renderizar texto mediante miles de widgets.

El editor debería ser un componente especializado o una superficie de texto virtualizada, no una lista de Text widgets.

---

# 130. FIRST SPIKE OBLIGATORIO

Antes de construir toda la aplicación:

crear una prueba mínima con:

```text
Slint
+
TextBuffer
+
10k lines
+
100k lines
+
1m lines
+
syntax highlighting
+
scroll
+
cursor
+
selection
```

Luego benchmark:

```text
typing
scroll
jump line
search
resize
theme switch
```

Si Slint no satisface los objetivos, evaluar una alternativa nativa antes de seguir desarrollando.

No gastar semanas construyendo features sobre una capa UI que no supera este spike.

---

# 131. SEGUNDO SPIKE — LARGE FILE

Construir:

```text
Paged/Piece Buffer
+
Line Index
+
Viewport renderer
```

con fixture de 500 MB y 1 GB.

Validar:

- memoria.
- latencia.
- save.
- edits.
- search.
- goto line.

---

# 132. TERCER SPIKE — PARSING

Probar:

```text
Tree-sitter
Markdown parser
SQL parser
quick-xml
JSON
HTML
```

con archivos normales y grandes.

No activar parsing completo en large mode.

---

# 133. CUARTO SPIKE — SCRATCH

Implementar:

```text
new scratch
autosave
crash simulation
restore
archive
rename
```

Antes de AI.

---

# 134. QUINTO SPIKE — AI

Implementar solo:

```text
Gemini provider
secure key storage
rename
```

Verificar:

- key storage.
- timeout.
- cancellation.
- no logs de secret.
- malformed response.
- offline error.
- invalid API credential.

Después ampliar.

---

# 135. RELEASE MODES

## Debug

- logs.
- assertions.
- diagnostics.

## Release

- LTO.
- strip symbols según platform.
- panic strategy documentada.
- optimizaciones.

No sacrificar información necesaria para crash diagnosis interno.

---

# 136. TELEMETRÍA

Por defecto:

**OFF.**

Si se incorpora en el futuro:

- opt-in.
- claramente explicado.
- nunca contenido de documentos.
- nunca API keys.
- nunca nombres de archivos completos si contienen información privada.

---

# 137. UPDATE SYSTEM

No forma parte del core V1.

Puede añadirse más adelante.

---

# 138. AUTO UPDATE

Futuro.

No introducir un servicio residente solo por actualizar la aplicación.

---

# 139. INSTALADOR

Windows:

- instalador sencillo.
- desinstalación limpia.
- asociación opcional de `.md`, `.txt`, `.sql`, `.json`, `.xml`, `.html`, `.js`.
- opción portable si no complica.

Preferir un binario pequeño.

---

# 140. ASOCIACIONES DE ARCHIVO

Configurar opcionalmente:

```text
.md
.txt
.sql
.json
.xml
.html
.htm
.js
.mjs
.cjs
```

---

# 141. PORTABLE MODE

Futuro, o V1 si resulta fácil:

```text
LightMark.exe
config/
scratch/
```

No escribir settings fuera de la carpeta portable.

---

# 142. DOCUMENTACIÓN DEL USUARIO

Incluir:

```text
docs/user/
  getting-started.md
  keyboard-shortcuts.md
  markdown.md
  scratch.md
  archives.md
  large-files.md
  ai.md
```

---

# 143. DOCUMENTACIÓN DEL DESARROLLADOR

Incluir:

```text
docs/dev/
  architecture.md
  text-buffer.md
  parser-system.md
  ui.md
  performance.md
  testing.md
  ai-provider.md
```

---

# 144. AGENT MEMORY

El agente debe leer al iniciar:

```text
SPEC.md
README.md
docs/dev/architecture.md
```

Antes de modificar arquitectura.

No asumir que una librería sigue igual solo porque existía en una versión anterior.

---

# 145. DEPENDENCIAS

Toda dependencia debe justificar:

```text
Purpose
Binary size impact
Compile-time impact
Runtime impact
License
Maintenance status
```

Evitar dependencias duplicadas.

Preferir librerías Rust puras cuando no afecten funcionalidad.

---

# 146. VERSIONES DE REFERENCIA VERIFICADAS

Estas versiones fueron consultadas como base el 2026-09-24. El agente debe volver a verificarlas al iniciar el proyecto y fijar las versiones que realmente se utilicen.

```text
Slint: 1.18.x baseline
tree-sitter: 0.27.x baseline
ropey: 1.6.1 estable; 2.x beta no asumir como base
sqlparser: 0.63.x
quick-xml: 0.42.x
html5ever: 0.40.x
serde_json: 1.0.x
ignore: 0.4.x
ripgrep: 15.2.x como referencia de motor/herramienta
zip: 8.x
keyring: 4.x
ureq: 3.x
notify: 8.x
```

No utilizar estos números ciegamente. Consultar releases actuales, MSRV, features y seguridad antes de congelar `Cargo.lock`.

---

# 147. DEPENDENCIAS ESPECÍFICAS A REVISAR

## UI

Slint.

## Buffer

ropey o backend propio paginado.

## Parsing

tree-sitter.

## Markdown

pulldown-cmark u otro parser compatible.

## SQL

sqlparser.

## JSON

serde_json.

## XML

quick-xml.

## HTML

html5ever.

## Search

regex + ignore + grep/ripgrep components.

## File watching

notify.

## ZIP

zip.

## Secrets

keyring.

## HTTP

ureq.

## Serialization

serde / serde_json.

---

# 148. LICENCIAS

Crear un archivo:

```text
THIRD_PARTY_LICENSES.md
```

No incorporar dependencia sin comprobar su licencia y compatibilidad con la licencia del producto.

---

# 149. REGLA DE UI: NO HACER UN IDE

Cada vez que el agente quiera añadir:

- panel.
- barra.
- badge.
- botón.
- indicador.
- widget.

debe preguntarse:

> ¿Esta información es necesaria durante la edición?

Si no:

- esconder.
- mover a Command Palette.
- mover a menú.
- mostrar bajo demanda.

---

# 150. REGLA DE UI: NO HACER UNA APP WEB

Evitar:

- enormes tarjetas.
- sidebar permanente.
- top navigation alta.
- gradientes llamativos.
- animaciones constantes.
- sombras pesadas.
- botones gigantes.
- texto redundante.

La app debe sentirse como software profesional de escritorio.

---

# 151. DEFINICIÓN DE "RÁPIDA"

"Rápida" no significa solamente startup.

Debe sentirse rápida al:

- abrir archivo.
- cambiar tab.
- escribir.
- seleccionar.
- scroll.
- buscar.
- reemplazar.
- cambiar Raw/View.
- abrir folder.
- crear Scratch.
- Park.
- Archive.

Ninguna acción normal debe producir un freeze de UI evitable.

---

# 152. DEFINICIÓN DE "LIGERA"

Objetivos:

- pocos procesos.
- sin servidor local.
- sin Chromium.
- sin DB permanente.
- sin cloud.
- bajo número de dependencias runtime.
- parser incremental.
- render virtual.
- background jobs.
- memoria controlada.

Medir el tamaño del ejecutable y RSS en release y registrar resultados.

No prometer un número de MB hasta tener benchmark de una build real.

---

# 153. V1 ROADMAP

## Milestone 0 — Architecture Spike

Entregables:

- repo.
- Cargo workspace.
- Slint window.
- TextBuffer prototype.
- benchmark harness.
- fixtures.

## Milestone 1 — Editor Core

- document.
- tabs.
- one group.
- cursor.
- selection.
- undo/redo.
- save/open.

## Milestone 2 — Sublime-like Layout

- 2–4 groups.
- move tabs.
- split.
- Quick Open.
- Command Palette.

## Milestone 3 — Search

- Find.
- Replace.
- Regex.
- Workspace search.

## Milestone 4 — Languages

- Markdown.
- HTML.
- JS.
- SQL.
- JSON.
- XML.
- TXT.

## Milestone 5 — Markdown

- Raw.
- View.
- Split.
- sync navigation.

## Milestone 6 — Formatters

- JSON.
- XML.
- SQL.
- HTML.

## Milestone 7 — Large Files

- paged buffer.
- line index.
- viewport rendering.
- large mode.
- benchmarks.

## Milestone 8 — Explorer/Workspace

- folder tree.
- file operations.
- session persistence.

## Milestone 9 — Scratch

- autosave.
- Inbox.
- Park.
- Archive.
- session export.
- ZIP/folder.

## Milestone 10 — AI

- secure key storage.
- Gemini.
- naming.
- summarize.
- rewrite.
- organize.
- merge.

## Milestone 11 — Polish

- 3 themes.
- accessibility.
- keyboard.
- packaging.
- recovery.
- external change handling.

---

# 154. CRITERIOS DE ACEPTACIÓN DEL MVP

El MVP NO está terminado hasta que un usuario pueda hacer lo siguiente:

### Caso 1 — Scratch

```text
Ctrl+N
escribir nota
Ctrl+N
escribir otra nota
cerrar aplicación
abrir aplicación
```

Ambas notas deben recuperarse.

### Caso 2 — Tabs

Abrir 30 tabs.

La app debe seguir siendo usable.

### Caso 3 — Groups

Iniciar con 1 grupo.

Activar 2.

Mover un tab.

Activar 3.

Volver a 1.

Debe funcionar sin perder pestañas.

### Caso 4 — SQL grande

Abrir `5m-lines.sql`.

Ir a una línea cercana al final.

Buscar `SELECT`.

Editar.

Guardar.

No congelar la UI.

### Caso 5 — Markdown

Editar Raw.

Cambiar a View.

Cambiar a Split.

Los cambios deben aparecer.

### Caso 6 — JSON

Abrir JSON minificado.

Format Document.

Guardar.

Reabrir.

Debe conservar datos.

### Caso 7 — XML

Abrir XML.

Format.

Guardar.

Reabrir.

Debe conservar estructura.

### Caso 8 — Archive

Tener 10 Scratch.

Park All.

Elegir ZIP.

Generar:

```text
archive.zip
```

con nombres automáticos.

### Caso 9 — AI off

Desactivar AI.

Todo lo demás debe funcionar.

### Caso 10 — AI failure

Configurar AI con credencial inválida.

La app debe:

- informar error.
- permitir seguir editando.
- no perder documentos.
- no congelar.

---

# 155. CRITERIOS DE ACEPTACIÓN VISUAL

La pantalla inicial debe pasar esta prueba:

> Un usuario debe poder identificar qué archivo está editando sin sentir que tiene que aprender una interfaz compleja.

La app debe mostrar inicialmente:

- tabs.
- editor.
- status.

El Explorer debe estar oculto.

Las columnas adicionales deben estar desactivadas.

No mostrar dashboard.

---

# 156. CRITERIOS DE ACEPTACIÓN DE ARCHIVOS GRANDES

No aceptar:

```text
Thread UI blocked while loading
```

No aceptar:

```text
Out of memory
```

como comportamiento normal de archivos grandes.

No aceptar:

```text
Syntax highlighting takes the entire file before first display
```

No aceptar:

```text
Scroll requires rendering every line
```

No aceptar:

```text
Save duplicates entire document in multiple buffers
```

---

# 157. CRITERIOS DE ACEPTACIÓN DE SCRATCH

Nunca debe existir el mensaje:

```text
Are you sure you want to close?
```

solo porque una Scratch no tiene nombre.

Scratch está diseñado para ser temporal y persistente.

La confirmación puede aparecer únicamente si existe una operación explícita que pueda destruir contenido.

---

# 158. CRITERIOS DE ACEPTACIÓN DE AI

Nunca:

- AI mandatory.
- network mandatory.
- shared API key.
- document upload automático.
- content transmission invisible.

Siempre:

- BYOK.
- secure storage.
- explicit scope.
- cancellation.
- error handling.
- preview before destructive transformation.

---

# 159. FUTURO BACKLOG

No implementar todavía, pero dejar espacio arquitectónico:

```text
LSP
Git integration
more languages
PCRE2
Mermaid
LaTeX
image manager
local AI
OpenAI provider
OpenRouter provider
ZIP virtual workspace
history diff UI
plugins
portable mode
update system
HTML preview
PDF export
DOCX export
```

---

# 160. PRINCIPIO FINAL PARA EL AGENTE

Cuando exista conflicto entre:

```text
más features
```

y:

```text
mejor editor
```

elegir:

**mejor editor.**

Cuando exista conflicto entre:

```text
parsing completo
```

y:

```text
UI responsive
```

elegir:

**UI responsive + parsing incremental.**

Cuando exista conflicto entre:

```text
AI convenience
```

y:

```text
privacidad/offline/cero dependencia
```

elegir:

**privacidad/offline/cero dependencia.**

Cuando exista conflicto entre:

```text
UI llena de controles
```

y:

```text
minimalismo
```

elegir:

**minimalismo + Command Palette.**

---

# 161. FUENTES TÉCNICAS CONSULTADAS

Estas fuentes no sustituyen la documentación de las versiones que se usarán al compilar.

- Slint 1.18, anuncio y mejoras de rendimiento:
  https://slint.dev/blog/slint-1.18-released
- Slint desktop / Windows:
  https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/windows/general/
- Tree-sitter Rust parser:
  https://docs.rs/tree-sitter/latest/tree_sitter/struct.Parser.html
- Ropey:
  https://docs.rs/crate/ropey/latest
- sqlparser:
  https://docs.rs/crate/sqlparser/latest
- quick-xml:
  https://docs.rs/crate/quick-xml/latest
- html5ever:
  https://docs.rs/crate/html5ever/latest
- serde_json:
  https://docs.rs/serde_json/latest/serde_json/
- ignore:
  https://docs.rs/ignore/latest/ignore/
- ripgrep:
  https://docs.rs/crate/ripgrep/latest
- zip:
  https://docs.rs/crate/zip/latest
- keyring:
  https://docs.rs/crate/keyring/latest
- ureq:
  https://docs.rs/crate/ureq/latest
- notify:
  https://docs.rs/crate/notify/latest
- Gemini API key guidance:
  https://ai.google.dev/gemini-api/docs/api-key

---

# 162. PROMPT INICIAL RECOMENDADO PARA EL AGENTE AI

Use el siguiente texto como mensaje inicial del agente al comenzar el repositorio:

> Eres el agente principal de desarrollo de una aplicación de escritorio nativa escrita en Rust. La aplicación es un editor ultraligero de texto y Markdown con UX inspirada en Sublime Text, pero cuyo diferenciador principal es un sistema Scratch/Inbox/Archive que permite trabajar con decenas de pestañas temporales sin perder información ni obligar al usuario a nombrarlas manualmente.
>
> Lee completamente `SPEC.md` antes de modificar el proyecto.
>
> No conviertas la aplicación en un IDE, web app o Electron app.
>
> Implementa por milestones. Primero construye los spikes de arquitectura y rendimiento. No construyas funcionalidades avanzadas sobre una arquitectura de editor que no haya demostrado funcionar con archivos grandes.
>
> Requisitos no negociables:
>
> - Rust estable.
> - UI nativa con Slint como primera opción.
> - Una columna por defecto.
> - Hasta cuatro grupos bajo demanda.
> - múltiples tabs.
> - Find/Replace/Regex.
> - Raw/View/Split para Markdown.
> - Syntax: Markdown, HTML, JS, SQL, JSON, XML, TXT.
> - Autoformat JSON/XML/SQL/HTML.
> - Explorer de carpetas opcional.
> - Scratch autosave.
> - Park/Inbox/Archive.
> - Exportar selección/grupo/workspace como carpeta o ZIP.
> - restaurar sesiones.
> - soporte serio para archivos grandes.
> - renderizado virtualizado.
> - parsing incremental.
> - AI opcional mediante provider abstraction y BYOK.
> - API keys fuera de configuración JSON.
> - tres temas: Graphite, Ivory, Aurora.
>
> No declares terminado ningún milestone sin:
>
> - tests,
> - clippy sin warnings relevantes,
> - benchmarks cuando corresponda,
> - validación de archivos grandes,
> - prueba de recuperación de documentos,
> - y revisión de que la UI mantiene la filosofía minimalista.
>
> Toda decisión arquitectónica que contradiga esta especificación debe documentarse antes de aplicarse.

---

# 163. DEFINITION OF DONE DEL PRODUCTO

La aplicación podrá considerarse versión 1.0 únicamente cuando:

- edite texto de forma fluida.
- maneje múltiples tabs sin degradación severa.
- tenga 1 grupo por defecto y hasta 4 grupos bajo demanda.
- tenga búsqueda y reemplazo profesional.
- soporte los 7 tipos de contenido especificados.
- tenga Markdown Raw/View/Split.
- pueda formatear JSON/XML/SQL/HTML.
- abra y navegue archivos grandes mediante un backend adecuado.
- mantenga la UI responsiva durante operaciones pesadas.
- tenga Explorer/Workspace.
- tenga Scratch autosave.
- tenga Inbox/Park/Archive.
- archive como carpeta o ZIP.
- restaure sesiones.
- tenga 3 temas pulidos.
- maneje cambios externos.
- permita funcionamiento completo sin AI.
- pueda utilizar AI mediante BYOK sin exponer secretos en archivos.
- tenga tests automatizados del core.
- tenga benchmarks reproducibles.
- tenga documentación de usuario y desarrollador.

El producto final debe poder resumirse en una sola idea:

> **Escribe rápido. Abre todo lo que necesites. No pienses en guardar cada nota. La aplicación se encarga de conservar, organizar y dejarte volver después.**

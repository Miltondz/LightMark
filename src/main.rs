#![windows_subsystem = "windows"]

use ropey::Rope;
use slint::{ComponentHandle, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

mod format;
mod highlight;
mod keyboard;
mod markdown;
mod scratch;
mod search;
mod session;
mod workspace;
use scratch::ScratchManager;
use session::{Session, SessionManager, TabState};

slint::slint! {
    import { Button, HorizontalBox, TextEdit, VerticalBox, LineEdit, ListView } from "std-widgets.slint";

    export struct PreviewLine {
        text: string,
        color: color,
        bg_color: color,
        font_size: length,
        is_bold: bool,
        is_mono: bool,
        has_border_left: bool,
        border_left_color: color,
        has_border_bottom: bool,
        border_bottom_color: color,
        padding_left: length,
        padding_top: length,
        padding_bottom: length,
    }

    // Separador vertical de barra de herramientas
    component ToolSeparator inherits Rectangle {
        width: 1px;
        height: 18px;
        background: #3e3e42;
    }

    // Botón limpio y moderno de barra de herramientas con texto legible
    component ToolButton inherits Rectangle {
        in property <string> label: "";
        in property <bool> active: false;
        callback clicked();

        height: 22px;
        background: tb_ta.pressed ? #094771 : (tb_ta.has-hover ? #3e3e42 : (root.active ? #0e639c : #2d2d30));
        border-radius: 3px;
        border-width: root.active ? 1px : 0px;
        border-color: #007acc;

        HorizontalLayout {
            padding-left: 7px;
            padding-right: 7px;
            alignment: center;

            Text {
                text: root.label;
                font-size: 11px;
                font-weight: root.active ? 600 : 400;
                color: root.active ? #ffffff : (tb_ta.has-hover ? #ffffff : #cccccc);
                vertical-alignment: center;
            }
        }

        tb_ta := TouchArea {
            clicked => { root.clicked(); }
            mouse-cursor: pointer;
        }
    }

    // Item de menú superior (Archivo, Editar, Ver...)
    component MenuBarItem inherits Rectangle {
        in property <string> text;
        in property <bool> is_open: false;
        callback clicked();

        height: 22px;
        background: root.is_open ? #37373d : (mb_ta.has-hover ? #2a2d2e : transparent);
        border-radius: 0px;

        HorizontalLayout {
            padding-left: 9px;
            padding-right: 9px;
            Text {
                text: root.text;
                font-size: 12px;
                color: root.is_open || mb_ta.has-hover ? #ffffff : #cccccc;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }

        mb_ta := TouchArea {
            clicked => { root.clicked(); }
            mouse-cursor: pointer;
        }
    }

    // Item de menú desplegable
    component DropdownMenuItem inherits Rectangle {
        in property <string> icon: "";
        in property <string> text: "";
        in property <string> shortcut: "";
        in property <bool> is_separator: false;
        in property <bool> is_checked: false;
        callback clicked();

        height: is_separator ? 7px : 24px;

        if is_separator: Rectangle {
            height: 1px;
            background: #3e3e42;
            y: 3px;
        }

        if !is_separator: Rectangle {
            background: item_ta.has-hover ? #094771 : transparent;
            border-radius: 3px;

            HorizontalLayout {
                padding-left: 10px;
                padding-right: 10px;
                spacing: 6px;

                // Checkmark o ícono
                Text {
                    text: root.is_checked ? "✓" : (root.icon != "" ? root.icon : " ");
                    font-size: 12px;
                    color: root.is_checked ? #3dc9a0 : (item_ta.has-hover ? #ffffff : #999999);
                    vertical-alignment: center;
                    width: 16px;
                }

                Text {
                    text: root.text;
                    font-size: 12px;
                    color: item_ta.has-hover ? #ffffff : #cccccc;
                    vertical-alignment: center;
                    horizontal-stretch: 1;
                }

                if root.shortcut != "": Text {
                    text: root.shortcut;
                    font-size: 11px;
                    color: item_ta.has-hover ? #aaaaaa : #666666;
                    vertical-alignment: center;
                    horizontal-alignment: right;
                }
            }

            item_ta := TouchArea {
                clicked => { root.clicked(); }
                mouse-cursor: pointer;
            }
        }
    }

    // Pestaña estilo Sublime Text (plana, integrada, con áreas de clic separadas)
    component FlatTab inherits Rectangle {
        in property <string> title: "Tab";
        in property <bool> is_active: false;
        in property <bool> is_dirty: false;
        callback tab_clicked();
        callback close_clicked();

        height: 33px;
        min-width: 90px;
        max-width: 220px;
        background: root.is_active ? #1e1e1e : (tab_ta.has-hover ? #2a2d2e : #252526);
        border-width: 0px;

        // Top accent line for active tab
        if root.is_active: Rectangle {
            x: 0px;
            y: 0px;
            width: 100%;
            height: 2px;
            background: #007acc;
        }

        HorizontalLayout {
            padding-left: 10px;
            padding-right: 4px;
            spacing: 4px;

            // Área de selección de la pestaña (no se superpone al botón de cerrar)
            Rectangle {
                horizontal-stretch: 1;

                Text {
                    text: root.is_dirty ? "● " + root.title : root.title;
                    font-size: 12px;
                    color: root.is_active ? #ffffff : (tab_ta.has-hover ? #cccccc : #999999);
                    vertical-alignment: center;
                    overflow: elide;
                }

                tab_ta := TouchArea {
                    clicked => { root.tab_clicked(); }
                    mouse-cursor: pointer;
                }
            }

            // Botón cerrar pestaña (área dedicada e independiente)
            Rectangle {
                width: 20px;
                height: 20px;
                background: close_ta.has-hover ? #3e3e42 : transparent;
                border-radius: 3px;

                Text {
                    text: "×";
                    font-size: 14px;
                    font-weight: 700;
                    color: close_ta.has-hover ? #ffffff : (root.is_active ? #cccccc : #666666);
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }

                close_ta := TouchArea {
                    clicked => { root.close_clicked(); }
                    mouse-cursor: pointer;
                }
            }
        }
    }

    export component App inherits Window {
        min-width: 640px;
        min-height: 400px;
        preferred-width: 1100px;
        preferred-height: 700px;
        title: "LightMark";
        background: #1e1e1e;
        full-screen: root.is_fullscreen;

        // ── Callbacks ────────────────────────────────────────────
        callback tab-clicked(int);
        callback close-tab(int);
        callback new-tab();
        callback new-scratch();
        callback save-scratch();
        callback toggle-markdown-view();
        callback toggle-markdown-split();
        callback search-text(string);
        callback replace-text(string);
        callback replace-all();
        callback find-next();
        callback find-prev();
        callback toggle-search-panel();
        callback switch-group(int);
        callback set-group-count(int);
        callback open-file();
        callback save-session();
        callback restore-session();
        callback park-tab();
        callback save-as();
        callback format-document();
        callback save-all();
        callback export-archive();
        callback undo();
        callback redo();
        callback toggle-explorer();
        callback open-folder();
        callback open-file-from-explorer(string);
        callback open-app-folder();
        callback toggle-fullscreen();
        callback open-browser-preview();
        callback toggle-syntax-view();
        callback copy-preview();
        callback copy-all-text();
        callback paste-clipboard();
        callback duplicate-line();
        callback delete-line();
        callback move-line-up();
        callback move-line-down();
        callback toggle-comment();
        callback indent-line();
        callback unindent-line();
        callback join-lines();

        // ── Properties ───────────────────────────────────────────
        in-out property <[string]> tab_titles;
        in-out property <string> documentText;
        in-out property <int> active_tab;
        in-out property <int> tab_count;
        in-out property <int> group_count: 1;
        in-out property <int> active_group;
        in-out property <bool> markdown_view_enabled: false;
        in-out property <bool> markdown_split_enabled: false;
        in-out property <string> status_text: "Listo";
        in-out property <string> line_numbers;
        in-out property <bool> show_explorer: false;
        in-out property <[string]> explorer_files;
        in-out property <int> active_dropdown: -1;
        in-out property <bool> show_search: false;
        in-out property <string> search_query: "";
        in-out property <string> replace_query: "";
        in-out property <string> search_status: "";
        in-out property <bool> is_fullscreen: false;
        in-out property <bool> show_settings: false;
        in-out property <bool> show_scratch_help: false;
        in-out property <int> editor_font_size: 14;
        in-out property <int> autosave_delay_ms: 1000;
        in-out property <bool> syntax_view_enabled: false;
        in-out property <string> detected_lang: "markdown";
        in-out property <string> doc_stats: "";
        in-out property <[PreviewLine]> preview_items;
        in-out property <string> preview_text: "";
        in-out property <bool> preview_selectable_mode: true;
        // Barra de herramientas y 2 columnas
        in-out property <bool> show_toolbar: false;
        in-out property <bool> show_second_column: false;
        // Cursor position

        // ── Layout Principal ─────────────────────────────────────
        VerticalLayout {
            spacing: 0px;
            padding: 0px;

            // ── FILA 1: Barra de Menú ────────────────────────────
            Rectangle {
                height: 28px;
                background: #333333;

                HorizontalLayout {
                    padding-left: 2px;
                    padding-right: 4px;
                    spacing: 0px;
                    alignment: start;

                    MenuBarItem {
                        text: "Archivo";
                        is_open: root.active_dropdown == 0;
                        clicked => { root.active_dropdown = root.active_dropdown == 0 ? -1 : 0; }
                    }
                    MenuBarItem {
                        text: "Editar";
                        is_open: root.active_dropdown == 1;
                        clicked => { root.active_dropdown = root.active_dropdown == 1 ? -1 : 1; }
                    }
                    MenuBarItem {
                        text: "Selección";
                        is_open: root.active_dropdown == 5;
                        clicked => { root.active_dropdown = root.active_dropdown == 5 ? -1 : 5; }
                    }
                    MenuBarItem {
                        text: "Ver";
                        is_open: root.active_dropdown == 2;
                        clicked => { root.active_dropdown = root.active_dropdown == 2 ? -1 : 2; }
                    }
                    MenuBarItem {
                        text: "Grupos";
                        is_open: root.active_dropdown == 3;
                        clicked => { root.active_dropdown = root.active_dropdown == 3 ? -1 : 3; }
                    }
                    MenuBarItem {
                        text: "Ayuda";
                        is_open: root.active_dropdown == 4;
                        clicked => { root.active_dropdown = root.active_dropdown == 4 ? -1 : 4; }
                    }

                    Rectangle { horizontal-stretch: 1; }
                }
            }

            // ── FILA 2: Barra de Herramientas (Togglable) ────────
            if root.show_toolbar: Rectangle {
                height: 30px;
                background: #252526;

                Rectangle {
                    y: parent.height - 1px;
                    height: 1px;
                    width: 100%;
                    background: #3e3e42;
                }

                HorizontalLayout {
                    padding-left: 6px;
                    padding-right: 6px;
                    spacing: 4px;
                    alignment: start;

                    // Archivo
                    ToolButton { label: "+ Nuevo"; clicked => { root.new-tab(); } }
                    ToolButton { label: "Abrir"; clicked => { root.open-file(); } }
                    ToolButton { label: "Guardar"; clicked => { root.save-scratch(); } }
                    ToolSeparator {}

                    // Edición
                    ToolButton { label: "Deshacer"; clicked => { root.undo(); } }
                    ToolButton { label: "Rehacer"; clicked => { root.redo(); } }
                    ToolButton { label: "Buscar"; active: root.show_search; clicked => { root.toggle-search-panel(); } }
                    ToolSeparator {}

                    // Vistas y Paneles
                    ToolButton { label: "Vista Previa"; active: root.markdown_view_enabled && !root.syntax_view_enabled; clicked => { root.syntax_view_enabled = false; root.toggle-markdown-view(); } }
                    ToolButton { label: "Sintaxis"; active: root.syntax_view_enabled; clicked => { root.toggle-syntax-view(); } }
                    ToolButton { label: "Navegador"; clicked => { root.open-browser-preview(); } }
                    ToolButton { label: "2 Cols"; active: root.show_second_column; clicked => { root.show_second_column = !root.show_second_column; } }
                    ToolButton { label: "Explorador"; active: root.show_explorer; clicked => { root.toggle-explorer(); } }
                    ToolSeparator {}

                    // Ventana
                    ToolButton { label: "Pantalla Completa"; active: root.is_fullscreen; clicked => { root.toggle-fullscreen(); } }
                    ToolButton { label: "Configuración"; active: root.show_settings; clicked => { root.show_settings = !root.show_settings; } }

                    Rectangle { horizontal-stretch: 1; }
                }
            }

            // ── FILA 3: Panel de Búsqueda ─────────────────────────
            if root.show_search: Rectangle {
                height: 32px;
                background: #252526;

                HorizontalLayout {
                    padding-left: 8px;
                    padding-right: 8px;
                    spacing: 4px;
                    alignment: start;
                    Text { text: "Buscar:"; font-size: 12px; color: #cccccc; vertical-alignment: center; }
                    LineEdit {
                        text <=> root.search_query;
                        width: 150px;
                        accepted => { root.search-text(root.search_query); }
                    }
                    Button { text: "↑"; width: 24px; clicked => { root.find-prev(); } }
                    Button { text: "↓"; width: 24px; clicked => { root.find-next(); } }
                    Text { text: "Reemplazar:"; font-size: 12px; color: #cccccc; vertical-alignment: center; }
                    LineEdit { text <=> root.replace_query; width: 140px; }
                    Button { text: "Reemplazar"; clicked => { root.replace-text(root.replace_query); } }
                    Button { text: "Todo"; clicked => { root.replace-all(); } }
                    Text { text: root.search_status; font-size: 11px; color: #888888; vertical-alignment: center; }
                    Rectangle { horizontal-stretch: 1; }
                    Button { text: "×"; width: 22px; clicked => { root.toggle-search-panel(); } }
                }
            }

            // ── ÃREA CENTRAL ─────────────────────────────────────
            HorizontalLayout {
                spacing: 0px;
                vertical-stretch: 1;

                // Sidebar / Explorador
                if root.show_explorer: Rectangle {
                    width: 210px;
                    background: #252526;

                    VerticalLayout {
                        padding: 0px;
                        spacing: 0px;

                        // Cabecera explorador
                        Rectangle {
                            height: 26px;
                            background: #252526;
                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 6px;
                                Text { text: "EXPLORADOR"; font-size: 10px; color: #888888; vertical-alignment: center; horizontal-stretch: 1; }
                                TouchArea {
                                    width: 20px;
                                    Text { text: "+"; font-size: 14px; color: #888888; horizontal-alignment: center; vertical-alignment: center; }
                                    clicked => { root.open-folder(); }
                                    mouse-cursor: pointer;
                                }
                            }
                        }

                        // Separador
                        Rectangle { height: 1px; background: #3e3e42; }

                        // Lista de archivos
                        for file in root.explorer_files: Rectangle {
                            height: 22px;
                            background: file_area.has-hover ? #2a2d2e : transparent;
                            HorizontalLayout {
                                padding-left: 16px;
                                Text {
                                    text: file;
                                    font-size: 12px;
                                    color: #cccccc;
                                    vertical-alignment: center;
                                    overflow: elide;
                                }
                            }
                            file_area := TouchArea {
                                clicked => { root.open-file-from-explorer(file); }
                                mouse-cursor: pointer;
                            }
                        }
                    }

                    // Separador derecho
                    Rectangle {
                        x: parent.width - 1px;
                        width: 1px;
                        height: 100%;
                        background: #3e3e42;
                    }
                }

                // ── COLUMNA 1 (Editor Principal) ──────────────────
                VerticalLayout {
                    spacing: 0px;
                    horizontal-stretch: 1;

                    // Pestañas Columna 1
                    Rectangle {
                        height: 33px;
                        background: #252526;

                        HorizontalLayout {
                            spacing: 0px;
                            padding: 0px;
                            alignment: start;

                            for i in root.tab_count: FlatTab {
                                title: root.tab_titles[i];
                                is_active: root.active_tab == i && root.active_group == 0;
                                tab_clicked => { root.tab-clicked(i); }
                                close_clicked => { root.close-tab(i); }
                            }

                            // Botón añadir tab '+'
                            Rectangle {
                                width: 30px;
                                height: 33px;
                                background: add_ta.has-hover ? #3e3e42 : transparent;
                                border-radius: 3px;

                                Text {
                                    text: "+";
                                    font-size: 18px;
                                    font-weight: 600;
                                    color: add_ta.has-hover ? #ffffff : #999999;
                                    horizontal-alignment: center;
                                    vertical-alignment: center;
                                }

                                add_ta := TouchArea {
                                    clicked => { root.new-tab(); }
                                    mouse-cursor: pointer;
                                }
                            }

                            Rectangle { horizontal-stretch: 1; background: #252526; }
                        }

                        // Línea inferior de pestaÃ±as
                        Rectangle {
                            y: parent.height - 1px;
                            height: 1px;
                            width: 100%;
                            background: #3e3e42;
                        }
                    }

                    // Editor Columna 1
                    HorizontalLayout {
                        spacing: 0px;
                        vertical-stretch: 1;
                        padding: 0px;

                        // Números de línea
                        Rectangle {
                            width: 46px;
                            background: #1e1e1e;
                            VerticalLayout {
                                padding-right: 8px;
                                padding-top: 4px;
                                Text {
                                    text: root.line_numbers;
                                    font-family: "Consolas, monospace";
                                    font-size: root.editor_font_size * 1px;
                                    color: #495057;
                                    horizontal-alignment: right;
                                    vertical-alignment: top;
                                }
                            }
                            // Línea separadora
                            Rectangle {
                                x: parent.width - 1px;
                                width: 1px;
                                height: 100%;
                                background: #2d2d30;
                            }
                        }

                        // TextEdit principal
                        TextEdit {
                            text <=> root.documentText;
                            font-size: root.editor_font_size * 1px;
                            font-family: "Consolas, monospace";
                            horizontal-stretch: 1;
                        }
                    }
                }

                // ── COLUMNA 2 (Previa / Segundo Editor) ───────────
                if root.show_second_column || root.markdown_view_enabled || root.markdown_split_enabled || root.syntax_view_enabled: Rectangle {
                    width: 1px;
                    background: #3e3e42;
                }

                if root.show_second_column || root.markdown_view_enabled || root.markdown_split_enabled || root.syntax_view_enabled: VerticalLayout {
                    spacing: 0px;
                    horizontal-stretch: 1;

                    // Barra de cabecera columna 2
                    Rectangle {
                        height: 33px;
                        background: #252526;

                        HorizontalLayout {
                            padding-left: 12px;
                            padding-right: 6px;
                            spacing: 8px;

                            Text {
                                text: root.syntax_view_enabled
                                    ? "SINTAXIS â€” " + root.detected_lang
                                    : "VISTA PREVIA â€” " + (root.detected_lang == "html" ? "HTML" : "Markdown");
                                font-size: 10px;
                                color: #888888;
                                vertical-alignment: center;
                                horizontal-stretch: 1;
                            }

                            // Botón alternar entre Previa y Sintaxis
                            TouchArea {
                                width: 70px;
                                height: 22px;
                                Rectangle {
                                    background: sw_ta.has-hover ? #3e3e42 : transparent;
                                    border-radius: 3px;
                                    border-width: 1px;
                                    border-color: #3e3e42;
                                    Text {
                                        text: root.syntax_view_enabled ? "📖 Texto" : "🎨 Sintaxis";
                                        font-size: 10px;
                                        color: #888888;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                }
                                sw_ta := TouchArea {
                                    clicked => { root.toggle-syntax-view(); }
                                    mouse-cursor: pointer;
                                }
                            }

                            // Botón abrir en navegador
                            TouchArea {
                                width: 70px;
                                height: 22px;
                                Rectangle {
                                    background: wb_ta.has-hover ? #3e3e42 : transparent;
                                    border-radius: 3px;
                                    border-width: 1px;
                                    border-color: #3e3e42;
                                    Text {
                                        text: "ðŸŒ Web";
                                        font-size: 10px;
                                        color: #61afef;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                }
                                wb_ta := TouchArea {
                                    clicked => { root.open-browser-preview(); }
                                    mouse-cursor: pointer;
                                }
                            }

                            // Botón cerrar columna 2
                            TouchArea {
                                width: 24px;
                                height: 24px;
                                Text {
                                    text: "×";
                                    font-size: 16px;
                                    color: cl_ta.has-hover ? #ffffff : #666666;
                                    horizontal-alignment: center;
                                    vertical-alignment: center;
                                }
                                cl_ta := TouchArea {
                                    clicked => {
                                        root.markdown_view_enabled = false;
                                        root.syntax_view_enabled = false;
                                        root.markdown_split_enabled = false;
                                        root.show_second_column = false;
                                    }
                                    mouse-cursor: pointer;
                                }
                            }
                        }

                        Rectangle {
                            y: parent.height - 1px;
                            height: 1px;
                            width: 100%;
                            background: #3e3e42;
                        }
                    }

                    // Contenido columna 2: Modo Texto 100% Seleccionable y Copiable con Mouse/Ctrl+C
                    if root.preview_selectable_mode: TextEdit {
                        text: root.preview_text;
                        read-only: true;
                        font-size: root.editor_font_size * 1px;
                        font-family: "Consolas, monospace";
                        horizontal-stretch: 1;
                        vertical-stretch: 1;
                    }

                    // Modo Alternativo: Lista visual con colores y bloques estilizados
                    if !root.preview_selectable_mode: ListView {
                        vertical-stretch: 1;
                        for line in root.preview_items: Rectangle {
                            background: line.bg_color;
                            min-height: 20px;

                            // Borde izquierdo (blockquote, etc.)
                            if line.has_border_left: Rectangle {
                                x: 0px;
                                y: 0px;
                                width: 3px;
                                height: parent.height;
                                background: line.border_left_color;
                            }

                            // Borde inferior (encabezados, etc.)
                            if line.has_border_bottom: Rectangle {
                                x: 0px;
                                y: parent.height - 1px;
                                width: parent.width;
                                height: 1px;
                                background: line.border_bottom_color;
                            }

                            HorizontalLayout {
                                padding-left: line.padding_left;
                                padding-top: line.padding_top;
                                padding-bottom: line.padding_bottom;
                                padding-right: 8px;

                                Text {
                                    text: line.text;
                                    color: line.color;
                                    font-size: line.font_size;
                                    font-weight: line.is_bold ? 700 : 400;
                                    font-family: line.is_mono ? "Consolas, monospace" : "Segoe UI, sans-serif";
                                    vertical-alignment: center;
                                    wrap: word-wrap;
                                }
                            }
                        }
                    }
                }
            }

            // ── BARRA DE ESTADO (estilo Sublime Text) ────────────
            Rectangle {
                height: 22px;
                background: #007acc;

                HorizontalLayout {
                    padding-left: 8px;
                    padding-right: 8px;
                    spacing: 0px;

                    Text {
                        text: root.status_text;
                        font-size: 11px;
                        color: #ffffff;
                        vertical-alignment: center;
                        horizontal-stretch: 1;
                        overflow: elide;
                    }

                    // Stats + lenguaje
                    Text {
                        text: root.doc_stats + "   " + root.detected_lang;
                        font-size: 11px;
                        color: #d0e8ff;
                        vertical-alignment: center;
                    }
                }
            }
        }

        // ── OVERLAYS Y DROPDOWNS (z-layer) ────────────────────────

        // Overlay para cerrar menús al clic fuera
        if root.active_dropdown >= 0: TouchArea {
            x: 0px;
            y: 0px;
            width: 100%;
            height: 100%;
            z: 80;
            clicked => { root.active_dropdown = -1; }
        }

        // ── Dropdown Archivo ──────────────────────────────────────────
        if root.active_dropdown == 0: Rectangle {
            x: 2px;
            y: 28px;
            width: 250px;
            height: 298px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                DropdownMenuItem { text: "Nuevo Archivo"; shortcut: "Ctrl+N"; clicked => { root.active_dropdown = -1; root.new-tab(); } }
                DropdownMenuItem { text: "Abrir Archivo..."; shortcut: "Ctrl+O"; clicked => { root.active_dropdown = -1; root.open-file(); } }
                DropdownMenuItem { text: "Abrir Carpeta..."; shortcut: ""; clicked => { root.active_dropdown = -1; root.open-folder(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Guardar"; shortcut: "Ctrl+S"; clicked => { root.active_dropdown = -1; root.save-scratch(); } }
                DropdownMenuItem { text: "Guardar Como..."; shortcut: "Ctrl+Shift+S"; clicked => { root.active_dropdown = -1; root.save-as(); } }
                DropdownMenuItem { text: "Guardar Todo"; shortcut: "Ctrl+Shift+A"; clicked => { root.active_dropdown = -1; root.save-all(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Cerrar Pestaña"; shortcut: "Ctrl+W"; clicked => { root.active_dropdown = -1; root.close-tab(root.active_tab); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Guardar Sesión"; shortcut: ""; clicked => { root.active_dropdown = -1; root.save-session(); } }
                DropdownMenuItem { text: "Restaurar Sesión"; shortcut: ""; clicked => { root.active_dropdown = -1; root.restore-session(); } }
                DropdownMenuItem { text: "Exportar a ZIP..."; shortcut: ""; clicked => { root.active_dropdown = -1; root.export-archive(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Configuración..."; shortcut: ""; clicked => { root.active_dropdown = -1; root.show_settings = true; } }
            }
        }

        // ── Dropdown Editar ───────────────────────────────────────────
        if root.active_dropdown == 1: Rectangle {
            x: 60px;
            y: 28px;
            width: 250px;
            height: 190px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                DropdownMenuItem { text: "Deshacer"; shortcut: "Ctrl+Z"; clicked => { root.active_dropdown = -1; root.undo(); } }
                DropdownMenuItem { text: "Rehacer"; shortcut: "Ctrl+Y"; clicked => { root.active_dropdown = -1; root.redo(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Copiar Todo"; shortcut: "Ctrl+C"; clicked => { root.active_dropdown = -1; root.copy-all-text(); } }
                DropdownMenuItem { text: "Pegar desde Portapapeles"; shortcut: "Ctrl+V"; clicked => { root.active_dropdown = -1; root.paste-clipboard(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Buscar y Reemplazar"; shortcut: "Ctrl+F"; clicked => { root.active_dropdown = -1; root.toggle-search-panel(); } }
                DropdownMenuItem { text: "Formatear Documento"; shortcut: "Ctrl+Shift+F"; clicked => { root.active_dropdown = -1; root.format-document(); } }
            }
        }

        // ── Dropdown Selección ─────────────────────────────────────
        if root.active_dropdown == 5: Rectangle {
            x: 120px;
            y: 28px;
            width: 260px;
            height: 238px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                DropdownMenuItem { text: "Copiar Todo al Portapapeles"; shortcut: "Ctrl+C"; clicked => { root.active_dropdown = -1; root.copy-all-text(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Duplicar Línea"; shortcut: "Ctrl+Shift+D"; clicked => { root.active_dropdown = -1; root.duplicate-line(); } }
                DropdownMenuItem { text: "Eliminar Línea"; shortcut: "Ctrl+Shift+K"; clicked => { root.active_dropdown = -1; root.delete-line(); } }
                DropdownMenuItem { text: "Mover Línea Arriba"; shortcut: "Alt+Up"; clicked => { root.active_dropdown = -1; root.move-line-up(); } }
                DropdownMenuItem { text: "Mover Línea Abajo"; shortcut: "Alt+Down"; clicked => { root.active_dropdown = -1; root.move-line-down(); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { text: "Comentar / Descomentar"; shortcut: "Ctrl+/"; clicked => { root.active_dropdown = -1; root.toggle-comment(); } }
                DropdownMenuItem { text: "Indentar Bloque"; shortcut: "Tab"; clicked => { root.active_dropdown = -1; root.indent-line(); } }
                DropdownMenuItem { text: "Desindentar Bloque"; shortcut: "Shift+Tab"; clicked => { root.active_dropdown = -1; root.unindent-line(); } }
                DropdownMenuItem { text: "Unir Líneas"; shortcut: "Ctrl+J"; clicked => { root.active_dropdown = -1; root.join-lines(); } }
            }
        }

        // ── Dropdown Ver ───────────────────────────────────────────
        if root.active_dropdown == 2: Rectangle {
            x: 186px;
            y: 28px;
            width: 280px;
            height: 221px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                // Toolbar toggle
                DropdownMenuItem {
                    is_checked: root.show_toolbar;
                    text: "Barra de Herramientas";
                    shortcut: "";
                    clicked => {
                        root.active_dropdown = -1;
                        root.show_toolbar = !root.show_toolbar;
                    }
                }
                DropdownMenuItem {
                    is_checked: root.show_explorer;
                    text: "Explorador de Archivos";
                    shortcut: "Ctrl+B";
                    clicked => { root.active_dropdown = -1; root.toggle-explorer(); }
                }
                DropdownMenuItem { is_separator: true; }
                // Layout
                DropdownMenuItem {
                    is_checked: root.group_count == 1 && !root.show_second_column;
                    text: "Una columna (Ctrl+1)";
                    shortcut: "";
                    clicked => {
                        root.active_dropdown = -1;
                        root.show_second_column = false;
                        root.markdown_view_enabled = false;
                        root.syntax_view_enabled = false;
                        root.markdown_split_enabled = false;
                    }
                }
                DropdownMenuItem {
                    is_checked: root.show_second_column;
                    text: "Dos columnas (Ctrl+D)";
                    shortcut: "Ctrl+D";
                    clicked => {
                        root.active_dropdown = -1;
                        root.show_second_column = !root.show_second_column;
                        if root.show_second_column {
                            root.markdown_view_enabled = true;
                        }
                    }
                }
                DropdownMenuItem { is_separator: true; }
                // Vistas
                DropdownMenuItem {
                    is_checked: root.markdown_view_enabled && !root.syntax_view_enabled;
                    text: "Vista Previa (Markdown / HTML)";
                    shortcut: "Ctrl+M";
                    clicked => {
                        root.active_dropdown = -1;
                        root.syntax_view_enabled = false;
                        root.toggle-markdown-view();
                    }
                }
                DropdownMenuItem {
                    is_checked: root.syntax_view_enabled;
                    text: "Vista de Sintaxis (Tokens)";
                    shortcut: "";
                    clicked => { root.active_dropdown = -1; root.toggle-syntax-view(); }
                }
                DropdownMenuItem {
                    text: "Abrir Vista Previa en Navegador";
                    shortcut: "";
                    clicked => { root.active_dropdown = -1; root.open-browser-preview(); }
                }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem {
                    is_checked: root.is_fullscreen;
                    text: "Pantalla Completa";
                    shortcut: "F11";
                    clicked => { root.active_dropdown = -1; root.toggle-fullscreen(); }
                }
            }
        }

        // ── Dropdown Grupos ────────────────────────────────────────
        if root.active_dropdown == 3: Rectangle {
            x: 246px;
            y: 28px;
            width: 220px;
            height: 207px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                DropdownMenuItem { is_checked: root.active_group == 0; text: "Ir a Grupo 1"; shortcut: "Alt+1"; clicked => { root.active_dropdown = -1; root.switch-group(0); } }
                DropdownMenuItem { is_checked: root.active_group == 1; text: "Ir a Grupo 2"; shortcut: "Alt+2"; clicked => { root.active_dropdown = -1; root.switch-group(1); } }
                DropdownMenuItem { is_checked: root.active_group == 2; text: "Ir a Grupo 3"; shortcut: "Alt+3"; clicked => { root.active_dropdown = -1; root.switch-group(2); } }
                DropdownMenuItem { is_checked: root.active_group == 3; text: "Ir a Grupo 4"; shortcut: "Alt+4"; clicked => { root.active_dropdown = -1; root.switch-group(3); } }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem { is_checked: root.group_count == 1; text: "Disposición: 1 Grupo"; shortcut: ""; clicked => { root.active_dropdown = -1; root.set-group-count(1); } }
                DropdownMenuItem { is_checked: root.group_count == 2; text: "Disposición: 2 Grupos"; shortcut: ""; clicked => { root.active_dropdown = -1; root.set-group-count(2); } }
                DropdownMenuItem { is_checked: root.group_count == 3; text: "Disposición: 3 Grupos"; shortcut: ""; clicked => { root.active_dropdown = -1; root.set-group-count(3); } }
                DropdownMenuItem { is_checked: root.group_count == 4; text: "Disposición: 4 Grupos"; shortcut: ""; clicked => { root.active_dropdown = -1; root.set-group-count(4); } }
            }
        }

        // ── Dropdown Ayuda ────────────────────────────────────────────
        if root.active_dropdown == 4: Rectangle {
            x: 306px;
            y: 28px;
            width: 280px;
            height: 87px;
            z: 90;
            background: #252526;
            border-width: 1px;
            border-color: #454545;
            border-radius: 4px;
            drop-shadow-blur: 12px;
            drop-shadow-color: #000000cc;

            VerticalLayout {
                padding: 4px;
                spacing: 0px;

                DropdownMenuItem {
                    text: "¿Cómo funciona el Autoguardado?";
                    shortcut: "";
                    clicked => { root.active_dropdown = -1; root.show_scratch_help = true; }
                }
                DropdownMenuItem {
                    text: "Ver Atajos de Teclado";
                    shortcut: "";
                    clicked => {
                        root.active_dropdown = -1;
                        root.status_text = "Ctrl+N Nuevo  Ctrl+S Guardar  Ctrl+F Buscar  Ctrl+M Vista Previa  Ctrl+D 2 Cols  F11 Pantalla";
                    }
                }
                DropdownMenuItem { is_separator: true; }
                DropdownMenuItem {
                    text: "Acerca de LightMark";
                    shortcut: "";
                    clicked => {
                        root.active_dropdown = -1;
                        root.status_text = "LightMark v0.1.0 — Editor rápido de texto y notas en Rust + Slint";
                    }
                }
            }
        }

        // ── Modal: Configuración ───────────────────────────────────
        if root.show_settings: TouchArea {
            x: 0px; y: 0px; width: 100%; height: 100%;
            z: 150;
            Rectangle { background: #00000088; }
            clicked => { root.show_settings = false; }
        }
        if root.show_settings: Rectangle {
            x: (parent.width - 480px) / 2;
            y: (parent.height - 360px) / 2;
            width: 480px;
            height: 360px;
            z: 160;
            background: #252526;
            border-width: 1px;
            border-color: #007acc;
            border-radius: 6px;
            drop-shadow-blur: 20px;
            drop-shadow-color: #000000cc;

            VerticalBox {
                padding: 18px;
                spacing: 12px;

                HorizontalBox {
                    height: 28px;
                    Text { text: "Configuración de LightMark"; font-size: 15px; font-weight: 700; color: #ffffff; vertical-alignment: center; }
                    Rectangle { horizontal-stretch: 1; }
                    Button { text: "×"; width: 28px; clicked => { root.show_settings = false; } }
                }

                Rectangle { height: 1px; background: #3e3e42; }

                VerticalBox {
                    spacing: 10px;
                    padding: 0px;

                    Text { text: "Autoguardado:"; font-size: 12px; font-weight: 600; color: #aaaaaa; }
                    HorizontalBox {
                        spacing: 6px; padding: 0px;
                        Button { text: root.autosave_delay_ms == 500 ? "● 500ms" : "500ms"; clicked => { root.autosave_delay_ms = 500; } }
                        Button { text: root.autosave_delay_ms == 1000 ? "● 1s" : "1s"; clicked => { root.autosave_delay_ms = 1000; } }
                        Button { text: root.autosave_delay_ms == 2000 ? "● 2s" : "2s"; clicked => { root.autosave_delay_ms = 2000; } }
                        Button { text: root.autosave_delay_ms == 5000 ? "● 5s" : "5s"; clicked => { root.autosave_delay_ms = 5000; } }
                    }

                    Text { text: "Tamaño de Fuente:"; font-size: 12px; font-weight: 600; color: #aaaaaa; }
                    HorizontalBox {
                        spacing: 6px; padding: 0px;
                        Button { text: root.editor_font_size == 12 ? "● 12" : "12"; clicked => { root.editor_font_size = 12; } }
                        Button { text: root.editor_font_size == 13 ? "● 13" : "13"; clicked => { root.editor_font_size = 13; } }
                        Button { text: root.editor_font_size == 14 ? "● 14" : "14"; clicked => { root.editor_font_size = 14; } }
                        Button { text: root.editor_font_size == 15 ? "● 15" : "15"; clicked => { root.editor_font_size = 15; } }
                        Button { text: root.editor_font_size == 16 ? "● 16" : "16"; clicked => { root.editor_font_size = 16; } }
                        Button { text: root.editor_font_size == 18 ? "● 18" : "18"; clicked => { root.editor_font_size = 18; } }
                    }

                    Text { text: "Barra de Herramientas:"; font-size: 12px; font-weight: 600; color: #aaaaaa; }
                    HorizontalBox {
                        spacing: 6px; padding: 0px;
                        Button { text: root.show_toolbar ? "● Visible" : "Oculta"; clicked => { root.show_toolbar = !root.show_toolbar; } }
                    }

                    Text { text: "Datos de la Aplicación:"; font-size: 12px; font-weight: 600; color: #aaaaaa; }
                    HorizontalBox {
                        spacing: 6px; padding: 0px;
                        Button { text: "Abrir Carpeta de Datos"; clicked => { root.open-app-folder(); } }
                    }
                }

                Rectangle { vertical-stretch: 1; }
                HorizontalBox {
                    alignment: end;
                    Button { text: "Cerrar"; clicked => { root.show_settings = false; } }
                }
            }
        }

        // ── Modal: Scratch Help ────────────────────────────────────
        if root.show_scratch_help: TouchArea {
            x: 0px; y: 0px; width: 100%; height: 100%;
            z: 150;
            Rectangle { background: #00000088; }
            clicked => { root.show_scratch_help = false; }
        }
        if root.show_scratch_help: Rectangle {
            x: (parent.width - 560px) / 2;
            y: (parent.height - 400px) / 2;
            width: 560px;
            height: 400px;
            z: 160;
            background: #252526;
            border-width: 1px;
            border-color: #007acc;
            border-radius: 6px;
            drop-shadow-blur: 20px;
            drop-shadow-color: #000000cc;

            VerticalBox {
                padding: 18px;
                spacing: 12px;

                HorizontalBox {
                    height: 28px;
                    Text { text: "¿Qué son las Notas Scratch?"; font-size: 15px; font-weight: 700; color: #ffffff; vertical-alignment: center; }
                    Rectangle { horizontal-stretch: 1; }
                    Button { text: "×"; width: 28px; clicked => { root.show_scratch_help = false; } }
                }

                Rectangle { height: 1px; background: #3e3e42; }

                TextEdit {
                    read-only: true;
                    font-size: 12px;
                    text: "CONCEPTO:\nTodos los documentos abiertos funcionan como borradores persistentes (estilo Sublime Text) para capturar ideas, notas o código sin fricción.\n\nCÓMO FUNCIONA EL AUTOGUARDADO:\n• Guardado continuo: Tus documentos se preservan automáticamente. Nunca pierdes contenido aunque cierres la aplicación.\n• Guardar (Ctrl+S): Si el documento no tiene archivo en disco, abre 'Guardar Como' para asignarle nombre. Si ya tiene archivo, guarda directamente.\n• Detección de sintaxis: Identifica automáticamente Markdown, HTML, JSON, Rust, Python, etc.\n\nRESPALDO:\n   Exportar -> Empaquetar los borradores en un archivo ZIP de respaldo.";
                }

                HorizontalBox {
                    alignment: end;
                    Button { text: "Entendido"; clicked => { root.show_scratch_help = false; } }
                }
            }
        }
    }
}

#[derive(Clone)]
struct Tab {
    title: SharedString,
    document: Rope,
    is_scratch: bool,
    scratch_id: Option<String>,
    undo_stack: UndoStack,
    file_path: Option<std::path::PathBuf>,
    last_known_mtime: Option<SystemTime>,
    last_undo_push: std::time::Instant,
}

#[derive(Clone)]
struct UndoStack {
    history: Vec<Rope>,
    position: usize,
}

impl UndoStack {
    fn new(document: Rope) -> Self {
        Self {
            history: vec![document],
            position: 0,
        }
    }

    fn push(&mut self, document: Rope) {
        // Remove redo history (states after current position)
        self.history.truncate(self.position + 1);
        // Avoid duplicates
        if self.history.last() != Some(&document) {
            self.history.push(document);
            self.position += 1;
            // Cap history to 200 states to prevent excessive memory usage
            if self.history.len() > 200 {
                self.history.remove(0);
                self.position = self.position.saturating_sub(1);
            }
        }
    }

    fn undo(&mut self) -> Option<Rope> {
        if self.position > 0 {
            self.position -= 1;
            self.history.get(self.position).cloned()
        } else {
            None
        }
    }

    fn redo(&mut self) -> Option<Rope> {
        if self.position < self.history.len() - 1 {
            self.position += 1;
            self.history.get(self.position).cloned()
        } else {
            None
        }
    }

    fn can_undo(&self) -> bool {
        self.position > 0
    }

    fn can_redo(&self) -> bool {
        self.position < self.history.len() - 1
    }
}

impl Tab {
    fn new_regular(title: &str, document: Rope) -> Self {
        Self {
            title: title.into(),
            document: document.clone(),
            is_scratch: false,
            scratch_id: None,
            undo_stack: UndoStack::new(document),
            file_path: None,
            last_known_mtime: None,
            last_undo_push: std::time::Instant::now(),
        }
    }

    fn new_scratch(title: &str, document: Rope, scratch_id: String) -> Self {
        Self {
            title: title.into(),
            document: document.clone(),
            is_scratch: true,
            scratch_id: Some(scratch_id),
            undo_stack: UndoStack::new(document),
            file_path: None,
            last_known_mtime: None,
            last_undo_push: std::time::Instant::now(),
        }
    }
}

struct Group {
    tabs: Vec<Tab>,
    active_tab: usize,
}

struct EditorState {
    groups: Vec<Group>,
    active_group: usize,
    scratch_manager: ScratchManager,
    session_manager: SessionManager,
    last_save: SystemTime,
    workspace_root: Option<std::path::PathBuf>,
    search_results: Vec<search::SearchResult>,
    search_index: usize,
    dirty: bool,
    last_saved_instant: std::time::Instant,
}

impl EditorState {
    fn new() -> Self {
        let scratch_dir = scratch::ensure_scratch_dirs();
        let app_data = scratch::get_app_data_dir();
        let sessions_dir = session::ensure_sessions_dir(&app_data);
        let sm = ScratchManager::new(scratch_dir);
        let draft_id = sm.next_id_public();
        let tabs = vec![Tab::new_scratch("Sin título 1", Rope::new(), draft_id)];
        let groups = vec![Group {
            tabs,
            active_tab: 0,
        }];
        Self {
            groups,
            active_group: 0,
            scratch_manager: sm,
            session_manager: SessionManager::new(sessions_dir),
            last_save: SystemTime::now(),
            workspace_root: None,
            search_results: Vec::new(),
            search_index: 0,
            dirty: false,
            last_saved_instant: std::time::Instant::now(),
        }
    }

    fn active_group_tabs(&self) -> &Vec<Tab> {
        &self.groups[self.active_group].tabs
    }

    fn tab_titles(&self) -> Vec<SharedString> {
        self.active_group_tabs()
            .iter()
            .map(|tab| tab.title.clone())
            .collect()
    }

    fn document_text(&self) -> SharedString {
        if self.groups.is_empty() || self.active_group >= self.groups.len() {
            return "".into();
        }
        let group = &self.groups[self.active_group];
        if group.tabs.is_empty() || group.active_tab >= group.tabs.len() {
            return "".into();
        }
        group.tabs[group.active_tab].document.to_string().into()
    }

    fn detected_language(&self) -> String {
        if self.groups.is_empty() || self.active_group >= self.groups.len() {
            return "plaintext".to_string();
        }
        let group = &self.groups[self.active_group];
        if group.tabs.is_empty() || group.active_tab >= group.tabs.len() {
            return "plaintext".to_string();
        }
        let tab = &group.tabs[group.active_tab];
        if let Some(ref path) = tab.file_path {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                match ext.to_lowercase().as_str() {
                    "md" | "markdown" => "markdown".to_string(),
                    "json" => "json".to_string(),
                    "sql" => "sql".to_string(),
                    "html" | "htm" => "html".to_string(),
                    "xml" => "xml".to_string(),
                    "js" | "mjs" | "cjs" => "javascript".to_string(),
                    _ => "plaintext".to_string(),
                }
            } else {
                "plaintext".to_string()
            }
        } else if tab.is_scratch {
            let content = tab.document.to_string();
            scratch::detect_language(&content).unwrap_or("markdown".to_string())
        } else {
            "plaintext".to_string()
        }
    }

    fn to_session(&self) -> Session {
        let mut groups = Vec::new();
        for (i, group) in self.groups.iter().enumerate() {
            let tabs: Vec<TabState> = group
                .tabs
                .iter()
                .map(|tab| TabState {
                    document_id: tab
                        .scratch_id
                        .clone()
                        .unwrap_or_else(|| format!("tab-{}", i)),
                    path: tab.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    title: tab.title.to_string(),
                    is_scratch: tab.is_scratch,
                    cursor: 0,
                    scroll_line: 0,
                    pinned: false,
                })
                .collect();
            groups.push(session::GroupState {
                id: i,
                tabs,
                active_tab: group.active_tab,
            });
        }
        Session {
            version: 1,
            name: "Restored Session".to_string(),
            created_at: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            groups,
            active_group: self.active_group,
            group_count: self.groups.len(),
            workspace_root: self
                .workspace_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        }
    }

    fn restore_from_session(&mut self, session: &Session) {
        self.groups.clear();
        let scratch_dir = self.scratch_manager.base_dir().to_path_buf();

        for group_state in &session.groups {
            let tabs: Vec<Tab> = group_state
                .tabs
                .iter()
                .map(|tab_state| {
                    if tab_state.is_scratch {
                        let scratch_path = scratch_dir.join(format!("{}.md", tab_state.document_id));
                        let rope = if let Ok(content) = std::fs::read_to_string(&scratch_path) {
                            Rope::from_str(&content)
                        } else {
                            Rope::new()
                        };
                        let title = if tab_state.title.starts_with("Scratch") || tab_state.title.is_empty() {
                            "Sin título".to_string()
                        } else {
                            tab_state.title.clone()
                        };
                        Tab::new_scratch(&title, rope, tab_state.document_id.clone())
                    } else if let Some(ref path_str) = tab_state.path {
                        let path = std::path::PathBuf::from(path_str);
                        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                        let rope = if let Ok(content) = std::fs::read_to_string(&path) {
                            Rope::from_str(&content)
                        } else {
                            Rope::new()
                        };
                        let mut tab = Tab::new_regular(&tab_state.title, rope);
                        tab.file_path = Some(path);
                        tab.last_known_mtime = mtime;
                        tab
                    } else {
                        Tab::new_regular(&tab_state.title, Rope::new())
                    }
                })
                .collect();

            self.groups.push(Group {
                tabs: if tabs.is_empty() {
                    vec![Tab::new_regular("Tab 1", Rope::from("Hello, world!\n"))]
                } else {
                    tabs
                },
                active_tab: group_state.active_tab,
            });
        }

        if self.groups.is_empty() {
            self.groups.push(Group {
                tabs: vec![Tab::new_regular("Tab 1", Rope::from("Hello, world!\n"))],
                active_tab: 0,
            });
        }

        self.active_group = session.active_group.min(self.groups.len() - 1);

        if let Some(ref root) = session.workspace_root {
            self.workspace_root = Some(std::path::PathBuf::from(root));
        }
    }
}


fn c_rgb(r: u8, g: u8, b: u8) -> slint::Color {
    slint::Color::from_argb_u8(255, r, g, b)
}

fn c_rgba(r: u8, g: u8, b: u8, a: u8) -> slint::Color {
    slint::Color::from_argb_u8(a, r, g, b)
}

fn build_preview_items(content: &str, lang: &str, syntax_mode: bool) -> Vec<PreviewLine> {
    let c_trans = c_rgba(0, 0, 0, 0);
    let c_white = c_rgb(255, 255, 255);
    let c_text = c_rgb(212, 212, 212);
    let c_blue_h2 = c_rgb(86, 156, 214);
    let c_blue_h3 = c_rgb(79, 193, 255);
    let c_border = c_rgb(62, 62, 66);
    let c_code = c_rgb(206, 145, 120);
    let c_code_bg = c_rgb(37, 37, 38);
    let c_quote = c_rgb(187, 187, 187);
    let c_quote_border = c_rgb(0, 122, 204);
    let c_green = c_rgb(61, 201, 160);
    let c_gray = c_rgb(136, 136, 136);
    let c_table_head = c_rgb(45, 45, 48);
    let c_tag = c_rgb(86, 156, 214);
    let c_keyword = c_rgb(197, 134, 192);
    let c_number = c_rgb(181, 206, 168);
    let c_comment = c_rgb(106, 153, 85);
    let c_url = c_rgb(55, 148, 255);

    let mut items = Vec::new();

    if syntax_mode {
        // --- VISTA DE SINTAXIS POR COLORES (Desglose léxico por líneas) ---
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate().take(500) {
            let line_num = i + 1;
            let trimmed = line.trim();

            let (color, is_bold) = if trimmed.starts_with('#') {
                (c_blue_h2, true)
            } else if trimmed.starts_with("<!--") || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                (c_comment, false)
            } else if trimmed.starts_with('<') && trimmed.contains('>') {
                (c_tag, false)
            } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                (c_code, false)
            } else if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                (c_number, false)
            } else if trimmed.starts_with("select ") || trimmed.starts_with("create ") || trimmed.starts_with("fn ") || trimmed.starts_with("let ") || trimmed.starts_with("import ") {
                (c_keyword, true)
            } else if trimmed.contains('=') && (trimmed.contains('"') || trimmed.contains('\'')) {
                (c_tag, false)
            } else {
                (c_text, false)
            };

            items.push(PreviewLine {
                text: format!("{:3} │ {}", line_num, line).into(),
                color,
                bg_color: c_trans,
                font_size: 13.0,
                is_bold,
                is_mono: true,
                has_border_left: false,
                border_left_color: c_trans,
                has_border_bottom: false,
                border_bottom_color: c_trans,
                padding_left: 6.0,
                padding_top: 1.0,
                padding_bottom: 1.0,
            });
        }
    } else if lang == "html" || (content.trim_start().starts_with('<') && content.contains('>')) {
        // --- VISTA PREVIA HTML CON ESTILOS Y COLORES ---
        let lines: Vec<&str> = content.lines().collect();
        for line in lines.iter().take(500) {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();

            if lower.starts_with("<h1") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_white,
                    bg_color: c_trans,
                    font_size: 18.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 10.0,
                    padding_bottom: 6.0,
                });
            } else if lower.starts_with("<h2") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 15.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 8.0,
                    padding_bottom: 4.0,
                });
            } else if lower.starts_with("<h3") || lower.starts_with("<h4") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("▸ {}", inner).into(),
                    color: c_blue_h3,
                    bg_color: c_trans,
                    font_size: 13.5,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 2.0,
                });
            } else if lower.starts_with("<blockquote") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: inner.into(),
                    color: c_quote,
                    bg_color: c_code_bg,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: true,
                    border_left_color: c_quote_border,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 12.0,
                    padding_top: 4.0,
                    padding_bottom: 4.0,
                });
            } else if lower.starts_with("<pre") || lower.starts_with("<code") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("  {}", inner).into(),
                    color: c_code,
                    bg_color: c_code_bg,
                    font_size: 12.5,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 8.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else if lower.starts_with("<li") {
                let inner = extract_inner_text(trimmed);
                items.push(PreviewLine {
                    text: format!("  • {}", inner).into(),
                    color: c_text,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else {
                let inner = extract_inner_text(trimmed);
                if !inner.is_empty() {
                    items.push(PreviewLine {
                        text: inner.into(),
                        color: c_text,
                        bg_color: c_trans,
                        font_size: 13.0,
                        is_bold: false,
                        is_mono: false,
                        has_border_left: false,
                        border_left_color: c_trans,
                        has_border_bottom: false,
                        border_bottom_color: c_trans,
                        padding_left: 6.0,
                        padding_top: 2.0,
                        padding_bottom: 2.0,
                    });
                }
            }
        }
    } else {
        // --- VISTA PREVIA MARKDOWN FORMATEADA CON COLORES Y TABLAS ---
        let rendered = markdown::render_markdown_preview(content);
        let raw_lines: Vec<&str> = rendered.lines().collect();
        let mut in_code_block = false;
        let mut in_table = false;
        let mut is_table_header = false;

        let mut i = 0;
        while i < raw_lines.len() && items.len() < 500 {
            let line = raw_lines[i];

            // Check if next line is an H1 underline (─────)
            if i + 1 < raw_lines.len() && raw_lines[i + 1].starts_with("──────") && !line.starts_with('┌') && !line.starts_with('├') && !line.starts_with('└') {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_white,
                    bg_color: c_trans,
                    font_size: 18.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 10.0,
                    padding_bottom: 6.0,
                });
                i += 2;
                continue;
            }

            // Check if next line is an H2 underline (┈┈┈┈)
            if i + 1 < raw_lines.len() && raw_lines[i + 1].starts_with("┈┈┈┈") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 15.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: true,
                    border_bottom_color: c_border,
                    padding_left: 6.0,
                    padding_top: 8.0,
                    padding_bottom: 4.0,
                });
                i += 2;
                continue;
            }

            // H3
            if line.starts_with("▸ ") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h3,
                    bg_color: c_trans,
                    font_size: 14.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            // Code block start
            if line.starts_with("┌─── Código") {
                in_code_block = true;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_comment,
                    bg_color: c_code_bg,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            // Code block end
            if in_code_block && line.starts_with("└────────────────") {
                in_code_block = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_comment,
                    bg_color: c_code_bg,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 6.0,
                });
                i += 1;
                continue;
            }

            // Code block line
            if in_code_block {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_code,
                    bg_color: c_code_bg,
                    font_size: 12.5,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            // Table borders
            if line.starts_with("┌──") && line.contains('┬') {
                in_table = true;
                is_table_header = true;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 6.0,
                    padding_bottom: 0.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("├──") && line.contains('┼') {
                is_table_header = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 0.0,
                    padding_bottom: 0.0,
                });
                i += 1;
                continue;
            }

            if line.starts_with("└──") && line.contains('┴') {
                in_table = false;
                is_table_header = false;
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_blue_h2,
                    bg_color: c_trans,
                    font_size: 12.0,
                    is_bold: false,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 0.0,
                    padding_bottom: 6.0,
                });
                i += 1;
                continue;
            }

            // Table row
            if in_table && line.starts_with('│') {
                let bg_color = if is_table_header { c_table_head } else { c_trans };
                let color = if is_table_header { c_white } else { c_text };
                items.push(PreviewLine {
                    text: line.into(),
                    color,
                    bg_color,
                    font_size: 12.0,
                    is_bold: is_table_header,
                    is_mono: true,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 1.0,
                    padding_bottom: 1.0,
                });
                i += 1;
                continue;
            }

            // Blockquote
            if line.starts_with("  │ ") {
                let quote_text = line.strip_prefix("  │ ").unwrap_or(line);
                items.push(PreviewLine {
                    text: quote_text.into(),
                    color: c_quote,
                    bg_color: c_code_bg,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: true,
                    border_left_color: c_quote_border,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 12.0,
                    padding_top: 3.0,
                    padding_bottom: 3.0,
                });
                i += 1;
                continue;
            }

            // Task list done
            if line.contains("[✓]") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_green,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: true,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            // Task list pending
            if line.contains("[ ]") {
                items.push(PreviewLine {
                    text: line.into(),
                    color: c_gray,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
                i += 1;
                continue;
            }

            // Empty line spacer
            if line.trim().is_empty() {
                items.push(PreviewLine {
                    text: " ".into(),
                    color: c_trans,
                    bg_color: c_trans,
                    font_size: 6.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 0.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            } else {
                let is_link = line.contains("🔗");
                let color = if is_link { c_url } else { c_text };
                items.push(PreviewLine {
                    text: line.into(),
                    color,
                    bg_color: c_trans,
                    font_size: 13.0,
                    is_bold: false,
                    is_mono: false,
                    has_border_left: false,
                    border_left_color: c_trans,
                    has_border_bottom: false,
                    border_bottom_color: c_trans,
                    padding_left: 6.0,
                    padding_top: 2.0,
                    padding_bottom: 2.0,
                });
            }

            i += 1;
        }
    }

    if items.is_empty() {
        items.push(PreviewLine {
            text: "(Documento vacío)".into(),
            color: c_gray,
            bg_color: c_trans,
            font_size: 13.0,
            is_bold: false,
            is_mono: false,
            has_border_left: false,
            border_left_color: c_trans,
            has_border_bottom: false,
            border_bottom_color: c_trans,
            padding_left: 6.0,
            padding_top: 10.0,
            padding_bottom: 10.0,
        });
    }

    items
}

fn extract_inner_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn generate_line_numbers(text: &str) -> String {
    let lines = text.lines().count();
    if lines == 0 {
        "1".to_string()
    } else {
        (1..=lines)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}



fn duplicate_last_line(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(&last) = lines.last() {
        lines.push(last);
        let mut res = lines.join("\n");
        if text.ends_with('\n') {
            res.push('\n');
        }
        res
    } else {
        text.to_string()
    }
}

fn delete_last_line(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if !lines.is_empty() {
        lines.pop();
        let mut res = lines.join("\n");
        if text.ends_with('\n') && !res.is_empty() {
            res.push('\n');
        }
        res
    } else {
        String::new()
    }
}

fn move_line_up(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let len = lines.len();
    if len >= 2 {
        lines.swap(len - 1, len - 2);
        let mut res = lines.join("\n");
        if text.ends_with('\n') {
            res.push('\n');
        }
        res
    } else {
        text.to_string()
    }
}

fn move_line_down(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let len = lines.len();
    if len >= 2 {
        lines.swap(0, 1);
        let mut res = lines.join("\n");
        if text.ends_with('\n') {
            res.push('\n');
        }
        res
    } else {
        text.to_string()
    }
}

fn toggle_comment(text: &str, lang: &str) -> String {
    let prefix = match lang.to_lowercase().as_str() {
        "rust" | "js" | "javascript" | "ts" | "typescript" | "c" | "cpp" | "java" | "csharp" => "// ",
        "python" | "ruby" | "bash" | "sh" | "yaml" | "yml" | "toml" => "# ",
        "sql" => "-- ",
        "html" | "xml" | "markdown" | "md" => "<!-- ",
        _ => "// ",
    };
    let is_html_type = prefix == "<!-- ";

    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(prefix) {
            let indent = &line[..line.len() - trimmed.len()];
            let uncommented = if is_html_type && trimmed.ends_with(" -->") {
                &trimmed[prefix.len()..trimmed.len() - 4]
            } else {
                &trimmed[prefix.len()..]
            };
            lines.push(format!("{}{}", indent, uncommented));
        } else {
            let indent = &line[..line.len() - trimmed.len()];
            if is_html_type {
                lines.push(format!("{}{}{} -->", indent, prefix, trimmed));
            } else {
                lines.push(format!("{}{}{}", indent, prefix, trimmed));
            }
        }
    }
    let mut res = lines.join("\n");
    if text.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn indent_lines(text: &str) -> String {
    let lines: Vec<String> = text.lines().map(|l| format!("    {}", l)).collect();
    let mut res = lines.join("\n");
    if text.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn unindent_lines(text: &str) -> String {
    let lines: Vec<String> = text.lines().map(|l| {
        if l.starts_with("    ") {
            l[4..].to_string()
        } else if l.starts_with('\t') {
            l[1..].to_string()
        } else {
            l.trim_start_matches(' ').to_string()
        }
    }).collect();
    let mut res = lines.join("\n");
    if text.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn join_lines(text: &str) -> String {
    text.lines().collect::<Vec<&str>>().join(" ")
}

fn sync_preview(ui: &App, content: &str, lang: &str) {
    let is_syntax = ui.get_syntax_view_enabled();
    // Render full plain preview for the 100% selectable and copyable TextEdit
    let formatted_text = if is_syntax {
        highlight::render_syntax_view(content, lang)
    } else {
        markdown::render_preview(content, lang)
    };
    ui.set_preview_text(formatted_text.into());

    let items = build_preview_items(content, lang, is_syntax);
    let model = std::rc::Rc::new(slint::VecModel::from(items));
    ui.set_preview_items(model.into());
}

fn make_model(titles: Vec<SharedString>) -> slint::ModelRc<SharedString> {
    Rc::new(VecModel::from(titles)).into()
}

fn main() -> Result<(), slint::PlatformError> {
    let state = Rc::new(RefCell::new(EditorState::new()));
    let ui = App::new()?;

    // Try to restore auto session on startup
    {
        let mut s = state.borrow_mut();
        if let Ok(session) = s.session_manager.load_auto_session()
            && let Some(restored) = session
        {
            s.restore_from_session(&restored);
        }
    }

    // Initial state setup
    {
        let s = state.borrow();
        let initial_text = s.document_text();
        let initial_lang = s.detected_language();
        let initial_chars = initial_text.chars().count();
        let initial_words = initial_text.split_whitespace().count();
        let initial_lines = initial_text.lines().count().max(1);

        ui.set_tab_titles(make_model(s.tab_titles()));
        ui.set_tab_count(s.active_group_tabs().len() as i32);
        ui.set_documentText(initial_text.clone());
        ui.set_active_tab(0);
        ui.set_group_count(s.groups.len() as i32);
        ui.set_active_group(0);
        ui.set_markdown_split_enabled(false);
        ui.set_detected_lang(initial_lang.clone().into());
        ui.set_doc_stats(format!("Líneas: {} | Palabras: {} | Caracteres: {}", initial_lines, initial_words, initial_chars).into());
        ui.set_line_numbers(generate_line_numbers(initial_text.as_ref()).into());
        sync_preview(&ui, initial_text.as_ref(), &initial_lang);
        ui.set_status_text("Listo".into());
    }

    // Copy preview to clipboard (works with arboard)
    let ui_weak = ui.as_weak();
    ui.on_copy_preview(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let text = ui.get_preview_text();
            if let Ok(mut clip) = arboard::Clipboard::new() {
                if clip.set_text(text.to_string()).is_ok() {
                    ui.set_status_text("Vista previa copiada al portapapeles".into());
                } else {
                    ui.set_status_text("Error al copiar al portapapeles".into());
                }
            }
        }
    });

    // Copy all document text to clipboard
    let ui_weak = ui.as_weak();
    ui.on_copy_all_text(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let text = ui.get_documentText();
            if let Ok(mut clip) = arboard::Clipboard::new() {
                if clip.set_text(text.to_string()).is_ok() {
                    ui.set_status_text("Documento copiado al portapapeles".into());
                } else {
                    ui.set_status_text("Error al copiar al portapapeles".into());
                }
            }
        }
    });

    // Paste clipboard into document
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_paste_clipboard(move || {
        if let Some(ui) = ui_weak.upgrade() {
            if let Ok(mut clip) = arboard::Clipboard::new() {
                if let Ok(pasted) = clip.get_text() {
                    let mut s = state_clone.borrow_mut();
                    let ag = s.active_group;
                    let at = s.groups[ag].active_tab;
                    let prev_doc = s.groups[ag].tabs[at].document.clone();
                    s.groups[ag].tabs[at].undo_stack.push(prev_doc);
                    let mut current = ui.get_documentText().to_string();
                    current.push_str(&pasted);
                    s.groups[ag].tabs[at].document = Rope::from_str(&current);
                    s.dirty = true;
                    ui.set_documentText(current.into());
                    ui.set_status_text("Texto pegado desde el portapapeles".into());
                }
            }
        }
    });

    // Modern editor line operations
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_duplicate_line(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = duplicate_last_line(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Línea duplicada (Ctrl+Shift+D)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_delete_line(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = delete_last_line(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Línea eliminada (Ctrl+Shift+K)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_move_line_up(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = move_line_up(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Línea movida hacia arriba (Alt+Up)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_move_line_down(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = move_line_down(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Línea movida hacia abajo (Alt+Down)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_toggle_comment(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let lang = s.detected_language();
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = toggle_comment(&current, &lang);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Comentarios alternados (Ctrl+/)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_indent_line(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = indent_lines(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Bloque indentado (+4 espacios)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_unindent_line(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = unindent_lines(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Bloque desindentado (-4 espacios)".into());
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_join_lines(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let prev_doc = s.groups[ag].tabs[at].document.clone();
            s.groups[ag].tabs[at].undo_stack.push(prev_doc);
            let current = ui.get_documentText().to_string();
            let updated = join_lines(&current);
            s.groups[ag].tabs[at].document = Rope::from_str(&updated);
            s.dirty = true;
            ui.set_documentText(updated.into());
            ui.set_status_text("Líneas unidas (Ctrl+J)".into());
        }
    });

    // Handle tab click
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_tab_clicked(move |index| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            // Save current content back to the previous tab
            let current_text = ui.get_documentText();
            let ag = s.active_group;
            let old_active = s.groups[ag].active_tab;
            let new_doc = Rope::from_str(current_text.as_ref());
            if s.groups[ag].tabs[old_active].document != new_doc {
                s.groups[ag].tabs[old_active]
                    .undo_stack
                    .push(new_doc.clone());
            }
            s.groups[ag].tabs[old_active].document = new_doc;
            // Switch to the new tab
            s.groups[ag].active_tab = index as usize;
            let current_text = s.document_text();
            let lang = s.detected_language();
            let char_count = current_text.chars().count();
            let word_count = current_text.split_whitespace().count();
            let line_count = current_text.lines().count().max(1);

            ui.set_documentText(current_text.clone());
            ui.set_active_tab(s.groups[ag].active_tab as i32);
            ui.set_detected_lang(lang.clone().into());
            ui.set_doc_stats(format!("Líneas: {} | Palabras: {} | Caracteres: {}", line_count, word_count, char_count).into());
            ui.set_line_numbers(generate_line_numbers(current_text.as_ref()).into());

            // Update preview or syntax view if enabled
            if ui.get_markdown_view_enabled() || ui.get_markdown_split_enabled() || ui.get_syntax_view_enabled() || ui.get_show_second_column() {
                sync_preview(&ui, current_text.as_ref(), &lang);
            }
            let tab_title = &s.groups[ag].tabs[s.groups[ag].active_tab].title;
            ui.set_status_text(format!("{} | {}", tab_title, lang).into());
        }
    });

    // Handle new tab (Documento borrador con persistencia automática de sesión, estilo Sublime Text)
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_new_tab(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let draft_id = s.scratch_manager.next_id_public();
            let num = s.groups[ag].tabs.len() + 1;
            let title = format!("Sin título {}", num);
            let new_index = s.groups[ag].tabs.len();
            s.groups[ag].tabs.push(Tab::new_scratch(&title, Rope::new(), draft_id));
            s.groups[ag].active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(new_index as i32);
            let lang = s.detected_language();
            ui.set_status_text(format!("{} | {}", title, lang).into());
        }
    });

    // Handle new scratch (llama a la misma lógica unificada de nuevo documento)
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_new_scratch(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let draft_id = s.scratch_manager.next_id_public();
            let num = s.groups[ag].tabs.len() + 1;
            let title = format!("Sin título {}", num);
            let new_index = s.groups[ag].tabs.len();
            s.groups[ag].tabs.push(Tab::new_scratch(&title, Rope::new(), draft_id));
            s.groups[ag].active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(new_index as i32);
            let lang = s.detected_language();
            ui.set_status_text(format!("{} | {}", title, lang).into());
        }
    });

    // Handle Guardar (Ctrl+S): Si ya tiene un archivo en disco, guarda directamente. Si es borrador temporal, abre Guardar Como para pedir nombre real.
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_save_scratch(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let current_text = ui.get_documentText();
            let file_path_opt = {
                let mut s = state_clone.borrow_mut();
                let ag = s.active_group;
                let active_tab = s.groups[ag].active_tab;
                let new_doc = Rope::from_str(current_text.as_ref());
                if s.groups[ag].tabs[active_tab].document != new_doc {
                    s.groups[ag].tabs[active_tab]
                        .undo_stack
                        .push(new_doc.clone());
                }
                s.groups[ag].tabs[active_tab].document = new_doc;
                s.groups[ag].tabs[active_tab].file_path.clone()
            };

            if let Some(path) = file_path_opt {
                // Caso A: Ya tiene nombre real en disco
                if let Err(e) = std::fs::write(&path, current_text.as_ref() as &str) {
                    ui.set_status_text(format!("Error al guardar: {}", e).into());
                } else {
                    let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                    let mut s = state_clone.borrow_mut();
                    let ag = s.active_group;
                    let active_tab = s.groups[ag].active_tab;
                    s.groups[ag].tabs[active_tab].last_known_mtime = mtime;
                    s.last_save = SystemTime::now();
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Archivo".to_string());
                    ui.set_status_text(format!("Guardado: {}", name).into());
                }
            } else {
                // Caso B: Es un borrador temporal pendiente de guardar con nombre real -> Guardar Como
                let picker = rfd::FileDialog::new()
                    .add_filter("Markdown", &["md", "markdown"])
                    .add_filter("Texto", &["txt"])
                    .add_filter("Todos los archivos", &["*" as &str]);

                if let Some(path) = picker.save_file() {
                    if let Err(e) = std::fs::write(&path, current_text.as_ref() as &str) {
                        ui.set_status_text(format!("Error al guardar: {}", e).into());
                    } else {
                        let title = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Archivo".to_string());
                        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

                        let mut s = state_clone.borrow_mut();
                        let ag = s.active_group;
                        let active_tab = s.groups[ag].active_tab;
                        s.groups[ag].tabs[active_tab].title = title.into();
                        s.groups[ag].tabs[active_tab].is_scratch = false;
                        s.groups[ag].tabs[active_tab].scratch_id = None;
                        s.groups[ag].tabs[active_tab].file_path = Some(path.clone());
                        s.groups[ag].tabs[active_tab].last_known_mtime = mtime;
                        s.last_save = SystemTime::now();
                        ui.set_tab_titles(make_model(s.tab_titles()));
                        let lang = s.detected_language();
                        ui.set_status_text(format!("Guardado como: {} | {}", path.display(), lang).into());
                    }
                }
            }
        }
    });

    // Handle close tab
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_close_tab(move |index| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let index = index as usize;
            let ag = s.active_group;
            if s.groups[ag].tabs.len() > 1 {
                if index < s.groups[ag].tabs.len() {
                    s.groups[ag].tabs.remove(index);
                    if index <= s.groups[ag].active_tab && s.groups[ag].active_tab > 0 {
                        s.groups[ag].active_tab -= 1;
                    }
                }
                if s.groups[ag].active_tab >= s.groups[ag].tabs.len() {
                    s.groups[ag].active_tab = s.groups[ag].tabs.len().saturating_sub(1);
                }
            } else {
                // If only 1 tab left, reset it to an empty persistent draft tab
                let draft_id = s.scratch_manager.next_id_public();
                s.groups[ag].tabs[0] = Tab::new_scratch("Sin título 1", Rope::new(), draft_id);
                s.groups[ag].active_tab = 0;
            }
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            let new_text = s.document_text();
            ui.set_documentText(new_text.clone());
            ui.set_active_tab(s.groups[ag].active_tab as i32);
            let title = s.groups[ag].tabs[s.groups[ag].active_tab].title.clone();
            let lang = s.detected_language();
            ui.set_status_text(format!("{} | {}", title, lang).into());
            sync_preview(&ui, new_text.as_ref(), &lang);
        }
    });




    // Handle toggle search panel
    let ui_weak = ui.as_weak();
    ui.on_toggle_search_panel(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let cur = ui.get_show_search();
            ui.set_show_search(!cur);
        }
    });

    // Handle search query
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_search_text(move |query| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let doc = &s.groups[ag].tabs[at].document;
            let results = search::search_in_rope(doc, query.as_str(), search::SearchOptions::default());
            let count = results.len();
            s.search_results = results;
            s.search_index = 0;
            if count > 0 {
                ui.set_search_status(format!("1 de {}", count).into());
            } else {
                ui.set_search_status("0 coincidencias".into());
            }
        }
    });

    // Handle find next
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_find_next(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let count = s.search_results.len();
            if count > 0 {
                s.search_index = (s.search_index + 1) % count;
                ui.set_search_status(format!("{} de {}", s.search_index + 1, count).into());
            }
        }
    });

    // Handle find previous
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_find_prev(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let count = s.search_results.len();
            if count > 0 {
                s.search_index = if s.search_index == 0 { count - 1 } else { s.search_index - 1 };
                ui.set_search_status(format!("{} de {}", s.search_index + 1, count).into());
            }
        }
    });

    // Handle replace text (single)
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_replace_text(move |replacement| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let count = s.search_results.len();
            if count > 0 && s.search_index < count {
                let single_match = vec![s.search_results[s.search_index].clone()];
                let prev_doc = s.groups[ag].tabs[at].document.clone();
                s.groups[ag].tabs[at].undo_stack.push(prev_doc);
                search::replace_in_rope(&mut s.groups[ag].tabs[at].document, &single_match, replacement.as_str(), false);
                s.dirty = true;
                let new_text = s.groups[ag].tabs[at].document.to_string();
                ui.set_documentText(new_text.clone().into());
                let lang = s.detected_language();
                sync_preview(&ui, &new_text, &lang);
                // Re-run search with current query
                let query = ui.get_search_query();
                s.search_results = search::search_in_rope(&s.groups[ag].tabs[at].document, query.as_str(), search::SearchOptions::default());
                let new_count = s.search_results.len();
                if new_count > 0 {
                    s.search_index = s.search_index.min(new_count - 1);
                    ui.set_search_status(format!("{} de {}", s.search_index + 1, new_count).into());
                } else {
                    ui.set_search_status("0 coincidencias".into());
                }
            }
        }
    });

    // Handle replace all
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_replace_all(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let at = s.groups[ag].active_tab;
            let replacement = ui.get_replace_query();
            let results = s.search_results.clone();
            let count = results.len();
            if count > 0 {
                let prev_doc = s.groups[ag].tabs[at].document.clone();
                s.groups[ag].tabs[at].undo_stack.push(prev_doc);
                search::replace_in_rope(&mut s.groups[ag].tabs[at].document, &results, replacement.as_str(), true);
                s.dirty = true;
                let new_text = s.groups[ag].tabs[at].document.to_string();
                ui.set_documentText(new_text.clone().into());
                let lang = s.detected_language();
                sync_preview(&ui, &new_text, &lang);
                s.search_results.clear();
                s.search_index = 0;
                ui.set_search_status(format!("{} reemplazados", count).into());
            }
        }
    });

    // Handle group switch
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_switch_group(move |index| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            // Save current content before switching groups
            let current_text = ui.get_documentText();
            let ag = s.active_group;
            let old_active = s.groups[ag].active_tab;
            let new_doc = Rope::from_str(current_text.as_ref());
            if s.groups[ag].tabs[old_active].document != new_doc {
                s.groups[ag].tabs[old_active]
                    .undo_stack
                    .push(new_doc.clone());
            }
            s.groups[ag].tabs[old_active].document = new_doc;

            // Switch to the new group
            s.active_group = index as usize;

            // If the group doesn't exist yet, create it as persistent draft
            while s.groups.len() <= s.active_group {
                let draft_id = s.scratch_manager.next_id_public();
                s.groups.push(Group {
                    tabs: vec![Tab::new_scratch("Sin título 1", Rope::new(), draft_id)],
                    active_tab: 0,
                });
            }

            let new_ag = s.active_group;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[new_ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.groups[new_ag].active_tab as i32);
            ui.set_active_group(s.active_group as i32);
            let lang = s.detected_language();
            ui.set_status_text(format!("Group {} | {}", s.active_group + 1, lang).into());
        }
    });

    // Handle group count change
    let ui_weak = ui.as_weak();
    ui.on_set_group_count(move |count| {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_group_count(count);
        }
    });

    // Handle markdown view toggle
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_toggle_markdown_view(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let was_enabled = ui.get_markdown_view_enabled();
            let new_enabled = !was_enabled;

            if new_enabled {
                let current_text = ui.get_documentText();
                let lang = {
                    let s = state_clone.borrow();
                    s.detected_language()
                };
                sync_preview(&ui, current_text.as_ref(), &lang);
                ui.set_show_second_column(true);
            } else {
                ui.set_show_second_column(false);
            }

            ui.set_markdown_view_enabled(new_enabled);
        }
    });

    // Handle markdown split toggle
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_toggle_markdown_split(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let was_split = ui.get_markdown_split_enabled();
            let new_split = !was_split;
            if new_split {
                let current_text = ui.get_documentText();
                let lang = {
                    let s = state_clone.borrow();
                    s.detected_language()
                };
                sync_preview(&ui, current_text.as_ref(), &lang);
                ui.set_markdown_view_enabled(true);
                ui.set_show_second_column(true);
            } else {
                ui.set_show_second_column(false);
            }
            ui.set_markdown_split_enabled(new_split);
        }
    });

    // Handle syntax view toggle
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_toggle_syntax_view(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let was_syntax = ui.get_syntax_view_enabled();
            let new_syntax = !was_syntax;
            ui.set_syntax_view_enabled(new_syntax);

            let current_text = ui.get_documentText();
            let lang = {
                let s = state_clone.borrow();
                s.detected_language()
            };

            if new_syntax {
                ui.set_markdown_view_enabled(true);
                ui.set_show_second_column(true);
                sync_preview(&ui, current_text.as_ref(), &lang);
                ui.set_status_text(format!("Vista de sintaxis activada [{}]", lang).into());
            } else {
                sync_preview(&ui, current_text.as_ref(), &lang);
                ui.set_status_text(format!("Vista previa formateada activada [{}]", lang).into());
            }
        }
    });

    // Handle browser preview
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_open_browser_preview(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let current_text = ui.get_documentText();
            let lang = {
                let s = state_clone.borrow();
                s.detected_language()
            };
            let html_content = markdown::generate_standalone_html(current_text.as_ref(), &lang);
            let temp_path = std::env::temp_dir().join("lightmark_preview.html");
            if std::fs::write(&temp_path, html_content).is_ok() {
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", "", &temp_path.to_string_lossy()])
                    .spawn();
                ui.set_status_text(format!("Vista previa abierta en el navegador: {}", temp_path.display()).into());
            } else {
                ui.set_status_text("Error al generar vista previa para el navegador".into());
            }
        }
    });

    // Handle undo
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_undo(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let active_tab = s.groups[ag].active_tab;
            if s.groups[ag].tabs[active_tab].undo_stack.can_undo()
                && let Some(doc) = s.groups[ag].tabs[active_tab].undo_stack.undo()
            {
                s.groups[ag].tabs[active_tab].document = doc.clone();
                ui.set_documentText(doc.to_string().into());
            }
        }
    });

    // Handle redo
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_redo(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let active_tab = s.groups[ag].active_tab;
            if s.groups[ag].tabs[active_tab].undo_stack.can_redo()
                && let Some(doc) = s.groups[ag].tabs[active_tab].undo_stack.redo()
            {
                s.groups[ag].tabs[active_tab].document = doc.clone();
                ui.set_documentText(doc.to_string().into());
            }
        }
    });

    // Handle open file
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_open_file(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let picker = rfd::FileDialog::new()
                .add_filter("markdown", &["md", "markdown"])
                .add_filter("text", &["txt"])
                .add_filter("all files", &["*" as &str]);

            if let Some(path) = picker.pick_file() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let rope = Rope::from_str(&content);
                        let title = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Untitled".to_string());

                        let mtime = std::fs::metadata(&path)
                            .ok()
                            .and_then(|m| m.modified().ok());

                        let mut s = state_clone.borrow_mut();
                        let ag = s.active_group;
                        let mut tab = Tab::new_regular(&title, rope);
                        tab.file_path = Some(path.clone());
                        tab.last_known_mtime = mtime;
                        s.groups[ag].tabs.push(tab);
                        s.groups[ag].active_tab = s.groups[ag].tabs.len() - 1;
                        ui.set_tab_titles(make_model(s.tab_titles()));
                        ui.set_tab_count(s.groups[ag].tabs.len() as i32);
                        ui.set_documentText(s.document_text());
                        ui.set_active_tab((s.groups[ag].tabs.len() - 1) as i32);
                        ui.set_status_text(format!("Abierto: {}", path.display()).into());
                    }
                    Err(e) => {
                        ui.set_status_text(format!("Error al abrir archivo: {}", e).into());
                    }
                }
            }
        }
    });

    // Handle save session
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_save_session(move || {
        if ui_weak.upgrade().is_some() {
            let s = state_clone.borrow();
            let session = s.to_session();
            if let Err(e) = s.session_manager.save_session(&session, "last-session") {
                eprintln!("Failed to save session: {}", e);
            } else {
                // Also save as auto-restore
                if let Err(e) = s.session_manager.save_auto_session(&session) {
                    eprintln!("Failed to save auto session: {}", e);
                }
            }
        }
    });

    // Handle restore session
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_restore_session(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            if let Ok(session) = s.session_manager.load_session("last-session") {
                s.restore_from_session(&session);
            }
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.active_group_tabs().len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.groups[s.active_group].active_tab as i32);
            ui.set_group_count(s.groups.len() as i32);
            ui.set_active_group(s.active_group as i32);
        }
    });

    // Handle park tab
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_park_tab(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let active_tab = s.groups[ag].active_tab;

            if s.groups[ag].tabs[active_tab].is_scratch {
                let scratch_id = s.groups[ag].tabs[active_tab].scratch_id.clone();
                let doc_clone = s.groups[ag].tabs[active_tab].document.clone();

                if let Some(ref sid) = scratch_id {
                    if let Some(mut scratch_doc) = s
                        .scratch_manager
                        .create_scratch_doc_for_save(sid.clone(), doc_clone)
                    {
                        if s.scratch_manager.park_document(&mut scratch_doc).is_err() {
                            eprintln!("Failed to park document");
                        }
                        s.groups[ag].tabs[active_tab].title = scratch_doc.effective_title().into();
                    }

                    // Remove the parked tab
                    s.groups[ag].tabs.remove(active_tab);
                    if s.groups[ag].tabs.is_empty() {
                        s.groups[ag].tabs.push(Tab::new_regular("Tab 1", Rope::new()));
                        s.groups[ag].active_tab = 0;
                    } else if s.groups[ag].active_tab >= s.groups[ag].tabs.len() {
                        s.groups[ag].active_tab = s.groups[ag].tabs.len().saturating_sub(1);
                    }
                    ui.set_tab_titles(make_model(s.tab_titles()));
                    ui.set_tab_count(s.groups[ag].tabs.len() as i32);
                    ui.set_documentText(s.document_text());
                    ui.set_active_tab(s.groups[ag].active_tab as i32);
                }
            }
        }
    });

    // Handle save as
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_save_as(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let picker = rfd::FileDialog::new()
                .add_filter("markdown", &["md", "markdown"])
                .add_filter("text", &["txt"])
                .add_filter("all files", &["*" as &str]);

            let current_text = ui.get_documentText();
            let path = picker.save_file();

            if let Some(path) = path {
                if std::fs::write(&path, current_text.as_ref() as &str).is_err() {
                    eprintln!("Failed to save file");
                } else {
                    let title = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string());

                    let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

                    let mut s = state_clone.borrow_mut();
                    let ag = s.active_group;
                    let active_tab = s.groups[ag].active_tab;
                    s.groups[ag].tabs[active_tab].title = title.into();
                    s.groups[ag].tabs[active_tab].is_scratch = false;
                    s.groups[ag].tabs[active_tab].scratch_id = None;
                    s.groups[ag].tabs[active_tab].file_path = Some(path);
                    s.groups[ag].tabs[active_tab].last_known_mtime = mtime;
                    ui.set_tab_titles(make_model(s.tab_titles()));
                    let lang = s.detected_language();
                    ui.set_status_text(format!("Guardado como: {} | {}", s.groups[ag].tabs[active_tab].title, lang).into());
                }
            }
        }
    });

    // Handle format document
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_format_document(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let s = state_clone.borrow();
            let lang = s.detected_language();
            let current_text = ui.get_documentText();

            let fmt_lang = format::FormatLanguage::from_extension(&lang);
            if let Some(fmt_lang) = fmt_lang {
                match format::format_text(current_text.as_ref(), fmt_lang) {
                    Ok(formatted) => {
                        drop(s);
                        ui.set_documentText(formatted.into());
                    }
                    Err(e) => {
                        eprintln!("Format error: {:?}", e);
                    }
                }
            }
        }
    });

    // Handle save all
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_save_all(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let current_text = ui.get_documentText();
            let ag = s.active_group;
            let active_tab = s.groups[ag].active_tab;
            s.groups[ag].tabs[active_tab].document = {
                let new_doc = Rope::from_str(current_text.as_ref());
                if s.groups[ag].tabs[active_tab].document != new_doc {
                    s.groups[ag].tabs[active_tab]
                        .undo_stack
                        .push(new_doc.clone());
                }
                new_doc
            };

            let base_dir = s.scratch_manager.base_dir().to_path_buf();

            // Collect scratch tab info to avoid borrow conflicts
            let scratch_tabs: Vec<(String, Rope)> = s
                .groups
                .iter()
                .flat_map(|g| &g.tabs)
                .filter_map(|t| {
                    t.scratch_id
                        .as_ref()
                        .map(|id| (id.clone(), t.document.clone()))
                })
                .collect();

            // Save all scratch tabs and collect updated titles
            let mut updated_titles: Vec<(String, String)> = Vec::new();
            for (sid, doc_clone) in scratch_tabs {
                let mut index = scratch::ScratchIndex::load(&base_dir);
                if let Some(mut scratch_doc) = s
                    .scratch_manager
                    .create_scratch_doc_for_save(sid.clone(), doc_clone)
                {
                    if s.scratch_manager
                        .save_scratch(&mut scratch_doc, &mut index)
                        .is_err()
                    {
                        eprintln!("Failed to autosave scratch document: {}", sid);
                    }
                    updated_titles.push((sid, scratch_doc.effective_title()));
                }
            }

            // Update tab titles
            for (sid, title) in updated_titles {
                for group in &mut s.groups {
                    for tab in &mut group.tabs {
                        if tab
                            .scratch_id
                            .as_ref()
                            .map(|id| id == &sid)
                            .unwrap_or(false)
                        {
                            tab.title = title.clone().into();
                        }
                    }
                }
            }

            // Also save regular files with file_path
            for group in &mut s.groups {
                for tab in &mut group.tabs {
                    if let Some(ref path) = tab.file_path {
                        if let Err(e) = std::fs::write(path, tab.document.to_string()) {
                            eprintln!("Failed to save regular file {}: {}", path.display(), e);
                        } else if let Ok(meta) = std::fs::metadata(path) {
                            tab.last_known_mtime = meta.modified().ok();
                        }
                    }
                }
            }

            // Also save session
            let session = s.to_session();
            if let Err(e) = s.session_manager.save_auto_session(&session) {
                eprintln!("Failed to save auto session: {}", e);
            }
            ui.set_status_text("Todos los documentos guardados".into());
        }
    });

    // Timer for periodic auto-save and keyboard shortcuts
    let ui_weak_for_timer = ui.as_weak();
    let state_for_timer = state.clone();
    let mut shortcut_checker = keyboard::ShortcutChecker::new();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            if let Some(ui) = ui_weak_for_timer.upgrade() {
                let actions = shortcut_checker.check();
                if !actions.is_empty() {
                    let mut s = state_for_timer.borrow_mut();
                    let current_text = ui.get_documentText();
                    let ag = s.active_group;
                    let active_tab = s.groups[ag].active_tab;
                    let new_doc = Rope::from_str(current_text.as_ref());
                    if s.groups[ag].tabs[active_tab].document != new_doc {
                        s.groups[ag].tabs[active_tab]
                            .undo_stack
                            .push(new_doc.clone());
                    }
                    s.groups[ag].tabs[active_tab].document = new_doc;

                    for action in &actions {
                        match action.as_str() {
                            "new-scratch" => {
                                let scratch_id = s.scratch_manager.next_id_public();
                                let scratch_num = s.groups.iter().flat_map(|g| &g.tabs).filter(|t| t.is_scratch).count() + 1;
                                let title = format!("Scratch {}", scratch_num);
                                let new_index = s.groups[ag].tabs.len();
                                s.groups[ag]
                                    .tabs
                                    .push(Tab::new_scratch(&title, Rope::new(), scratch_id));
                                s.groups[ag].active_tab = new_index;
                                ui.set_tab_titles(make_model(s.tab_titles()));
                                ui.set_tab_count(s.groups[ag].tabs.len() as i32);
                                ui.set_documentText(s.document_text());
                                ui.set_active_tab(new_index as i32);
                            }
                            "save-all" => {
                                let base_dir = s.scratch_manager.base_dir().to_path_buf();
                                let scratch_tabs: Vec<(String, Rope)> = s
                                    .groups
                                    .iter()
                                    .flat_map(|g| &g.tabs)
                                    .filter_map(|t| {
                                        t.scratch_id
                                            .as_ref()
                                            .map(|id| (id.clone(), t.document.clone()))
                                    })
                                    .collect();
                                for (sid, doc_clone) in scratch_tabs {
                                    let mut index = scratch::ScratchIndex::load(&base_dir);
                                    if let Some(mut scratch_doc) = s
                                        .scratch_manager
                                        .create_scratch_doc_for_save(sid, doc_clone)
                                    {
                                        let _ = s
                                            .scratch_manager
                                            .save_scratch(&mut scratch_doc, &mut index);
                                    }
                                }
                                let session = s.to_session();
                                let _ = s.session_manager.save_auto_session(&session);
                            }
                            "toggle-markdown-view" => {
                                let was_enabled = ui.get_markdown_view_enabled();
                                let new_enabled = !was_enabled;
                                if new_enabled {
                                    let lang = s.detected_language();
                                    sync_preview(&ui, current_text.as_ref(), &lang);
                                }
                                ui.set_markdown_view_enabled(new_enabled);
                            }
                            "undo" => {
                                let ag = s.active_group;
                                let active_tab = s.groups[ag].active_tab;
                                if s.groups[ag].tabs[active_tab].undo_stack.can_undo()
                                    && let Some(doc) =
                                        s.groups[ag].tabs[active_tab].undo_stack.undo()
                                {
                                    s.groups[ag].tabs[active_tab].document = doc.clone();
                                    ui.set_documentText(doc.to_string().into());
                                }
                            }
                            "redo" => {
                                let ag = s.active_group;
                                let active_tab = s.groups[ag].active_tab;
                                if s.groups[ag].tabs[active_tab].undo_stack.can_redo()
                                    && let Some(doc) =
                                        s.groups[ag].tabs[active_tab].undo_stack.redo()
                                {
                                    s.groups[ag].tabs[active_tab].document = doc.clone();
                                    ui.set_documentText(doc.to_string().into());
                                }
                            }
                            "set-group-count-1" => {
                                ui.set_group_count(1);
                            }
                            "set-group-count-2" => {
                                ui.set_group_count(2);
                            }
                            "set-group-count-3" => {
                                ui.set_group_count(3);
                            }
                            "set-group-count-4" => {
                                ui.set_group_count(4);
                            }
                            "duplicate-line" => {
                                let prev_doc = s.groups[ag].tabs[active_tab].document.clone();
                                s.groups[ag].tabs[active_tab].undo_stack.push(prev_doc);
                                let updated = duplicate_last_line(&current_text);
                                s.groups[ag].tabs[active_tab].document = Rope::from_str(&updated);
                                s.dirty = true;
                                ui.set_documentText(updated.into());
                                ui.set_status_text("Línea duplicada (Ctrl+Shift+D)".into());
                            }
                            "delete-line" => {
                                let prev_doc = s.groups[ag].tabs[active_tab].document.clone();
                                s.groups[ag].tabs[active_tab].undo_stack.push(prev_doc);
                                let updated = delete_last_line(&current_text);
                                s.groups[ag].tabs[active_tab].document = Rope::from_str(&updated);
                                s.dirty = true;
                                ui.set_documentText(updated.into());
                                ui.set_status_text("Línea eliminada (Ctrl+Shift+K)".into());
                            }
                            "join-lines" => {
                                let prev_doc = s.groups[ag].tabs[active_tab].document.clone();
                                s.groups[ag].tabs[active_tab].undo_stack.push(prev_doc);
                                let updated = join_lines(&current_text);
                                s.groups[ag].tabs[active_tab].document = Rope::from_str(&updated);
                                s.dirty = true;
                                ui.set_documentText(updated.into());
                                ui.set_status_text("Líneas unidas (Ctrl+J)".into());
                            }
                            _ => {}
                        }
                    }
                }

                // Auto-save logic with debouncing (reduced disk I/O)
                if actions.is_empty() {
                    let current_text = ui.get_documentText();
                    let mut s = state_for_timer.borrow_mut();
                    let ag = s.active_group;
                    let active_tab = s.groups[ag].active_tab;

                    let new_doc = Rope::from_str(current_text.as_ref());
                    let has_changed = s.groups[ag].tabs[active_tab].document != new_doc;
                    if has_changed {
                        let prev_doc = s.groups[ag].tabs[active_tab].document.clone();
                        if s.groups[ag].tabs[active_tab].last_undo_push.elapsed() >= std::time::Duration::from_millis(500) {
                            s.groups[ag].tabs[active_tab].undo_stack.push(prev_doc);
                            s.groups[ag].tabs[active_tab].last_undo_push = std::time::Instant::now();
                        }
                        s.groups[ag].tabs[active_tab].document = new_doc;
                        s.dirty = true;
                        s.last_saved_instant = std::time::Instant::now();

                        let lang = s.detected_language();
                        let char_count = current_text.chars().count();
                        let word_count = current_text.split_whitespace().count();
                        let line_count = current_text.lines().count().max(1);
                        ui.set_detected_lang(lang.clone().into());
                        ui.set_doc_stats(format!("Líneas: {} | Palabras: {} | Caracteres: {}", line_count, word_count, char_count).into());

                        // Keep preview or syntax view synchronized
                        if ui.get_markdown_split_enabled() || ui.get_markdown_view_enabled() || ui.get_syntax_view_enabled() || ui.get_show_second_column() {
                            sync_preview(&ui, current_text.as_ref(), &lang);
                        }

                        // Update line numbers only when changed
                        ui.set_line_numbers(generate_line_numbers(current_text.as_ref()).into());
                    }

                    // Save to disk only if dirty and configured delay has elapsed
                    let delay_ms = ui.get_autosave_delay_ms().max(200) as u64;
                    if s.dirty && s.last_saved_instant.elapsed() >= std::time::Duration::from_millis(delay_ms) {
                        s.dirty = false;
                        let base_dir = s.scratch_manager.base_dir().to_path_buf();
                        let scratch_tabs: Vec<(String, Rope)> = s
                            .groups
                            .iter()
                            .flat_map(|g| &g.tabs)
                            .filter_map(|t| {
                                t.scratch_id
                                    .as_ref()
                                    .map(|id| (id.clone(), t.document.clone()))
                            })
                            .collect();

                        for (sid, doc_clone) in scratch_tabs {
                            let mut index = scratch::ScratchIndex::load(&base_dir);
                            if let Some(mut scratch_doc) = s
                                .scratch_manager
                                .create_scratch_doc_for_save(sid, doc_clone)
                            {
                                let _ = s.scratch_manager.save_scratch(&mut scratch_doc, &mut index);
                            }
                        }

                        // Also autosave any modified regular files
                        for group in &mut s.groups {
                            for tab in &mut group.tabs {
                                if let Some(ref path) = tab.file_path {
                                    if let Ok(content_disk) = std::fs::read_to_string(path) {
                                        let current_str = tab.document.to_string();
                                        if content_disk != current_str {
                                            if let Ok(_) = std::fs::write(path, &current_str) {
                                                if let Ok(meta) = std::fs::metadata(path) {
                                                    tab.last_known_mtime = meta.modified().ok();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let session = s.to_session();
                        let _ = s.session_manager.save_auto_session(&session);
                    }

                    // Check for external file changes
                    let files_to_check: Vec<(std::path::PathBuf, SystemTime, bool)> = s
                        .groups
                        .iter()
                        .flat_map(|g| g.tabs.iter())
                        .filter_map(|t| {
                            t.file_path
                                .as_ref()
                                .zip(t.last_known_mtime)
                                .map(|(p, m)| (p.clone(), m, false))
                        })
                        .collect();

                    for (path, last_mtime, _) in files_to_check {
                        if let Ok(metadata) = std::fs::metadata(&path)
                            && let Ok(current_mtime) = metadata.modified()
                            && current_mtime > last_mtime
                            && let Ok(content) = std::fs::read_to_string(&path)
                        {
                            let new_doc = Rope::from_str(&content);
                            for group in &mut s.groups {
                                let is_active_tab =
                                    group.tabs[group.active_tab].file_path.as_ref() == Some(&path);
                                for tab in &mut group.tabs {
                                    if tab.file_path.as_ref() == Some(&path) {
                                        if tab.document != new_doc {
                                            tab.undo_stack.push(tab.document.clone());
                                        }
                                        tab.document = new_doc.clone();
                                        tab.last_known_mtime = Some(current_mtime);
                                    }
                                }
                                if is_active_tab {
                                    ui.set_documentText(new_doc.to_string().into());
                                    ui.set_status_text(
                                        format!("Recargado: {}", path.display()).into(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        },
    );

    // Handle export archive
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_export_archive(move || {
        if ui_weak.upgrade().is_some() {
            let s = state_clone.borrow();
            let picker = rfd::FileDialog::new()
                .add_filter("folder", &["folder" as &str])
                .set_title("Select archive location");

            let parked_docs: Vec<scratch::ScratchDocument> =
                if let Ok(docs) = s.scratch_manager.list_parked_documents() {
                    docs.into_iter()
                        .filter_map(|meta| s.scratch_manager.load_scratch(&meta).ok())
                        .collect()
                } else {
                    Vec::new()
                };

            if !parked_docs.is_empty()
                && let Some(dir) = picker.pick_folder()
            {
                let now = SystemTime::now();
                let timestamp = now
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let dir_name = format!("archive-{}", timestamp);
                let full_path = dir.join(&dir_name);

                if let Err(e) = scratch::export_to_folder(&parked_docs, &full_path) {
                    eprintln!("Failed to export archive: {}", e);
                }
            }
        }
    });

    // Handle toggle explorer
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_toggle_explorer(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let was_showing = ui.get_show_explorer();
            let new_showing = !was_showing;
            ui.set_show_explorer(new_showing);

            if new_showing {
                let s = state_clone.borrow();
                let browse_dir = s.workspace_root.as_deref().unwrap_or(std::path::Path::new("."));
                if let Ok(entries) = std::fs::read_dir(browse_dir) {
                    let mut files: Vec<SharedString> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let path = e.path();
                            let name = e.file_name().to_string_lossy().to_string();
                            if path.is_dir() {
                                format!("[DIR] {}", name).into()
                            } else {
                                name.into()
                            }
                        })
                        .collect();
                    files.sort();
                    ui.set_explorer_files(make_model(files));
                }
            }
        }
    });

    // Handle open folder
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_open_folder(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let picker = rfd::FileDialog::new()
                .add_filter("folder", &["folder" as &str])
                .set_title("Seleccionar carpeta de espacio de trabajo");

            if let Some(dir) = picker.pick_folder() {
                {
                    let mut s = state_clone.borrow_mut();
                    s.workspace_root = Some(dir.clone());
                }
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    let mut files: Vec<SharedString> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let path = e.path();
                            let name = e.file_name().to_string_lossy().to_string();
                            if path.is_dir() {
                                format!("[DIR] {}", name).into()
                            } else {
                                name.into()
                            }
                        })
                        .collect();
                    files.sort();
                    ui.set_explorer_files(make_model(files));
                    ui.set_show_explorer(true);
                }
            }
        }
    });

    // Handle open file from explorer
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_open_file_from_explorer(move |file_name: SharedString| {
        if let Some(ui) = ui_weak.upgrade() {
            let file_str = file_name.as_str();
            if file_str.starts_with("[DIR] ") {
                // Ignore directory click or could open directory
                return;
            }

            let mut s = state_clone.borrow_mut();
            let path = if let Some(ref root) = s.workspace_root {
                root.join(file_str)
            } else {
                std::path::PathBuf::from(file_str)
            };

            if path.is_file()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let rope = Rope::from_str(&content);
                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());

                let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

                let ag = s.active_group;
                let mut tab = Tab::new_regular(&title, rope);
                tab.file_path = Some(path);
                tab.last_known_mtime = mtime;
                s.groups[ag].tabs.push(tab);
                s.groups[ag].active_tab = s.groups[ag].tabs.len() - 1;
                ui.set_tab_titles(make_model(s.tab_titles()));
                ui.set_tab_count(s.groups[ag].tabs.len() as i32);
                ui.set_documentText(s.document_text());
                ui.set_active_tab((s.groups[ag].tabs.len() - 1) as i32);
                let lang = s.detected_language();
                ui.set_status_text(format!("Abierto: {} | {}", title, lang).into());
            }
        }
    });

    // Handle toggle fullscreen
    let ui_weak = ui.as_weak();
    ui.on_toggle_fullscreen(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let cur = ui.get_is_fullscreen();
            ui.set_is_fullscreen(!cur);
            ui.set_status_text(if !cur {
                "Pantalla completa activada (F11 o clic en ⛶ para salir)".into()
            } else {
                "Pantalla completa desactivada".into()
            });
        }
    });

    // Handle open app data folder
    ui.on_open_app_folder(move || {
        let app_data = scratch::get_app_data_dir();
        std::fs::create_dir_all(&app_data).ok();
        std::process::Command::new("explorer.exe")
            .arg(app_data.to_string_lossy().to_string())
            .spawn()
            .ok();
    });

    ui.run()
}

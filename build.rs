fn main() {
    // U5: los widgets de std-widgets.slint (LineEdit, Button, ComboBox, SpinBox, Switch,
    // ScrollView...) siguen el tema "fluent" claro por defecto; la UI de LightMark está
    // diseñada en oscuro (Theme en ui/theme.slint), así que se fija explícitamente el
    // estilo "fluent-dark" del compilador de Slint para que esos widgets nativos combinen
    // con el resto (antes de este cambio se veían con fondo claro sobre la app oscura).
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-dark".into()),
    )
    .unwrap();

    // Manifiesto de aplicación: sin él, Windows resuelve comctl32.dll a la versión v5 del
    // sistema, que carece de TaskDialogIndirect (usado por los diálogos de 3 botones de rfd
    // al cerrar una ventana con cambios sin guardar) -> "Entry Point Not Found" en tiempo de
    // ejecución. Pedir explícitamente comctl32 v6 (common controls) y declarar conciencia de
    // DPI por monitor para que la UI no se vea borrosa en pantallas de alta densidad.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        use embed_manifest::{embed_manifest, manifest::{ActiveCodePage, DpiAwareness}, new_manifest};
        embed_manifest(
            new_manifest("LightMark.App")
                .active_code_page(ActiveCodePage::Utf8)
                .dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("no se pudo incrustar el manifiesto de la aplicación");
    }
    println!("cargo:rerun-if-changed=build.rs");
}

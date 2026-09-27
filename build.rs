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
}

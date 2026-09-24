use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use ropey::Rope;

slint::slint! {
    import { TextEdit } from "std-widgets.slint";

    export component Editor inherits Window {
        width: 800px;
        height: 600px;

        in-out property <string> documentText;

        TextEdit {
            text <=> root.documentText;
            // We'll try without wrap and editable for now
        }
    }
}

struct EditorState {
    document: Rope,
}

impl EditorState {
    fn new() -> Self {
        Self {
            document: Rope::from("Hello, world!\n"),
        }
    }

    fn document_text(&self) -> SharedString {
        self.document.to_string().into()
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let state = EditorState::new();
    let ui = Editor::new()?;

    // Set initial text
    ui.set_documentText(state.document_text());

    ui.run()
}

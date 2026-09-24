use ropey::Rope;
use slint::{ComponentHandle, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

slint::slint! {
    import { Button, HorizontalBox, TextEdit, VerticalBox } from "std-widgets.slint";

    export component App inherits Window {
        width: 800px;
        height: 600px;

        callback tab-clicked(int);
        callback close-tab(int);
        callback new-tab();

        in-out property <[string]> tab_titles;
        in-out property <string> documentText;
        in-out property <int> active_tab;
        in-out property <int> tab_count;

        VerticalBox {
            HorizontalBox {
                for i in root.tab_count: Button {
                    text: root.tab_titles[i];
                    width: 120px;
                    height: 24px;
                    clicked => {
                        root.tab-clicked(i);
                    }
                }
                Button {
                    text: "+";
                    width: 24px;
                    height: 24px;
                    clicked => {
                        root.new-tab();
                    }
                }
            }

            TextEdit {
                text <=> root.documentText;
            }
        }
    }
}

struct Tab {
    title: String,
    document: Rope,
}

struct EditorState {
    tabs: Vec<Tab>,
    active_tab: usize,
}

impl EditorState {
    fn new() -> Self {
        let tabs = vec![Tab {
            title: "Tab 1".to_string(),
            document: Rope::from("Hello, world!\n"),
        }];
        Self {
            tabs,
            active_tab: 0,
        }
    }

    fn tab_titles(&self) -> Vec<SharedString> {
        self.tabs
            .iter()
            .map(|tab| tab.title.clone().into())
            .collect()
    }

    fn document_text(&self) -> SharedString {
        self.tabs[self.active_tab].document.to_string().into()
    }
}

// Helper to convert Vec<SharedString> to a ModelRc<SharedString>
fn make_model(titles: Vec<SharedString>) -> slint::ModelRc<SharedString> {
    Rc::new(VecModel::from(titles)).into()
}

fn main() -> Result<(), slint::PlatformError> {
    let state = Rc::new(RefCell::new(EditorState::new()));
    let ui = App::new()?;

    ui.set_tab_titles(make_model(state.borrow().tab_titles()));
    ui.set_tab_count(state.borrow().tabs.len() as i32);
    ui.set_documentText(state.borrow().document_text());
    ui.set_active_tab(0);

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_tab_clicked(move |index| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            // Save current content back to the previous tab before switching
            let current_text = ui.get_documentText();
            let old_active = s.active_tab;
            s.tabs[old_active].document = Rope::from_str(current_text.as_ref());
            // Switch to the new tab
            s.active_tab = index as usize;
            // Load the new tab's content
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.active_tab as i32);
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_close_tab(move |index| {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let index = index as usize;
            if s.tabs.len() > 1 {
                s.tabs.remove(index);
                if s.active_tab >= s.tabs.len() {
                    s.active_tab = s.tabs.len() - 1;
                }
                ui.set_tab_titles(make_model(s.tab_titles()));
                ui.set_tab_count(s.tabs.len() as i32);
                ui.set_documentText(s.document_text());
            }
        }
    });

    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_new_tab(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let new_index = s.tabs.len();
            s.tabs.push(Tab {
                title: format!("Tab {}", new_index + 1),
                document: Rope::new(),
            });
            s.active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.tabs.len() as i32);
            ui.set_documentText(s.document_text());
        }
    });

    ui.run()
}

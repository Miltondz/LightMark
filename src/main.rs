use ropey::Rope;
use slint::{ComponentHandle, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

slint::slint! {
    import { Button, HorizontalBox, TextEdit, VerticalBox } from "std-widgets.slint";

    export component App inherits Window {
        width: 1000px;
        height: 700px;

        callback tab-clicked(int);
        callback close-tab(int);
        callback new-tab();
        callback switch-group(int);
        callback set-group-count(int);

        in-out property <[string]> tab_titles;
        in-out property <string> documentText;
        in-out property <int> active_tab;
        in-out property <int> tab_count;
        in-out property <int> group_count;
        in-out property <int> active_group;

        VerticalBox {
            // Group selector row
            HorizontalBox {
                for i in root.group_count: Button {
                    text: "G" + i;
                    width: 32px;
                    height: 24px;
                    clicked => {
                        root.switch-group(i);
                    }
                }
                Button {
                    text: "1G";
                    width: 40px;
                    height: 24px;
                    clicked => {
                        root.set-group-count(1);
                    }
                }
                Button {
                    text: "2G";
                    width: 40px;
                    height: 24px;
                    clicked => {
                        root.set-group-count(2);
                    }
                }
                Button {
                    text: "3G";
                    width: 40px;
                    height: 24px;
                    clicked => {
                        root.set-group-count(3);
                    }
                }
                Button {
                    text: "4G";
                    width: 40px;
                    height: 24px;
                    clicked => {
                        root.set-group-count(4);
                    }
                }
            }

            // Tab bar for active group
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

            // Editor area
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

struct Group {
    tabs: Vec<Tab>,
    active_tab: usize,
}

struct EditorState {
    groups: Vec<Group>,
    active_group: usize,
}

impl EditorState {
    fn new() -> Self {
        let tabs = vec![Tab {
            title: "Tab 1".to_string(),
            document: Rope::from("Hello, world!\n"),
        }];
        let groups = vec![Group {
            tabs,
            active_tab: 0,
        }];
        Self {
            groups,
            active_group: 0,
        }
    }

    fn active_group_tabs(&self) -> &Vec<Tab> {
        &self.groups[self.active_group].tabs
    }

    fn tab_titles(&self) -> Vec<SharedString> {
        self.active_group_tabs()
            .iter()
            .map(|tab| tab.title.clone().into())
            .collect()
    }

    fn document_text(&self) -> SharedString {
        let active_tab = self.groups[self.active_group].active_tab;
        self.active_group_tabs()[active_tab]
            .document
            .to_string()
            .into()
    }
}

fn make_model(titles: Vec<SharedString>) -> slint::ModelRc<SharedString> {
    Rc::new(VecModel::from(titles)).into()
}

fn main() -> Result<(), slint::PlatformError> {
    let state = Rc::new(RefCell::new(EditorState::new()));
    let ui = App::new()?;

    // Initial state setup
    ui.set_tab_titles(make_model(state.borrow().tab_titles()));
    ui.set_tab_count(state.borrow().active_group_tabs().len() as i32);
    ui.set_documentText(state.borrow().document_text());
    ui.set_active_tab(0);
    ui.set_group_count(1);
    ui.set_active_group(0);

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
            s.groups[ag].tabs[old_active].document = Rope::from_str(current_text.as_ref());
            // Switch to the new tab
            s.groups[ag].active_tab = index as usize;
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.groups[ag].active_tab as i32);
        }
    });

    // Handle new tab
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_new_tab(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let new_index = s.groups[ag].tabs.len();
            s.groups[ag].tabs.push(Tab {
                title: format!("Tab {}", new_index + 1),
                document: Rope::new(),
            });
            s.groups[ag].active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(new_index as i32);
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
                s.groups[ag].tabs.remove(index);
                if s.groups[ag].active_tab >= s.groups[ag].tabs.len() {
                    s.groups[ag].active_tab = s.groups[ag].tabs.len() - 1;
                }
                ui.set_tab_titles(make_model(s.tab_titles()));
                ui.set_tab_count(s.groups[ag].tabs.len() as i32);
                ui.set_documentText(s.document_text());
                ui.set_active_tab(s.groups[ag].active_tab as i32);
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
            s.groups[ag].tabs[old_active].document = Rope::from_str(current_text.as_ref());

            // Switch to the new group
            s.active_group = index as usize;

            // If the group doesn't exist yet, create it
            while s.groups.len() <= s.active_group {
                s.groups.push(Group {
                    tabs: vec![Tab {
                        title: "Tab 1".to_string(),
                        document: Rope::new(),
                    }],
                    active_tab: 0,
                });
            }

            let new_ag = s.active_group;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[new_ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.groups[new_ag].active_tab as i32);
            ui.set_active_group(s.active_group as i32);
        }
    });

    // Handle group count change
    let ui_weak = ui.as_weak();
    ui.on_set_group_count(move |count| {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_group_count(count);
        }
    });

    ui.run()
}

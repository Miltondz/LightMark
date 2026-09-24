use ropey::Rope;
use slint::{ComponentHandle, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

mod markdown;
mod scratch;
use scratch::ScratchManager;

slint::slint! {
    import { Button, HorizontalBox, TextEdit, VerticalBox } from "std-widgets.slint";

    export component App inherits Window {
        width: 1000px;
        height: 700px;

        callback tab-clicked(int);
        callback close-tab(int);
        callback new-tab();
        callback new-scratch();
        callback save-scratch();
        callback toggle-markdown-view();
        callback switch-group(int);
        callback set-group-count(int);

        in-out property <[string]> tab_titles;
        in-out property <string> documentText;
        in-out property <string> markdownViewText;
        in-out property <int> active_tab;
        in-out property <int> tab_count;
        in-out property <int> group_count;
        in-out property <int> active_group;
        in-out property <bool> markdown_view_enabled;

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
                    text: "Scratch";
                    width: 60px;
                    height: 24px;
                    clicked => {
                        root.new-scratch();
                    }
                }
                Button {
                    text: "Save";
                    width: 60px;
                    height: 24px;
                    clicked => {
                        root.save-scratch();
                    }
                }
                Button {
                    text: "View";
                    width: 60px;
                    height: 24px;
                    clicked => {
                        root.toggle-markdown-view();
                    }
                }
            }

            // Editor area
            // Editor area - two TextEdit elements, one visible at a time
            TextEdit {
                text <=> root.documentText;
                visible: !root.markdown_view_enabled;
            }
            TextEdit {
                text: root.markdownViewText;
                visible: root.markdown_view_enabled;
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
}

impl Tab {
    fn new_regular(title: &str, document: Rope) -> Self {
        Self {
            title: title.into(),
            document,
            is_scratch: false,
            scratch_id: None,
        }
    }

    fn new_scratch(document: Rope, scratch_id: String) -> Self {
        Self {
            title: format!("Scratch {}", scratch_id).into(),
            document,
            is_scratch: true,
            scratch_id: Some(scratch_id),
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
    last_save: SystemTime,
}

impl EditorState {
    fn new() -> Self {
        let tabs = vec![Tab::new_regular("Tab 1", Rope::from("Hello, world!\n"))];
        let groups = vec![Group {
            tabs,
            active_tab: 0,
        }];
        let scratch_dir = scratch::ensure_scratch_dirs();
        Self {
            groups,
            active_group: 0,
            scratch_manager: ScratchManager::new(scratch_dir),
            last_save: SystemTime::now(),
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
            // Load the new tab's content
            ui.set_documentText(s.document_text());
            ui.set_active_tab(s.groups[ag].active_tab as i32);
            // Update markdown view if enabled
            if ui.get_markdown_view_enabled() {
                let rendered = markdown::render_markdown_to_html(s.document_text().as_ref());
                ui.set_markdownViewText(rendered.into());
            }
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
            s.groups[ag].tabs.push(Tab::new_regular(
                &format!("Tab {}", new_index + 1),
                Rope::new(),
            ));
            s.groups[ag].active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(new_index as i32);
        }
    });

    // Handle new scratch document
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_new_scratch(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            // Create scratch document with content from current state
            let scratch_id = s.scratch_manager.next_id_public();
            let ag = s.active_group;
            let new_index = s.groups[ag].tabs.len();
            s.groups[ag]
                .tabs
                .push(Tab::new_scratch(Rope::new(), scratch_id));
            s.groups[ag].active_tab = new_index;
            ui.set_tab_titles(make_model(s.tab_titles()));
            ui.set_tab_count(s.groups[ag].tabs.len() as i32);
            ui.set_documentText(s.document_text());
            ui.set_active_tab(new_index as i32);
        }
    });

    // Handle save scratch (autosave/debounce)
    let ui_weak = ui.as_weak();
    let state_clone = state.clone();
    ui.on_save_scratch(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let mut s = state_clone.borrow_mut();
            let ag = s.active_group;
            let active_tab = s.groups[ag].active_tab;
            // Save content back to the tab
            let current_text = ui.get_documentText();
            s.groups[ag].tabs[active_tab].document = Rope::from_str(current_text.as_ref());

            let is_scratch = s.groups[ag].tabs[active_tab].is_scratch;
            if is_scratch {
                // Clone what we need to avoid borrow conflicts
                let scratch_id = s.groups[ag].tabs[active_tab].scratch_id.clone();
                let doc_clone = s.groups[ag].tabs[active_tab].document.clone();
                let base_dir = s.scratch_manager.base_dir().to_path_buf();

                if let Some(ref sid) = scratch_id {
                    let mut index = scratch::ScratchIndex::load(&base_dir);
                    if let Some(mut scratch_doc) = s
                        .scratch_manager
                        .create_scratch_doc_for_save(sid.clone(), doc_clone)
                    {
                        if s.scratch_manager
                            .save_scratch(&mut scratch_doc, &mut index)
                            .is_err()
                        {
                            eprintln!("Failed to autosave scratch document");
                        }
                        // Update the tab title based on content
                        s.groups[ag].tabs[active_tab].title = scratch_doc.effective_title().into();
                    }
                }
            }
            s.last_save = SystemTime::now();
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
                    tabs: vec![Tab::new_regular("Tab 1", Rope::new())],
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

    // Handle markdown view toggle
    let ui_weak = ui.as_weak();
    ui.on_toggle_markdown_view(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let was_enabled = ui.get_markdown_view_enabled();
            let new_enabled = !was_enabled;

            if new_enabled {
                // Render markdown when enabling view
                let current_text = ui.get_documentText();
                let rendered = markdown::render_markdown_to_html(current_text.as_ref());
                ui.set_markdownViewText(rendered.into());
            }

            ui.set_markdown_view_enabled(new_enabled);
        }
    });

    ui.run()
}

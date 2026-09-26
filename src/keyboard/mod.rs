#![allow(dead_code)]

use windows::Win32::UI::Input::KeyboardAndMouse::*;

/// Check if a virtual key is currently down
pub fn is_key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) & 0x8000u16 as i16) != 0 }
}

/// Check if Ctrl key is pressed
pub fn is_ctrl_pressed() -> bool {
    is_key_down(VK_CONTROL.0 as i32)
}

/// Check if Shift key is pressed
pub fn is_shift_pressed() -> bool {
    is_key_down(VK_SHIFT.0 as i32)
}

/// Check if Alt key is pressed
pub fn is_alt_pressed() -> bool {
    is_key_down(VK_MENU.0 as i32)
}

/// Keyboard shortcut checker with edge-triggered detection
pub struct ShortcutChecker {
    last_keys: Vec<char>,
}

impl Default for ShortcutChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortcutChecker {
    pub fn new() -> Self {
        Self {
            last_keys: Vec::new(),
        }
    }

    /// Check for pressed keys and return triggered action strings
    pub fn check(&mut self) -> Vec<String> {
        let mut actions = Vec::new();
        let ctrl = is_ctrl_pressed();
        let shift = is_shift_pressed();
        let alt = is_alt_pressed();

        // Check alphanumeric keys 'a'..='z' and digits '1'..='4'
        let keys_to_check: Vec<char> = ('a'..='z').chain('1'..='4').collect();

        for &c in &keys_to_check {
            let vk = if c.is_ascii_alphabetic() {
                (c as u8).to_ascii_uppercase() as i32
            } else {
                c as i32 // '1' is 0x31, etc. in ASCII and Win32 VK
            };

            if is_key_down(vk) {
                if !self.last_keys.contains(&c) {
                    self.last_keys.push(c);
                    if ctrl && !shift && !alt {
                        match c {
                            'n' => actions.push("new-scratch".to_string()),
                            'o' => actions.push("open-file".to_string()),
                            's' => actions.push("save-scratch".to_string()),
                            'f' => actions.push("toggle-search-panel".to_string()),
                            'h' => actions.push("toggle-search-panel".to_string()),
                            'r' => actions.push("toggle-markdown-view".to_string()),
                            '1'..='4' => actions.push(format!("set-group-count-{}", c)),
                            'z' => actions.push("undo".to_string()),
                            'y' => actions.push("redo".to_string()),
                            'j' => actions.push("join-lines".to_string()),
                            _ => {}
                        }
                    }
                    if ctrl && shift && !alt {
                        match c {
                            's' => actions.push("save-as".to_string()),
                            'a' => actions.push("save-all".to_string()),
                            'z' => actions.push("redo".to_string()),
                            'd' => actions.push("duplicate-line".to_string()),
                            'k' => actions.push("delete-line".to_string()),
                            _ => {}
                        }
                    }
                }
            } else if self.last_keys.contains(&c) {
                self.last_keys.retain(|&k| k != c);
            }
        }

        actions
    }
}

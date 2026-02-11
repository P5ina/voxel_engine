use winit::keyboard::KeyCode;

use crate::AppState;
use crate::ui::{UiMessage, UiScreen};

#[cfg(feature = "dev-tools")]
impl AppState {
    pub(crate) fn update_dev_screen(&mut self) -> bool {
        if self.ui_screen == UiScreen::Editor {
            self.update_editor();
            return true;
        }
        if self.ui_screen == UiScreen::Loading {
            self.update_loading();
            return true;
        }
        false
    }

    pub(crate) fn build_dev_ui(&mut self) -> Option<Option<UiMessage>> {
        let msg = match self.ui_screen {
            UiScreen::DevTools => ui::dev_tools(&self.egui.ctx),
            UiScreen::WorldSelect => ui::world_select(&self.egui.ctx, &mut self.world_select_state),
            UiScreen::Editor => ui::editor_hud(&self.egui.ctx, &mut self.editor_state, self.fps),
            UiScreen::EditorPause => ui::editor_pause(&self.egui.ctx),
            UiScreen::Loading => {
                ui::loading_screen(&self.egui.ctx, &self.loading_state);
                None
            }
            _ => return None,
        };
        Some(msg)
    }

    pub(crate) fn render_dev_3d(&self) -> bool {
        matches!(self.ui_screen, UiScreen::Editor | UiScreen::EditorPause)
    }

    pub(crate) fn handle_dev_ui_message(&mut self, msg: &UiMessage) -> bool {
        match msg {
            UiMessage::EditorQuitToMenu => {
                self.region_manager = None;
                self.ui_screen = UiScreen::MainMenu;
                self.grab_mouse(false);
                true
            }
            UiMessage::OpenDevTools => {
                self.prev_screen = UiScreen::MainMenu;
                self.ui_screen = UiScreen::DevTools;
                true
            }
            UiMessage::OpenWorldSelect => {
                self.world_select_state.worlds = self.scan_worlds();
                self.world_select_state.creating = false;
                self.world_select_state.new_map_name.clear();
                self.prev_screen = UiScreen::DevTools;
                self.ui_screen = UiScreen::WorldSelect;
                true
            }
            UiMessage::PlayWorld(path) => {
                self.enter_editor_after_gen = false;
                self.load_big_world_from_file(path);
                true
            }
            UiMessage::EditWorld(path) => {
                self.enter_editor_after_gen = true;
                self.load_big_world_from_file(path);
                true
            }
            UiMessage::CreateNewMap(name) => {
                self.editor_state.level_name = name.clone();
                self.enter_editor_after_gen = true;
                self.start_big_world_generation();
                true
            }
            UiMessage::SaveBigWorld => {
                self.save_big_world_to_file();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn dev_resume_target(&self) -> Option<UiScreen> {
        if self.prev_screen == UiScreen::Editor || self.ui_screen == UiScreen::EditorPause {
            Some(UiScreen::Editor)
        } else {
            None
        }
    }

    pub(crate) fn handle_dev_escape(&mut self) -> bool {
        match self.ui_screen {
            UiScreen::Editor => {
                self.prev_screen = UiScreen::Editor;
                self.ui_screen = UiScreen::EditorPause;
                self.grab_mouse(false);
                true
            }
            UiScreen::EditorPause => {
                self.ui_screen = UiScreen::Editor;
                self.grab_mouse(true);
                true
            }
            UiScreen::DevTools => {
                self.ui_screen = UiScreen::MainMenu;
                true
            }
            UiScreen::WorldSelect => {
                self.ui_screen = UiScreen::DevTools;
                true
            }
            UiScreen::Loading => true,
            _ => false,
        }
    }

    pub(crate) fn handle_dev_key(&mut self, code: KeyCode) -> bool {
        if self.ui_screen == UiScreen::Editor && self.input.ctrl && code == KeyCode::KeyS {
            self.save_big_world_to_file();
            true
        } else {
            false
        }
    }

    pub(crate) fn can_move_editor(&self) -> bool {
        self.ui_screen == UiScreen::Editor
    }

    pub(crate) fn editor_active(&self) -> bool {
        self.ui_screen == UiScreen::Editor
    }
}

#[cfg(not(feature = "dev-tools"))]
impl AppState {
    pub(crate) fn update_dev_screen(&mut self) -> bool {
        if self.ui_screen == UiScreen::Loading {
            self.update_loading();
            return true;
        }
        false
    }

    pub(crate) fn build_dev_ui(&mut self) -> Option<Option<UiMessage>> {
        if self.ui_screen == UiScreen::Loading {
            ui::loading_screen(&self.egui.ctx, &self.loading_state);
            return Some(None);
        }
        None
    }

    pub(crate) fn render_dev_3d(&self) -> bool {
        false
    }

    pub(crate) fn handle_dev_ui_message(&mut self, _msg: &UiMessage) -> bool {
        false
    }

    pub(crate) fn dev_resume_target(&self) -> Option<UiScreen> {
        None
    }

    pub(crate) fn handle_dev_escape(&mut self) -> bool {
        false
    }

    pub(crate) fn handle_dev_key(&mut self, _code: KeyCode) -> bool {
        false
    }

    pub(crate) fn can_move_editor(&self) -> bool {
        false
    }

    pub(crate) fn editor_active(&self) -> bool {
        false
    }
}

use crate::ui;

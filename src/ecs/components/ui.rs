//! UI-related components

use specs::{Component, VecStorage};

/// UI screen state component
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[storage(VecStorage)]
pub enum UiScreen {
    MainMenu,
    MapSelect,
    Matchmaking,
    Settings,
    InGame,
    PauseMenu,
    Loading,
    Editor,
    DevTools,
    WorldSelect,
}

impl Default for UiScreen {
    fn default() -> Self {
        UiScreen::MainMenu
    }
}

/// FPS display component
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct FpsDisplay {
    pub current_fps: f32,
    pub frame_count: u32,
    pub accum_time: f32,
}

/// Debug display settings
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct DebugDisplay {
    pub enabled: bool,
    pub show_chunks: bool,
    pub show_physics: bool,
    pub show_network: bool,
}

/// Loading screen state
#[derive(Component, Debug, Clone, Default)]
#[storage(VecStorage)]
pub struct LoadingState {
    pub message: String,
    pub progress: f32,
    pub total_items: usize,
    pub completed_items: usize,
}

impl LoadingState {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            progress: 0.0,
            total_items: 0,
            completed_items: 0,
        }
    }

    pub fn update_progress(&mut self, completed: usize, total: usize) {
        self.completed_items = completed;
        self.total_items = total;
        if total > 0 {
            self.progress = completed as f32 / total as f32;
        }
    }
}

/// Editor mode state
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct EditorMode {
    pub enabled: bool,
    pub brush_size: f32,
    pub selected_material: u8,
}

/// Map selection state
#[derive(Component, Debug, Clone, Default)]
#[storage(VecStorage)]
pub struct MapSelectState {
    pub available_maps: Vec<String>,
    pub selected_index: Option<usize>,
    pub selected_path: Option<String>,
}

/// Mouse grab state for camera control
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct MouseGrabbed {
    pub grabbed: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    MainMenu,
    Settings,
    InGame,
    PauseMenu,
    LevelSelect,
    Editor,
    EditorPause,
    SaveDialog,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Play,
    Settings,
    Exit,
    Back,
    Resume,
    QuitToMenu,
    // Level select
    OpenLevelSelect,
    LoadLevel(String),
    NewLevel,
    // Editor
    OpenEditor,
    SaveLevel(String),
    EditorQuitToMenu,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum LightingMode {
    #[default]
    Simple,
    PathTracing,
}

impl LightingMode {
    pub fn name(&self) -> &'static str {
        match self {
            LightingMode::Simple => "Simple",
            LightingMode::PathTracing => "Path Tracing",
        }
    }
}

pub struct GameSettings {
    pub fov: f32,
    pub sensitivity: f32,
    pub lighting_mode: LightingMode,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fov: 70.0,
            sensitivity: 0.003,
            lighting_mode: LightingMode::Simple,
        }
    }
}

use crate::voxel::BlockType;

#[derive(Clone)]
pub struct EditorState {
    pub selected_block: BlockType,
    pub fly_speed: f32,
    pub level_name: String,
    pub show_grid: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected_block: BlockType::Stone,
            fly_speed: 20.0,
            level_name: String::from("untitled"),
            show_grid: true,
        }
    }
}

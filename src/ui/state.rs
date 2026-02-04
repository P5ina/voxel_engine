#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    MainMenu,
    Settings,
    InGame,
    PauseMenu,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Play,
    Settings,
    Exit,
    Back,
    Resume,
    QuitToMenu,
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

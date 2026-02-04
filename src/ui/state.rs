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

pub struct GameSettings {
    pub fov: f32,
    pub sensitivity: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fov: 70.0,
            sensitivity: 0.003,
        }
    }
}

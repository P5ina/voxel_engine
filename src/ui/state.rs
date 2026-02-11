#![cfg_attr(not(feature = "dev-tools"), allow(dead_code))]

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    MainMenu,
    MapSelect,
    Matchmaking,
    #[cfg(feature = "dev-tools")]
    DevTools,
    #[cfg(feature = "dev-tools")]
    WorldSelect,
    Settings,
    InGame,
    PauseMenu,
    #[cfg(feature = "dev-tools")]
    Editor,
    #[cfg(feature = "dev-tools")]
    EditorPause,
    #[cfg(feature = "dev-tools")]
    Loading,
}

/// Info about a saved world file
#[cfg(feature = "dev-tools")]
#[derive(Clone)]
pub struct WorldInfo {
    pub name: String,
    pub path: String,
    pub size_mb: f64,
    pub modified: String,
}

/// State for the world select screen
#[cfg(feature = "dev-tools")]
#[derive(Clone, Default)]
pub struct WorldSelectState {
    pub worlds: Vec<WorldInfo>,
    pub new_map_name: String,
    pub creating: bool,
}

/// Loading screen state
#[cfg(feature = "dev-tools")]
#[derive(Clone)]
pub struct LoadingState {
    pub message: String,
    pub progress: f32, // 0.0 - 1.0
    pub chunks_loaded: usize,
    pub chunks_total: usize,
}

#[cfg(feature = "dev-tools")]
impl Default for LoadingState {
    fn default() -> Self {
        Self {
            message: "Loading...".to_string(),
            progress: 0.0,
            chunks_loaded: 0,
            chunks_total: 0,
        }
    }
}

#[cfg(feature = "dev-tools")]
impl LoadingState {
    pub fn new(message: &str, total: usize) -> Self {
        Self {
            message: message.to_string(),
            progress: 0.0,
            chunks_loaded: 0,
            chunks_total: total,
        }
    }

    pub fn update(&mut self, loaded: usize) {
        self.chunks_loaded = loaded;
        if self.chunks_total > 0 {
            self.progress = loaded as f32 / self.chunks_total as f32;
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    OpenMapSelect,
    SelectMap(String),
    FindMatch,
    CancelMatchmaking,
    Settings,
    Exit,
    Back,
    Resume,
    QuitToMenu,
    // Editor
    #[cfg(feature = "dev-tools")]
    EditorQuitToMenu,
    #[cfg(feature = "dev-tools")]
    OpenDevTools,
    // World select
    #[cfg(feature = "dev-tools")]
    OpenWorldSelect,
    #[cfg(feature = "dev-tools")]
    PlayWorld(String),
    #[cfg(feature = "dev-tools")]
    EditWorld(String),
    #[cfg(feature = "dev-tools")]
    CreateNewMap(String),
    // Save
    #[cfg(feature = "dev-tools")]
    SaveBigWorld,
}

#[derive(Clone, Default)]
pub struct MapSelectState {
    pub worlds: Vec<PlayableWorld>,
}

#[derive(Clone)]
pub struct PlayableWorld {
    pub name: String,
    pub path: String,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum LightingMode {
    #[default]
    Simple,
    PathTracing,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum PerformancePreset {
    #[default]
    Potato,
    Balanced,
    High,
}

impl PerformancePreset {
    pub fn name(&self) -> &'static str {
        match self {
            PerformancePreset::Potato => "Potato",
            PerformancePreset::Balanced => "Balanced",
            PerformancePreset::High => "High",
        }
    }
}

impl LightingMode {
    pub fn name(&self) -> &'static str {
        match self {
            LightingMode::Simple => "Simple",
            LightingMode::PathTracing => "Path Tracing",
        }
    }
}

/// Brush shape for painting voxels
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BrushShape {
    #[default]
    Single,
    Sphere,
    Cube,
}

impl BrushShape {
    pub fn name(&self) -> &'static str {
        match self {
            BrushShape::Single => "Single",
            BrushShape::Sphere => "Sphere",
            BrushShape::Cube => "Cube",
        }
    }

    pub fn all() -> &'static [BrushShape] {
        &[BrushShape::Single, BrushShape::Sphere, BrushShape::Cube]
    }
}

pub struct GameSettings {
    pub fov: f32,
    pub sensitivity: f32,
    pub lighting_mode: LightingMode,
    pub performance_preset: PerformancePreset,
    pub show_debug: bool,
    pub max_bounces: u32,
    pub render_scale: f32,
    pub vsync: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        let mut s = Self {
            fov: 70.0,
            sensitivity: 0.003,
            lighting_mode: LightingMode::Simple,
            performance_preset: PerformancePreset::Balanced,
            show_debug: false,
            max_bounces: 2,
            render_scale: 1.0,
            vsync: true,
        };
        s.apply_preset();
        s
    }
}

impl GameSettings {
    pub fn apply_preset(&mut self) {
        match self.performance_preset {
            PerformancePreset::Potato => {
                self.max_bounces = 1;
                self.render_scale = 0.5;
                self.lighting_mode = LightingMode::Simple;
            }
            PerformancePreset::Balanced => {
                self.max_bounces = 2;
                self.render_scale = 0.75;
                self.lighting_mode = LightingMode::PathTracing;
            }
            PerformancePreset::High => {
                self.max_bounces = 4;
                self.render_scale = 1.0;
                self.lighting_mode = LightingMode::PathTracing;
            }
        }
    }
}

/// Debug information for the LOD/streaming overlay
#[derive(Default)]
pub struct DebugInfo {
    pub player_pos: [f32; 3],
    pub player_chunk: [i32; 3],
    pub camera_pos: Option<[f32; 3]>, // Some when free cam active
    pub total_meshes: usize,
    pub chunk_meshes: usize,
    pub lod_meshes: usize,
    // Streaming
    pub streaming_active: bool,
    pub streaming_loaded: usize,
    pub streaming_queue: usize,
    pub streaming_lod_nodes: usize,
    pub surface_total: usize,
    pub surface_requested: usize,
    pub surface_queued: usize,
    pub surface_meshed: usize,
    // Octree
    pub octree_active: bool,
    pub octree_nodes: usize,
    pub octree_data_blocks: usize,
    pub octree_depth: u8,
    // World
    pub world_chunks: usize,
}

#[derive(Clone)]
pub struct EditorState {
    /// Selected palette color index (1-255, 0 is air)
    pub selected_color: u8,
    /// Brush shape
    pub brush_shape: BrushShape,
    /// Brush size (radius for sphere, half-size for cube)
    pub brush_size: u8,
    pub fly_speed: f32,
    pub level_name: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected_color: 42, // Stone gray
            brush_shape: BrushShape::Single,
            brush_size: 2,
            fly_speed: 20.0,
            level_name: String::from("untitled"),
        }
    }
}

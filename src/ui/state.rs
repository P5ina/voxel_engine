#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    MainMenu,
    WorldSelect,
    Settings,
    InGame,
    PauseMenu,
    Editor,
    EditorPause,
    Loading,
}

/// Info about a saved world file
#[derive(Clone)]
pub struct WorldInfo {
    pub name: String,
    pub path: String,
    pub size_mb: f64,
    pub modified: String,
}

/// State for the world select screen
#[derive(Clone, Default)]
pub struct WorldSelectState {
    pub worlds: Vec<WorldInfo>,
    pub new_map_name: String,
    pub creating: bool,
}

/// Loading screen state
#[derive(Clone)]
pub struct LoadingState {
    pub message: String,
    pub progress: f32, // 0.0 - 1.0
    pub chunks_loaded: usize,
    pub chunks_total: usize,
}

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
    Settings,
    Exit,
    Back,
    Resume,
    QuitToMenu,
    // Editor
    EditorQuitToMenu,
    // World select
    OpenWorldSelect,
    PlayWorld(String),
    EditWorld(String),
    CreateNewMap(String),
    // Save
    SaveBigWorld,
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
    pub show_debug: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fov: 70.0,
            sensitivity: 0.003,
            lighting_mode: LightingMode::Simple,
            show_debug: false,
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

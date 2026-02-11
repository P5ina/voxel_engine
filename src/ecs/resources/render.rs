//! Rendering resources
//!
//! Note: Main render resources (RenderContext, PathTracer, etc.) are stored in AppRenderResources
//! in app_ecs.rs, not in the ECS World, to avoid borrow checker issues with wgpu types.

use std::collections::HashMap;

use crate::renderer::MeshResources;
use crate::MeshKey;

/// Chunk mesh storage resource
/// Maps MeshKey (Chunk or LOD node) to GPU mesh resources
///
/// Note: Debug not derived because MeshResources doesn't implement Debug
#[derive(Default)]
pub struct MeshStorage {
    pub meshes: HashMap<MeshKey, MeshResources>,
}

impl MeshStorage {
    pub fn insert(&mut self, key: MeshKey, mesh: MeshResources) {
        self.meshes.insert(key, mesh);
    }

    pub fn get(&self, key: &MeshKey) -> Option<&MeshResources> {
        self.meshes.get(key)
    }

    pub fn remove(&mut self, key: &MeshKey) -> Option<MeshResources> {
        self.meshes.remove(key)
    }
}

/// Window dimensions resource
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowDimensions {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl WindowDimensions {
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

/// Screen state for post-processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    MainMenu,
    InGame,
    Paused,
}

impl Default for ScreenState {
    fn default() -> Self {
        ScreenState::MainMenu
    }
}

/// Vsync mode setting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsyncMode {
    Enabled,
    Disabled,
    Adaptive,
}

impl Default for VsyncMode {
    fn default() -> Self {
        VsyncMode::Enabled
    }
}

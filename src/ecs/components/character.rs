//! Character and animation components

use std::sync::Arc;

use specs::{Component, NullStorage, VecStorage};

use crate::model::{AnimationPlayer, LoadedModel, SkeletonState};

/// First-person view component for FPS arms and held items
#[derive(Component, Debug)]
#[storage(VecStorage)]
pub struct FirstPersonView {
    /// The FPS arms model
    pub model: Option<Arc<LoadedModel>>,
    /// Current skeleton state for animation
    pub skeleton_state: Option<SkeletonState>,
    /// Animation player for FPS arms
    pub animation_player: AnimationPlayer,
    /// Currently held item type
    pub held_item: HeldItem,
    /// Meshes to hide based on held item
    pub hidden_meshes: Vec<String>,
    /// Camera shake offset from animation
    pub shake_offset: glam::Vec3,
    /// Camera shake rotation from animation
    pub shake_rotation: glam::Quat,
}

impl Default for FirstPersonView {
    fn default() -> Self {
        Self {
            model: None,
            skeleton_state: None,
            animation_player: AnimationPlayer::new(),
            held_item: HeldItem::None,
            hidden_meshes: Vec::new(),
            shake_offset: glam::Vec3::ZERO,
            shake_rotation: glam::Quat::IDENTITY,
        }
    }
}

impl FirstPersonView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the held item and update hidden meshes
    pub fn set_held_item(&mut self, item: HeldItem) {
        self.held_item = item;
        self.hidden_meshes = item.hidden_meshes();
    }

    /// Get camera shake from animation
    pub fn get_camera_shake(&self) -> (glam::Vec3, glam::Quat) {
        (self.shake_offset, self.shake_rotation)
    }

    /// Calculate view transform matrix for rendering
    pub fn view_transform(&self, camera_pos: glam::Vec3, camera_rot: glam::Quat) -> glam::Mat4 {
        let rotation_matrix = glam::Mat4::from_quat(camera_rot);
        let translation = glam::Mat4::from_translation(camera_pos);
        translation * rotation_matrix
    }
}

/// Types of items the player can hold
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldItem {
    None,
    Pickaxe,
    Rifle,
    Pistol,
    Melee,
}

impl HeldItem {
    /// Get mesh names to hide for this item
    pub fn hidden_meshes(&self) -> Vec<String> {
        match self {
            HeldItem::None => vec!["weapon".to_string()],
            HeldItem::Pickaxe => vec!["rifle".to_string(), "pistol".to_string()],
            HeldItem::Rifle => vec!["pickaxe".to_string(), "pistol".to_string()],
            HeldItem::Pistol => vec!["pickaxe".to_string(), "rifle".to_string()],
            HeldItem::Melee => vec![
                "pickaxe".to_string(),
                "rifle".to_string(),
                "pistol".to_string(),
            ],
        }
    }
}

/// Animation state component for skeletal animation
#[derive(Component, Debug, Clone, Default)]
#[storage(VecStorage)]
pub struct AnimationState {
    /// Current animation name
    pub current_anim: Option<String>,
    /// Animation playback time in seconds
    pub playback_time: f32,
    /// Playback speed multiplier
    pub speed: f32,
    /// Whether animation should loop
    pub looping: bool,
    /// Animation transition blend time
    pub blend_time: f32,
}

impl AnimationState {
    pub fn new() -> Self {
        Self {
            current_anim: None,
            playback_time: 0.0,
            speed: 1.0,
            looping: true,
            blend_time: 0.2,
        }
    }
}

/// Walking state for animation blending
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct WalkingState {
    pub is_walking: bool,
    pub is_sprinting: bool,
    pub walk_time: f32,
}

/// BVH dirty marker - triggers BVH rebuild when present
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct BvhDirty;

/// Third-person local player character (visible to others)
#[derive(Component, Debug)]
#[storage(VecStorage)]
pub struct LocalCharacter {
    pub model: Option<Arc<LoadedModel>>,
    pub skeleton_state: Option<SkeletonState>,
}

impl Default for LocalCharacter {
    fn default() -> Self {
        Self {
            model: None,
            skeleton_state: None,
        }
    }
}

/// Third-person remote player character
#[derive(Component, Debug)]
#[storage(VecStorage)]
pub struct RemoteCharacter {
    pub model: Option<Arc<LoadedModel>>,
    pub skeleton_state: Option<SkeletonState>,
    pub last_update: std::time::Instant,
    pub interpolation_target: Option<glam::Vec3>,
}

impl Default for RemoteCharacter {
    fn default() -> Self {
        Self {
            model: None,
            skeleton_state: None,
            last_update: std::time::Instant::now(),
            interpolation_target: None,
        }
    }
}

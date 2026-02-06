//! Local player character

use std::sync::Arc;

use glam::{Quat, Vec3};

use crate::model::{AnimationPlayer, LoadedModel, SkeletonState};

/// The local player's character representation
pub struct LocalPlayer {
    /// Model for third-person view
    pub model: Arc<LoadedModel>,
    /// Current skeleton state
    pub skeleton_state: Option<SkeletonState>,
    /// Animation player
    pub animation_player: AnimationPlayer,
    /// World position
    pub position: Vec3,
    /// Rotation (around Y axis)
    pub rotation: Quat,
    /// Scale factor
    pub scale: f32,
    /// Whether head/torso should be hidden (first-person mode)
    pub hide_head: bool,
    /// Indices of meshes to hide in first-person mode
    pub hidden_mesh_indices: Vec<usize>,
}

impl LocalPlayer {
    /// Update animation and skeleton
    pub fn update(&mut self, dt: f32) {
        if let (Some(skeleton), Some(state)) = (&self.model.skeleton, &mut self.skeleton_state) {
            self.animation_player.update(dt, skeleton, state);
        }
    }

    /// Get the transformation matrix
    pub fn transform_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.position,
        )
    }

    /// Check if a mesh should be visible (considering first-person mode)
    pub fn is_mesh_visible(&self, mesh_index: usize) -> bool {
        if !self.hide_head {
            return true;
        }
        !self.hidden_mesh_indices.contains(&mesh_index)
    }
}

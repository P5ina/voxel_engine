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
    pub fn new(model: Arc<LoadedModel>) -> Self {
        let skeleton_state = model
            .skeleton
            .as_ref()
            .map(|s| SkeletonState::from_skeleton(s));

        let mut animation_player = AnimationPlayer::new();
        for clip in &model.animations {
            animation_player.add_clip(clip.clone());
        }

        Self {
            model,
            skeleton_state,
            animation_player,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
            hide_head: true,
            hidden_mesh_indices: Vec::new(),
        }
    }

    /// Update animation and skeleton
    pub fn update(&mut self, dt: f32) {
        if let (Some(skeleton), Some(state)) = (&self.model.skeleton, &mut self.skeleton_state) {
            self.animation_player.update(dt, skeleton, state);
        }
    }

    /// Play an animation by name
    pub fn play_animation(&mut self, name: &str) {
        self.animation_player.play_by_name(name);
    }

    /// Get the transformation matrix
    pub fn transform_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.position,
        )
    }

    /// Set which meshes to hide in first-person mode
    pub fn set_hidden_meshes(&mut self, mesh_names: &[&str]) {
        self.hidden_mesh_indices = self
            .model
            .meshes
            .iter()
            .enumerate()
            .filter(|(_, m)| mesh_names.iter().any(|name| m.name.contains(name)))
            .map(|(i, _)| i)
            .collect();
    }

    /// Check if a mesh should be visible (considering first-person mode)
    pub fn is_mesh_visible(&self, mesh_index: usize) -> bool {
        if !self.hide_head {
            return true;
        }
        !self.hidden_mesh_indices.contains(&mesh_index)
    }
}

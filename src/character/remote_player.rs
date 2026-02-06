//! Remote player character (for multiplayer)

use std::sync::Arc;

use glam::{Quat, Vec3};

use crate::model::{AnimationPlayer, LoadedModel, SkeletonState};

/// A remote player's character (seen in multiplayer)
pub struct RemotePlayer {
    /// Model
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
    /// Interpolation target position (for smooth movement)
    target_position: Vec3,
    /// Interpolation target rotation
    target_rotation: Quat,
}

impl RemotePlayer {
    /// Update animation, skeleton, and interpolation
    pub fn update(&mut self, dt: f32) {
        // Interpolate position and rotation
        const INTERP_SPEED: f32 = 10.0;
        let t = (INTERP_SPEED * dt).min(1.0);
        self.position = self.position.lerp(self.target_position, t);
        self.rotation = self.rotation.slerp(self.target_rotation, t);

        // Update animation
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
}

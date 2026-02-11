//! Physics-related components

use specs::{Component, NullStorage, VecStorage};

/// Physics properties for entities
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Physics {
    /// Mass in kg (affects gravity and collision response)
    pub mass: f32,
    /// Friction coefficient (0.0 = no friction, 1.0 = max friction)
    pub friction: f32,
    /// Restitution/bounciness (0.0 = no bounce, 1.0 = perfect bounce)
    pub restitution: f32,
    /// Whether entity uses gravity
    pub use_gravity: bool,
    /// Whether entity has collision enabled
    pub has_collision: bool,
}

impl Default for Physics {
    fn default() -> Self {
        Self {
            mass: 70.0,
            friction: 0.8,
            restitution: 0.0,
            use_gravity: true,
            has_collision: true,
        }
    }
}

/// Terminal velocity component to cap maximum fall speed
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct TerminalVelocity {
    pub max_velocity: f32,
}

impl Default for TerminalVelocity {
    fn default() -> Self {
        Self { max_velocity: 50.0 }
    }
}

/// Gravity strength multiplier (default = 1.0)
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct GravityScale(pub f32);

impl Default for GravityScale {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Grounded state tracker
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct Grounded {
    pub is_grounded: bool,
    pub ground_normal: Option<glam::Vec3>,
    pub last_ground_time: f32,
}

/// Step height for auto-stepping up stairs
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct StepHeight(pub f32);

impl Default for StepHeight {
    fn default() -> Self {
        // VOXEL_SCALE + small margin for stepping
        Self(crate::voxel::VOXEL_SCALE + 0.06)
    }
}

/// Axis-aligned bounding box for collision
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Aabb {
    pub half_extents: glam::Vec3,
}

impl Aabb {
    pub fn from_player_size(width: f32, height: f32) -> Self {
        Self {
            half_extents: glam::Vec3::new(width, height / 2.0, width),
        }
    }

    pub fn min(&self, position: glam::Vec3) -> glam::Vec3 {
        position - self.half_extents
    }

    pub fn max(&self, position: glam::Vec3) -> glam::Vec3 {
        position + self.half_extents
    }
}

/// Marker for static entities that don't move
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct Static;

/// Marker for kinematic entities (moved by code, not physics)
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct Kinematic;

//! Spatial components for position, rotation, velocity, and scale

use glam::{Quat, Vec3};
use specs::{Component, VecStorage};

/// World position component
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Position(pub Vec3);

impl Default for Position {
    fn default() -> Self {
        Self(Vec3::ZERO)
    }
}

impl Position {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self(Vec3::new(x, y, z))
    }
}

/// World rotation component (quaternion)
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Rotation(pub Quat);

impl Default for Rotation {
    fn default() -> Self {
        Self(Quat::IDENTITY)
    }
}

impl Rotation {
    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        let yaw_quat = Quat::from_rotation_y(-yaw - std::f32::consts::FRAC_PI_2);
        let pitch_quat = Quat::from_rotation_x(pitch);
        Self(yaw_quat * pitch_quat)
    }
}

/// Velocity component for physics simulation
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Velocity(pub Vec3);

impl Default for Velocity {
    fn default() -> Self {
        Self(Vec3::ZERO)
    }
}

/// Scale component for non-uniform scaling
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Scale(pub Vec3);

impl Default for Scale {
    fn default() -> Self {
        Self(Vec3::ONE)
    }
}

impl Scale {
    pub fn uniform(scale: f32) -> Self {
        Self(Vec3::splat(scale))
    }
}

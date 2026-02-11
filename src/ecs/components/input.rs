//! Input-related components

use specs::{Component, NullStorage, VecStorage};

/// Input state for movement controls
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub up: bool,
    pub down: bool,
    pub ctrl: bool,
    pub sprint: bool,
}

impl InputState {
    /// Get movement input as a normalized direction vector (X=right, Z=forward)
    pub fn movement_vector(&self) -> glam::Vec3 {
        let x = (self.right as i32 - self.left as i32) as f32;
        let z = (self.forward as i32 - self.backward as i32) as f32;
        glam::Vec3::new(x, 0.0, z).normalize_or_zero()
    }

    /// Check if any movement key is pressed
    pub fn is_moving(&self) -> bool {
        self.forward || self.backward || self.left || self.right
    }

    /// Check if player wants to move upward (jump or fly up)
    pub fn wants_up(&self) -> bool {
        self.jump || self.up
    }

    /// Check if player wants to move downward
    pub fn wants_down(&self) -> bool {
        self.down
    }
}

/// Movement speed configuration
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct MovementSpeed {
    pub walk: f32,
    pub sprint: f32,
    pub freecam: f32,
    pub freecam_fast: f32,
    pub current: f32,
}

impl Default for MovementSpeed {
    fn default() -> Self {
        Self {
            walk: 45.0,
            sprint: 90.0,
            freecam: 60.0,
            freecam_fast: 200.0,
            current: 45.0,
        }
    }
}

/// Marker for entities that should respond to input
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct InputControlled;

/// Mouse input accumulator (for frame-rate independent mouse handling)
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct MouseInput {
    pub delta_x: f32,
    pub delta_y: f32,
}

impl MouseInput {
    pub fn add_delta(&mut self, dx: f32, dy: f32) {
        self.delta_x += dx;
        self.delta_y += dy;
    }

    pub fn reset(&mut self) {
        self.delta_x = 0.0;
        self.delta_y = 0.0;
    }
}

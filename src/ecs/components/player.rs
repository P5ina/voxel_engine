//! Player-related components

use specs::{Component, NullStorage, VecStorage};

/// Core player data (height, width, grounded state)
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Player {
    /// Player height in meters
    pub height: f32,
    /// Player width/depth (half-extent for X and Z)
    pub width: f32,
    /// Whether player is currently on ground
    pub on_ground: bool,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            height: 1.8,
            width: 0.3,
            on_ground: false,
        }
    }
}

impl Player {
    pub fn new(height: f32, width: f32) -> Self {
        Self {
            height,
            width,
            on_ground: false,
        }
    }

    /// Get eye position offset from player position
    pub fn eye_offset(&self) -> glam::Vec3 {
        glam::Vec3::new(0.0, self.height - 0.2, 0.0)
    }
}

/// Marker component for the local player entity
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct LocalPlayer;

/// Component for remote/networked players
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct RemotePlayer {
    /// Network player ID
    pub player_id: u64,
}

impl RemotePlayer {
    pub fn new(player_id: u64) -> Self {
        Self { player_id }
    }
}

/// Camera component attached to player or free cam entity
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Camera {
    /// Horizontal rotation in radians
    pub yaw: f32,
    /// Vertical rotation in radians
    pub pitch: f32,
    /// Field of view in radians
    pub fov: f32,
    /// Aspect ratio (width / height)
    pub aspect: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
    /// Head bob time accumulator
    pub bob_time: f32,
    /// Current bob intensity (0.0 to 1.0)
    pub bob_intensity: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
            fov: 70.0_f32.to_radians(),
            aspect: 1.0,
            near: 0.1,
            far: 1000.0,
            bob_time: 0.0,
            bob_intensity: 0.0,
        }
    }
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            aspect,
            ..Default::default()
        }
    }

    /// Get forward direction vector
    pub fn forward(&self) -> glam::Vec3 {
        glam::Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Get right direction vector
    pub fn right(&self) -> glam::Vec3 {
        self.forward().cross(glam::Vec3::Y).normalize()
    }

    /// Get head bob offset
    pub fn bob_offset(&self) -> glam::Vec3 {
        if self.bob_intensity < 0.001 {
            return glam::Vec3::ZERO;
        }

        const BOB_HEIGHT: f32 = 0.08;
        const BOB_SIDE: f32 = 0.03;

        let vertical = self.bob_time.sin().abs() * BOB_HEIGHT * self.bob_intensity;
        let horizontal = (self.bob_time * 0.5).sin() * BOB_SIDE * self.bob_intensity;

        let right = self.right();
        glam::Vec3::new(0.0, vertical, 0.0) + right * horizontal
    }

    /// Process mouse movement to update yaw/pitch
    pub fn process_mouse(&mut self, delta_x: f32, delta_y: f32, sensitivity: f32) {
        self.yaw += delta_x * sensitivity;
        self.pitch -= delta_y * sensitivity;

        let max_pitch = 89.0_f32.to_radians();
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
    }

    /// Update aspect ratio on window resize
    pub fn resize(&mut self, width: u32, height: u32) {
        if height > 0 {
            self.aspect = width as f32 / height as f32;
        }
    }
}

/// Marker component for free camera mode
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct FreeCam;

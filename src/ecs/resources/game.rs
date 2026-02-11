//! Game state resources

use std::time::Instant;

/// Game timing resource
/// Updated every frame by time_system
#[derive(Debug)]
pub struct GameTime {
    /// Time of last frame
    pub last_frame: Instant,
    /// Delta time in seconds (clamped to max 0.1s)
    pub dt: f32,
    /// Current FPS
    pub fps: f32,
    /// Frame time accumulator for FPS calculation
    pub frame_time_accum: f32,
    /// Frame count for FPS calculation
    pub frame_count: u32,
    /// Total frames elapsed
    pub total_frames: u64,
    /// Total elapsed time in seconds
    pub elapsed: f32,
}

impl Default for GameTime {
    fn default() -> Self {
        Self {
            last_frame: Instant::now(),
            dt: 0.0,
            fps: 0.0,
            frame_time_accum: 0.0,
            frame_count: 0,
            total_frames: 0,
            elapsed: 0.0,
        }
    }
}

impl GameTime {
    pub fn update(&mut self) {
        let now = Instant::now();
        self.dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.elapsed += self.dt;
        self.total_frames += 1;

        // FPS calculation (update every 0.5 seconds)
        self.frame_time_accum += self.dt;
        self.frame_count += 1;
        if self.frame_time_accum >= 0.5 {
            self.fps = self.frame_count as f32 / self.frame_time_accum;
            self.frame_time_accum = 0.0;
            self.frame_count = 0;
        }
    }
}

/// Global input state resource
/// Captures raw input from winit events
#[derive(Debug, Default)]
pub struct InputResource {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub up: bool,
    pub down: bool,
    pub ctrl: bool,
    pub sprint: bool,
    pub mouse_delta_x: f32,
    pub mouse_delta_y: f32,
}

impl InputResource {
    pub fn reset_mouse(&mut self) {
        self.mouse_delta_x = 0.0;
        self.mouse_delta_y = 0.0;
    }

    pub fn is_moving(&self) -> bool {
        self.forward || self.backward || self.left || self.right
    }
}

// Re-export GameSettings from UI module to ensure compatibility
pub use crate::ui::GameSettings;

/// Entity lookup resource for finding entities by various criteria
#[derive(Debug, Default)]
pub struct EntityLookup {
    /// Local player entity (if any)
    pub local_player: Option<specs::Entity>,
    /// Active camera entity (may be different from player in free cam)
    pub active_camera: Option<specs::Entity>,
    /// Remote players by network ID
    pub remote_players: std::collections::HashMap<u64, specs::Entity>,
}

/// Game state machine for high-level game flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Loading,
    Playing,
    Paused,
}

impl Default for GameState {
    fn default() -> Self {
        GameState::Menu
    }
}

/// Lighting resource (simple data, synced to GPU in render phase)
#[derive(Debug, Clone)]
pub struct Lighting {
    pub sun_direction: glam::Vec3,
    pub sun_intensity: f32,
    pub sun_color: glam::Vec3,
    pub time: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_direction: glam::Vec3::new(0.5, -0.8, 0.3).normalize(),
            sun_intensity: 1.0,
            sun_color: glam::Vec3::ONE,
            time: 0.0,
        }
    }
}

impl Lighting {
    /// Update lighting to a time-of-day value (0.0–1.0 = full day cycle).
    /// Production calls this with fixed `0.1` every frame.
    /// Formula matches `LightingParams::update_time` exactly.
    pub fn update_time(&mut self, time: f32) {
        let time = time % 1.0;
        let angle = time * std::f32::consts::TAU;

        let sun_y = angle.cos();
        let sun_z = angle.sin();
        let sun_x = 0.3;

        let len = (sun_x * sun_x + sun_y * sun_y + sun_z * sun_z).sqrt();
        self.sun_direction = glam::Vec3::new(sun_x / len, sun_y / len, sun_z / len);

        let sun_height = sun_y.max(0.0);
        self.sun_intensity = sun_height.powf(0.5);

        if sun_y > 0.0 {
            let dawn_dusk_factor = (1.0 - sun_height).powf(2.0) * sun_height.min(0.5) * 4.0;
            let r = 1.0 + dawn_dusk_factor * 0.3;
            let g = 0.95 - dawn_dusk_factor * 0.3;
            let b = 0.85 - dawn_dusk_factor * 0.5;
            self.sun_color = glam::Vec3::new(r.min(1.0), g.max(0.5), b.max(0.3));
        } else {
            self.sun_color = glam::Vec3::new(0.4, 0.45, 0.6);
            self.sun_intensity = 0.15;
        }

        self.time = time;
    }
}

//! Lighting system - updates sun position

use specs::{System, WriteExpect};

use crate::ecs::resources::Lighting;

pub struct LightingSystem;

impl<'a> System<'a> for LightingSystem {
    type SystemData = WriteExpect<'a, Lighting>;

    fn run(&mut self, mut lighting: Self::SystemData) {
        // Update lighting with fixed time-of-day (matches production behavior)
        lighting.update_time(0.1);
    }
}

/// Create the lighting system
pub fn lighting_system() -> LightingSystem {
    LightingSystem
}

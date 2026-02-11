//! Time system - updates game timing

use specs::{System, WriteExpect};

use crate::ecs::resources::GameTime;

pub struct TimeSystem;

impl<'a> System<'a> for TimeSystem {
    type SystemData = WriteExpect<'a, GameTime>;

    fn run(&mut self, mut game_time: Self::SystemData) {
        game_time.update();
    }
}

/// Create the time system
pub fn time_system() -> TimeSystem {
    TimeSystem
}

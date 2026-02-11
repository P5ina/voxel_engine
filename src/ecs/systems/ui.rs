//! UI system - handles UI updates and state

use specs::System;

pub struct UISystem;

impl<'a> System<'a> for UISystem {
    type SystemData = ();

    fn run(&mut self, (): Self::SystemData) {
        // UI is mostly handled by egui immediate mode
        // This system would handle any game logic related to UI state
        // such as transitioning between screens or handling UI messages
    }
}

/// Create the UI system
pub fn ui_system() -> UISystem {
    UISystem
}

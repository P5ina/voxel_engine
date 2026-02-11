//! Input system - processes raw input into component state

use specs::{Entities, Join, ReadExpect, System, WriteStorage};

use crate::ecs::components::{InputControlled, InputState, MouseInput, Player};
use crate::ecs::resources::{EntityLookup, InputResource};

pub struct InputSystem;

impl<'a> System<'a> for InputSystem {
    type SystemData = (
        Entities<'a>,
        ReadExpect<'a, InputResource>,
        ReadExpect<'a, EntityLookup>,
        WriteStorage<'a, InputState>,
        WriteStorage<'a, MouseInput>,
    );

    fn run(
        &mut self,
        (entities, input_res, lookup, mut input_states, mut mouse_inputs): Self::SystemData,
    ) {
        // Update input state on the local player
        if let Some(local_player) = lookup.local_player {
            if let Some(input) = input_states.get_mut(local_player) {
                input.forward = input_res.forward;
                input.backward = input_res.backward;
                input.left = input_res.left;
                input.right = input_res.right;
                input.jump = input_res.jump;
                input.up = input_res.up;
                input.down = input_res.down;
                input.ctrl = input_res.ctrl;
                input.sprint = input_res.sprint;
            }

            // Accumulate mouse input
            if let Some(mouse) = mouse_inputs.get_mut(local_player) {
                mouse.add_delta(input_res.mouse_delta_x, input_res.mouse_delta_y);
            }
        }
    }
}

/// Create the input system
pub fn input_system() -> InputSystem {
    InputSystem
}

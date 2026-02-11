//! Input system - processes raw input into component state

use specs::{ReadExpect, System, WriteStorage};

use crate::ecs::components::{InputState, MouseInput};
use crate::ecs::resources::{EntityLookup, InputResource};

pub struct InputSystem;

impl<'a> System<'a> for InputSystem {
    type SystemData = (
        ReadExpect<'a, InputResource>,
        ReadExpect<'a, EntityLookup>,
        WriteStorage<'a, InputState>,
        WriteStorage<'a, MouseInput>,
    );

    fn run(
        &mut self,
        (input_res, lookup, mut input_states, mut mouse_inputs): Self::SystemData,
    ) {
        // Update local player input
        if let Some(local_player) = lookup.local_player {
            Self::copy_input(&input_res, local_player, &mut input_states, &mut mouse_inputs);
        }

        // Also update active camera entity if it's different (free cam)
        if let Some(cam_entity) = lookup.active_camera {
            if lookup.local_player != Some(cam_entity) {
                Self::copy_input(&input_res, cam_entity, &mut input_states, &mut mouse_inputs);
            }
        }
    }
}

impl InputSystem {
    fn copy_input(
        input_res: &InputResource,
        entity: specs::Entity,
        input_states: &mut WriteStorage<'_, InputState>,
        mouse_inputs: &mut WriteStorage<'_, MouseInput>,
    ) {
        if let Some(input) = input_states.get_mut(entity) {
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
        if let Some(mouse) = mouse_inputs.get_mut(entity) {
            mouse.add_delta(input_res.mouse_delta_x, input_res.mouse_delta_y);
        }
    }
}

/// Create the input system
pub fn input_system() -> InputSystem {
    InputSystem
}

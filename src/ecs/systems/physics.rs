//! Player physics system

use specs::{Entities, Join, ReadExpect, ReadStorage, System, WriteStorage};

use crate::ecs::components::{
    Aabb, Grounded, InputState, MovementSpeed, Physics, Player, Position, StepHeight,
    TerminalVelocity, Velocity,
};
use crate::ecs::resources::{GameTime, WorldResource};

pub struct PlayerPhysicsSystem;

impl<'a> System<'a> for PlayerPhysicsSystem {
    type SystemData = (
        Entities<'a>,
        ReadExpect<'a, GameTime>,
        ReadExpect<'a, WorldResource>,
        ReadStorage<'a, InputState>,
        ReadStorage<'a, MovementSpeed>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Player>,
        WriteStorage<'a, Grounded>,
        ReadStorage<'a, Aabb>,
        ReadStorage<'a, Physics>,
        ReadStorage<'a, StepHeight>,
        ReadStorage<'a, TerminalVelocity>,
    );

    fn run(
        &mut self,
        (
            entities,
            game_time,
            world,
            input_states,
            speeds,
            mut positions,
            mut velocities,
            mut players,
            mut grounded,
            _aabbs,
            _physics_components,
            step_heights,
            terminal_vels,
        ): Self::SystemData,
    ) {
        const GRAVITY: f32 = 28.0;
        const DEFAULT_TERMINAL_VELOCITY: f32 = 50.0;
        const DEFAULT_STEP_HEIGHT: f32 = 0.56; // VOXEL_SCALE (0.5) + 0.06

        for (entity, pos, vel, player, input, speed) in (
            &entities,
            &mut positions,
            &mut velocities,
            &mut players,
            &input_states,
            &speeds,
        )
            .join()
        {
            let dt = game_time.dt;

            // Get optional component values with defaults
            let _use_gravity = true; // Default
            let has_collision = true; // Default
            let terminal_v = terminal_vels
                .get(entity)
                .map(|t| t.max_velocity)
                .unwrap_or(DEFAULT_TERMINAL_VELOCITY);
            let _step_h = step_heights
                .get(entity)
                .map(|s| s.0)
                .unwrap_or(DEFAULT_STEP_HEIGHT);

            // Apply gravity
            vel.0.y -= GRAVITY * dt;
            vel.0.y = vel.0.y.max(-terminal_v);

            // Apply movement input
            if input.is_moving() {
                let move_speed = if input.sprint {
                    speed.sprint
                } else {
                    speed.walk
                };

                // Calculate forward and right vectors based on camera
                // For now, simplified to just use input vector
                let move_vec = input.movement_vector();
                vel.0.x += move_vec.x * move_speed * dt;
                vel.0.z += move_vec.z * move_speed * dt;
            }

            // Handle jump
            if input.jump && player.on_ground {
                vel.0.y = 9.0;
                player.on_ground = false;
            }

            // Apply friction
            vel.0.x *= 0.8_f32.powf(dt * 60.0); // Frame-rate independent
            vel.0.z *= 0.8_f32.powf(dt * 60.0);

            // Update position with collision
            if has_collision {
                // Simple collision detection
                let new_pos = pos.0 + vel.0 * dt;

                // Check if new position collides with world
                // For now, simplified - just check ground collision
                let ground_check_y = ((new_pos.y - 0.1) / crate::voxel::VOXEL_SCALE).floor() as i32;
                let x = (new_pos.x / crate::voxel::VOXEL_SCALE).floor() as i32;
                let z = (new_pos.z / crate::voxel::VOXEL_SCALE).floor() as i32;

                if world.chunk_manager.is_solid(x, ground_check_y, z) {
                    // Hit ground
                    if vel.0.y < 0.0 {
                        vel.0.y = 0.0;
                        player.on_ground = true;
                        pos.0.y = new_pos
                            .y
                            .max((ground_check_y + 1) as f32 * crate::voxel::VOXEL_SCALE);
                    }
                } else {
                    player.on_ground = false;
                    pos.0 = new_pos;
                }
            } else {
                pos.0 += vel.0 * dt;
            }

            // Update grounded component if present
            if let Some(grounded_state) = grounded.get_mut(entity) {
                grounded_state.is_grounded = player.on_ground;
            }
        }
    }
}

/// Create the player physics system
pub fn player_physics_system() -> PlayerPhysicsSystem {
    PlayerPhysicsSystem
}

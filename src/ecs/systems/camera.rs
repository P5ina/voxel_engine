//! Camera systems - handles camera position updates and free camera movement

use specs::{Entities, Join, ReadExpect, ReadStorage, System, WriteStorage};

use crate::ecs::components::{
    Camera, FreeCam, InputState, MouseInput, MovementSpeed, Player, Position, Rotation, Velocity,
    WalkingState,
};
use crate::ecs::resources::{EntityLookup, GameSettings, GameTime, WindowDimensions};

/// Updates camera position based on player position and handles mouse input
pub struct CameraUpdateSystem;

impl<'a> System<'a> for CameraUpdateSystem {
    type SystemData = (
        Entities<'a>,
        ReadExpect<'a, GameTime>,
        ReadExpect<'a, GameSettings>,
        ReadExpect<'a, WindowDimensions>,
        ReadExpect<'a, EntityLookup>,
        ReadStorage<'a, Player>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Rotation>,
        ReadStorage<'a, InputState>,
        WriteStorage<'a, MouseInput>,
        WriteStorage<'a, Camera>,
        WriteStorage<'a, WalkingState>,
    );

    fn run(
        &mut self,
        (
            entities,
            game_time,
            settings,
            dims,
            lookup,
            players,
            positions,
            rotations,
            input_states,
            mut mouse_inputs,
            mut cameras,
            mut walking_states,
        ): Self::SystemData,
    ) {
        // Update active camera's aspect ratio
        if let Some(cam_entity) = lookup.active_camera {
            if let Some(camera) = cameras.get_mut(cam_entity) {
                camera.resize(dims.width, dims.height);
            }
        }

        // Sync camera with local player (if not in free cam)
        if let Some(local_player) = lookup.local_player {
            if let Some(active_cam) = lookup.active_camera {
                // Check if this is the player's camera (not free cam)
                if active_cam == local_player {
                    // Process mouse input for camera rotation
                    if let Some(mouse) = mouse_inputs.get_mut(local_player) {
                        if let Some(camera) = cameras.get_mut(local_player) {
                            camera.process_mouse(
                                mouse.delta_x,
                                mouse.delta_y,
                                settings.sensitivity,
                            );
                        }
                        mouse.reset();
                    }

                    // Update camera position from player
                    if let (Some(player), Some(player_pos), Some(camera)) = (
                        players.get(local_player),
                        positions.get(local_player),
                        cameras.get_mut(local_player),
                    ) {
                        // Set camera position to player eye position
                        camera.yaw = if let Some(rot) = rotations.get(local_player) {
                            // Extract yaw from rotation quaternion
                            let (yaw, _pitch, _roll) = rot.0.to_euler(glam::EulerRot::YXZ);
                            -yaw - std::f32::consts::FRAC_PI_2
                        } else {
                            camera.yaw
                        };

                        // Update walking state
                        if let Some(input) = input_states.get(local_player) {
                            if let Some(walking) = walking_states.get_mut(local_player) {
                                let was_walking = walking.is_walking;
                                walking.is_walking = input.is_moving();
                                walking.is_sprinting = input.sprint && walking.is_walking;

                                // Accumulate walk time for head bob
                                if walking.is_walking {
                                    walking.walk_time += game_time.dt;
                                    camera.bob_time = walking.walk_time * 10.0;
                                    camera.bob_intensity =
                                        if walking.is_sprinting { 1.2 } else { 1.0 };
                                } else {
                                    // Decay bob intensity
                                    camera.bob_intensity *= 0.9;
                                    if camera.bob_intensity < 0.01 {
                                        camera.bob_intensity = 0.0;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Free camera movement system
pub struct FreeCameraMovementSystem;

impl<'a> System<'a> for FreeCameraMovementSystem {
    type SystemData = (
        ReadExpect<'a, GameTime>,
        ReadStorage<'a, FreeCam>,
        ReadStorage<'a, InputState>,
        ReadStorage<'a, MovementSpeed>,
        ReadStorage<'a, Camera>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, Velocity>,
    );

    fn run(
        &mut self,
        (game_time, free_cams, inputs, speeds, cameras, mut positions, mut velocities): Self::SystemData,
    ) {
        for (_free_cam, input, speed, camera, pos, vel) in (
            &free_cams,
            &inputs,
            &speeds,
            &cameras,
            &mut positions,
            &mut velocities,
        )
            .join()
        {
            let dt = game_time.dt;

            let move_speed = if input.sprint {
                speed.freecam_fast
            } else {
                speed.freecam
            };

            // Get camera direction vectors
            let forward = camera.forward();
            let right = camera.right();

            // Calculate movement direction
            let mut move_dir = glam::Vec3::ZERO;
            if input.forward {
                move_dir += forward;
            }
            if input.backward {
                move_dir -= forward;
            }
            if input.right {
                move_dir += right;
            }
            if input.left {
                move_dir -= right;
            }
            if input.jump || input.up {
                move_dir += glam::Vec3::Y;
            }
            if input.down {
                move_dir -= glam::Vec3::Y;
            }

            // Apply movement
            if move_dir.length_squared() > 0.0 {
                move_dir = move_dir.normalize();
                pos.0 += move_dir * move_speed * dt;
            }

            // Free cam has no velocity persistence
            vel.0 = glam::Vec3::ZERO;
        }
    }
}

/// Create the camera update system
pub fn camera_update_system() -> CameraUpdateSystem {
    CameraUpdateSystem
}

/// Create the free camera movement system
pub fn free_camera_system() -> FreeCameraMovementSystem {
    FreeCameraMovementSystem
}

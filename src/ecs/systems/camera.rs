//! Camera systems - handles camera position updates and free camera movement

use specs::{ReadExpect, ReadStorage, System, WriteStorage};

use crate::ecs::components::{
    Camera, FreeCam, InputState, MovementSpeed, MouseInput, Position, Velocity, WalkingState,
};
use crate::ecs::resources::{EntityLookup, GameSettings, GameTime, WindowDimensions};

/// Updates camera rotation from mouse input and manages walking state / head bob
pub struct CameraUpdateSystem;

impl<'a> System<'a> for CameraUpdateSystem {
    type SystemData = (
        ReadExpect<'a, GameTime>,
        ReadExpect<'a, GameSettings>,
        ReadExpect<'a, WindowDimensions>,
        ReadExpect<'a, EntityLookup>,
        ReadStorage<'a, InputState>,
        WriteStorage<'a, MouseInput>,
        WriteStorage<'a, Camera>,
        WriteStorage<'a, WalkingState>,
    );

    fn run(
        &mut self,
        (
            game_time,
            settings,
            dims,
            lookup,
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

        // Process mouse input for active camera (works for both player and free cam)
        if let Some(cam_entity) = lookup.active_camera {
            if let Some(mouse) = mouse_inputs.get_mut(cam_entity) {
                if let Some(camera) = cameras.get_mut(cam_entity) {
                    camera.process_mouse(mouse.delta_x, mouse.delta_y, settings.sensitivity);
                }
                mouse.reset();
            }
        }

        // Update walking state (only when player is active camera, i.e. not free cam)
        if let Some(local_player) = lookup.local_player {
            if Some(local_player) == lookup.active_camera {
                if let Some(input) = input_states.get(local_player) {
                    if let Some(walking) = walking_states.get_mut(local_player) {
                        walking.is_walking = input.is_moving();
                        walking.is_sprinting = input.sprint && walking.is_walking;

                        if walking.is_walking {
                            walking.walk_time += game_time.dt;
                            if let Some(camera) = cameras.get_mut(local_player) {
                                camera.bob_time = walking.walk_time * 10.0;
                                camera.bob_intensity =
                                    if walking.is_sprinting { 1.2 } else { 1.0 };
                            }
                        } else if let Some(camera) = cameras.get_mut(local_player) {
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
        use specs::Join;
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

            let forward = camera.forward();
            let right = camera.right();

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

            if move_dir.length_squared() > 0.0 {
                move_dir = move_dir.normalize();
                pos.0 += move_dir * move_speed * dt;
            }

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

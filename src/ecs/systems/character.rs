//! Character animation and BVH systems

use specs::{Join, ReadExpect, ReadStorage, System, WriteStorage};

use crate::ecs::components::{
    BvhDirty, Camera, FirstPersonView as FpComponent, Position, WalkingState,
};
use crate::ecs::resources::GameTime;

/// Character animation system - updates first-person view animations
pub struct CharacterAnimationSystem;

impl<'a> System<'a> for CharacterAnimationSystem {
    type SystemData = (
        ReadExpect<'a, GameTime>,
        ReadStorage<'a, WalkingState>,
        ReadStorage<'a, Camera>,
        ReadStorage<'a, Position>,
        WriteStorage<'a, FpComponent>,
    );

    fn run(
        &mut self,
        (game_time, _walking_states, _cameras, _positions, mut fp_views): Self::SystemData,
    ) {
        for fp_view in (&mut fp_views).join() {
            let _dt = game_time.dt;

            // Update animation player
            // Note: This requires the model's skeleton which we need to get from the model
            if let Some(_model) = &fp_view.model {
                if let Some(ref mut _skeleton_state) = fp_view.skeleton_state {
                    // skeleton_state.update_from_animation(&fp_view.animation_player, model);
                    // Full implementation would need access to the Skeleton from the model
                }
            }

            // Get camera shake from animation
            // This would need to extract shake from animation curves
            // For now, keep shake at zero
            fp_view.shake_offset = glam::Vec3::ZERO;
            fp_view.shake_rotation = glam::Quat::IDENTITY;
        }
    }
}

/// Character BVH system - rebuilds BVH for path tracing
/// Note: BVH rebuild now happens in the render phase since it needs wgpu::Device
pub struct CharacterBvhSystem;

impl<'a> System<'a> for CharacterBvhSystem {
    type SystemData = (
        ReadExpect<'a, GameTime>,
        ReadStorage<'a, WalkingState>,
        ReadStorage<'a, Camera>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, FpComponent>,
        ReadStorage<'a, BvhDirty>,
    );

    fn run(
        &mut self,
        (_game_time, walking_states, cameras, positions, _fp_views, bvh_dirty): Self::SystemData,
    ) {
        // Check if any character needs BVH update
        let mut needs_rebuild = bvh_dirty.join().count() > 0;

        // Also rebuild if walking (first-person view animating)
        for walking in (&walking_states).join() {
            if walking.is_walking {
                needs_rebuild = true;
                break;
            }
        }

        if needs_rebuild {
            // Find first camera position (for first-person transform)
            let mut _camera_pos = glam::Vec3::ZERO;
            let mut _camera_rot = glam::Quat::IDENTITY;

            if let Some((camera, pos)) = (&cameras, &positions).join().next() {
                _camera_pos = pos.0 + camera.bob_offset();

                // Build rotation from yaw/pitch
                let yaw_quat =
                    glam::Quat::from_rotation_y(-camera.yaw - std::f32::consts::FRAC_PI_2);
                let pitch_quat = glam::Quat::from_rotation_x(camera.pitch);
                _camera_rot = yaw_quat * pitch_quat;
            }

            // BVH rebuild happens in render phase now since it needs wgpu::Device
            // We could set a flag here to indicate rebuild is needed
        }
    }
}

/// Create the character animation system
pub fn character_animation_system() -> CharacterAnimationSystem {
    CharacterAnimationSystem
}

/// Create the character BVH system
pub fn character_bvh_system() -> CharacterBvhSystem {
    CharacterBvhSystem
}

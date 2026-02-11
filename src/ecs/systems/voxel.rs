//! Voxel edit system - handles placing and breaking voxels

use specs::{Join, Read, ReadExpect, ReadStorage, System, WriteExpect};

use crate::ecs::components::{Camera, InputState, Position};
use crate::ecs::resources::{EntityLookup, WorldResource};
use crate::voxel::raycast::raycast;
use crate::voxel::VOXEL_SCALE;

pub struct VoxelEditSystem;

impl<'a> System<'a> for VoxelEditSystem {
    type SystemData = (
        ReadExpect<'a, EntityLookup>,
        WriteExpect<'a, WorldResource>,
        ReadStorage<'a, Camera>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, InputState>,
    );

    fn run(&mut self, (lookup, mut world, cameras, positions, _inputs): Self::SystemData) {
        // Get active camera for raycast
        let camera_entity = lookup.active_camera.or(lookup.local_player);

        if let Some(cam_entity) = camera_entity {
            if let (Some(camera), Some(pos)) = (cameras.get(cam_entity), positions.get(cam_entity))
            {
                let ray_origin = pos.0;
                let ray_dir = camera.forward();

                // Raycast to find hit voxel
                let max_dist = 10.0;
                let _hit = raycast(ray_origin, ray_dir, max_dist, |x, y, z| {
                    world.chunk_manager.is_solid(x, y, z)
                });

                // Check for input to edit voxels
                // Note: This is simplified - actual implementation would check
                // mouse button events stored in resources

                // If voxels were modified, mark chunks dirty
                // world.mark_dirty(chunk_pos);
            }
        }
    }
}

/// Create the voxel edit system
pub fn voxel_edit_system() -> VoxelEditSystem {
    VoxelEditSystem
}

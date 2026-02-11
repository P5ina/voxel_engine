//! Chunk streaming system - manages LOD and chunk loading

use specs::{Join, ReadExpect, ReadStorage, System, WriteExpect};

use crate::ecs::components::{Camera, Position};
use crate::ecs::resources::{EntityLookup, GameTime, MeshGenerationResource, WorldResource};

pub struct StreamingSystem;

impl<'a> System<'a> for StreamingSystem {
    type SystemData = (
        ReadExpect<'a, GameTime>,
        ReadExpect<'a, EntityLookup>,
        WriteExpect<'a, WorldResource>,
        WriteExpect<'a, MeshGenerationResource>,
        ReadStorage<'a, Camera>,
        ReadStorage<'a, Position>,
    );

    fn run(
        &mut self,
        (game_time, lookup, mut world, mut mesh_gen, cameras, positions): Self::SystemData,
    ) {
        if !world.use_streaming {
            return;
        }

        // Get camera position for streaming
        let camera_pos = if let Some(cam_entity) = lookup.active_camera {
            positions
                .get(cam_entity)
                .map(|p| p.0)
                .or_else(|| {
                    lookup
                        .local_player
                        .and_then(|e| positions.get(e).map(|p| p.0))
                })
                .unwrap_or(glam::Vec3::ZERO)
        } else {
            glam::Vec3::ZERO
        };

        // Update chunk streamer if available
        if let Some(streamer) = &mut world.streamer {
            // TODO: Implement streaming logic
            // This would check which chunks need to be loaded/unloaded
            // based on player position

            // Request meshes for newly visible chunks
            // for chunk_pos in streamer.get_visible_chunks(camera_pos) {
            //     if world.chunk_manager.get_chunk(chunk_pos).is_none() {
            //         // Load from region manager
            //     }
            //     mesh_gen.request_mesh(chunk_pos);
            // }
        }

        // Update chunks_ready flag
        if !world.chunks_ready {
            // Check if nearby chunks are loaded and meshed
            // TODO: Implement actual check
            world.chunks_ready = true;
        }
    }
}

/// Create the streaming system
pub fn streaming_system() -> StreamingSystem {
    StreamingSystem
}

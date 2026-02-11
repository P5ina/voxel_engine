#![cfg_attr(not(feature = "dev-tools"), allow(dead_code))]

#[cfg(feature = "dev-tools")]
use glam::Vec3;
#[cfg(feature = "dev-tools")]
use specs::WorldExt;

use crate::ui::DebugInfo;
use crate::world::ChunkPosition;
use crate::{AppState, MeshKey};

impl AppState {
    pub(crate) fn build_debug_info(&self) -> DebugInfo {
        let player_chunk = ChunkPosition::from_world_pos(
            self.player.position.x,
            self.player.position.y,
            self.player.position.z,
        );

        let mut chunk_meshes = 0usize;
        let mut lod_meshes = 0usize;
        for key in self.chunk_meshes.keys() {
            match key {
                MeshKey::Chunk(_) => chunk_meshes += 1,
                MeshKey::LodNode(_) => lod_meshes += 1,
            }
        }

        let mut surface_total = 0usize;
        let mut surface_requested = 0usize;
        let mut surface_queued = 0usize;
        let mut surface_meshed = 0usize;

        if let Some(streamer) = &self.chunk_streamer {
            let lod0_distance = crate::world::lod::LodConfig::default().distances[0];
            let chunk_world_size = crate::voxel::CHUNK_SIZE as f32 * crate::voxel::VOXEL_SCALE;
            let max_chunk_radius = (lod0_distance / chunk_world_size).ceil() as i32 + 1;

            for dx in -max_chunk_radius..=max_chunk_radius {
                for dz in -max_chunk_radius..=max_chunk_radius {
                    let cx = player_chunk.x + dx;
                    let cz = player_chunk.z + dz;
                    let center_vx =
                        cx * crate::voxel::CHUNK_SIZE as i32 + crate::voxel::CHUNK_SIZE as i32 / 2;
                    let center_vz =
                        cz * crate::voxel::CHUNK_SIZE as i32 + crate::voxel::CHUNK_SIZE as i32 / 2;
                    let surface_chunk_y = Self::terrain_height(center_vx, center_vz)
                        / crate::voxel::CHUNK_SIZE as i32;

                    for dy in -1..=1 {
                        let pos = ChunkPosition::new(cx, surface_chunk_y + dy, cz);
                        let center = pos.center_world_pos();
                        let dist = glam::Vec3::new(
                            center.0 - self.player.position.x,
                            center.1 - self.player.position.y,
                            center.2 - self.player.position.z,
                        )
                        .length();
                        if dist > lod0_distance {
                            continue;
                        }

                        surface_total += 1;

                        if streamer.has_mesh(pos) {
                            surface_meshed += 1;
                        } else if streamer.is_queued_lod0(pos) {
                            surface_queued += 1;
                        } else if streamer.needs_mesh(pos) || self.streaming_inflight.contains(&pos)
                        {
                            surface_requested += 1;
                        }
                    }
                }
            }
        }

        DebugInfo {
            player_pos: self.player.position.to_array(),
            player_chunk: [player_chunk.x, player_chunk.y, player_chunk.z],
            camera_pos: if self.free_cam {
                Some(self.camera.position.to_array())
            } else {
                None
            },
            total_meshes: self.chunk_meshes.len(),
            chunk_meshes,
            lod_meshes,
            streaming_active: self.use_streaming,
            streaming_loaded: self.chunk_streamer.as_ref().map_or(0, |s| s.loaded_count()),
            streaming_queue: self.chunk_streamer.as_ref().map_or(0, |s| s.queue_size()),
            streaming_lod_nodes: self
                .chunk_streamer
                .as_ref()
                .map_or(0, |s| s.loaded_lod_count()),
            surface_total,
            surface_requested,
            surface_queued,
            surface_meshed,
            octree_active: self.octree.is_some(),
            octree_nodes: self.octree.as_ref().map_or(0, |o| o.node_count()),
            octree_data_blocks: self.octree.as_ref().map_or(0, |o| o.data_count()),
            octree_depth: self.octree.as_ref().map_or(0, |o| o.depth()),
            world_chunks: self.world.chunk_count(),
        }
    }

    #[cfg(feature = "dev-tools")]
    pub(crate) fn update_editor(&mut self) {
        // Run ECS systems (timing + lighting)
        self.ecs_dispatcher.dispatch(&self.ecs_world);

        let game_time = self.ecs_world.read_resource::<crate::ecs::resources::GameTime>();
        let dt = game_time.dt;
        drop(game_time);

        // Sync ECS Lighting → render LightingParams
        {
            let ecs_lighting = self.ecs_world.read_resource::<crate::ecs::resources::Lighting>();
            self.lighting.sun_direction = ecs_lighting.sun_direction.to_array();
            self.lighting.sun_intensity = ecs_lighting.sun_intensity;
            self.lighting.sun_color = ecs_lighting.sun_color.to_array();
        }

        // Free camera movement
        let speed = self.editor_state.fly_speed * dt;
        let forward = self.camera.forward();
        let right = self.camera.right();
        let up = Vec3::Y;

        let mut move_dir = Vec3::ZERO;

        if self.input.forward {
            move_dir += forward;
        }
        if self.input.backward {
            move_dir -= forward;
        }
        if self.input.right {
            move_dir += right;
        }
        if self.input.left {
            move_dir -= right;
        }
        if self.input.jump || self.input.up {
            move_dir += up;
        }
        if self.input.down {
            move_dir -= up;
        }

        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize() * speed;
            self.camera.position += move_dir;
            // Keep player position synced for spawn point
            self.player.position =
                self.camera.position - Vec3::new(0.0, self.player.height - 0.2, 0.0);
        }

        self.camera.fov = self.game_settings.fov.to_radians();
        self.camera_resources
            .update(&self.render_ctx.queue, &self.camera);

        if self.use_streaming {
            self.update_streaming();
        }

        if !self.world.dirty_chunks().is_empty() || !self.pending_chunks.is_empty() {
            self.rebuild_dirty_meshes();
        }
    }
}

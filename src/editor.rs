use glam::Vec3;

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
            octree_active: self.octree.is_some(),
            octree_nodes: self.octree.as_ref().map_or(0, |o| o.node_count()),
            octree_data_blocks: self.octree.as_ref().map_or(0, |o| o.data_count()),
            octree_depth: self.octree.as_ref().map_or(0, |o| o.depth()),
            world_chunks: self.world.chunk_count(),
        }
    }

    pub(crate) fn update_editor(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        // FPS calculation
        self.frame_time_accum += dt;
        self.frame_count += 1;
        if self.frame_time_accum >= 0.5 {
            self.fps = self.frame_count as f32 / self.frame_time_accum;
            self.frame_time_accum = 0.0;
            self.frame_count = 0;
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
        self.lighting.update_time(0.1);

        if !self.world.dirty_chunks().is_empty() || !self.pending_chunks.is_empty() {
            self.rebuild_dirty_meshes();
        }
    }
}

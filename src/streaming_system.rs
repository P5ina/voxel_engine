use std::collections::{HashMap, HashSet};

use crate::renderer::MeshResources;
use crate::ui::PerformancePreset;
use crate::voxel::{self, AIR, Chunk, generate_chunk_mesh, generate_octree_lod_mesh};
use crate::world::{self, ChunkManager, ChunkPosition, ColumnPos, RegionCoord};
use crate::{AppState, LodMeshResult, MeshKey, StreamingMeshResult};

impl AppState {
    pub(crate) fn rebuild_dirty_meshes(&mut self) {
        // Add new dirty chunks to the pending queue
        let dirty = self.world.take_dirty_chunks();
        for chunk_pos in dirty {
            if !self.pending_set.contains(&chunk_pos) {
                self.pending_set.insert(chunk_pos);
                self.pending_chunks.push_back(chunk_pos);
            }
        }

        if self.pending_chunks.is_empty() {
            return;
        }

        // Sort pending chunks by distance from player (closer = higher priority)
        let player_pos = self.player_position();
        let player_chunk = ChunkPosition::from_world_pos(
            player_pos.x,
            player_pos.y,
            player_pos.z,
        );

        // Convert to vec, sort by distance, convert back to deque
        let mut chunks_vec: Vec<_> = self.pending_chunks.drain(..).collect();
        chunks_vec.sort_by_key(|pos| {
            let dx = pos.x - player_chunk.x;
            let dy = pos.y - player_chunk.y;
            let dz = pos.z - player_chunk.z;
            dx * dx + dy * dy + dz * dz
        });
        self.pending_chunks = chunks_vec.into();

        // In streaming mode, dispatch dirty meshes to background threads
        if let Some(ref tx) = self.streaming_mesh_tx {
            let num_to_process = self.pending_chunks.len().min(Self::CHUNKS_PER_FRAME);
            let mut dispatched = HashSet::new();

            for _ in 0..num_to_process {
                if let Some(chunk_pos) = self.pending_chunks.pop_front() {
                    self.pending_set.remove(&chunk_pos);

                    if self.streaming_inflight.contains(&chunk_pos) {
                        continue;
                    }
                    if self.streaming_inflight.len() >= Self::MAX_INFLIGHT {
                        // Re-queue for next frame
                        self.pending_chunks.push_back(chunk_pos);
                        self.pending_set.insert(chunk_pos);
                        break;
                    }

                    if self.world.get_chunk(chunk_pos).is_none() {
                        self.chunk_meshes.remove(&MeshKey::Chunk(chunk_pos));
                        continue;
                    }

                    // Snapshot chunk + neighbors for background meshing (9 column lookups)
                    let mut neighbor_chunks: HashMap<ChunkPosition, Chunk> = HashMap::new();
                    let col = ColumnPos::from_chunk_pos(chunk_pos);
                    for dx in -1..=1i32 {
                        for dz in -1..=1i32 {
                            let nc = ColumnPos::new(col.x + dx, col.z + dz);
                            if let Some(column) = self.world.get_column(nc) {
                                for dy in -1..=1i32 {
                                    let sy = chunk_pos.y + dy;
                                    if sy < 0 || sy >= voxel::NUM_SECTIONS as i32 {
                                        continue;
                                    }
                                    if let Some(section) = column.get_section(sy as u8) {
                                        neighbor_chunks.insert(
                                            ChunkPosition::new(nc.x, sy, nc.z),
                                            section.clone(),
                                        );
                                    }
                                }
                            }
                        }
                    }

                    self.streaming_inflight.insert(chunk_pos);
                    dispatched.insert(chunk_pos);
                    let tx = tx.clone();
                    rayon::spawn(move || {
                        let mut mini_world = ChunkManager::new();
                        for (np, nc) in neighbor_chunks {
                            mini_world.insert_chunk(np, nc);
                        }
                        let vertices = generate_chunk_mesh(&mini_world, chunk_pos);
                        let _ = tx.send(StreamingMeshResult {
                            pos: chunk_pos,
                            vertices,
                        });
                    });
                }
            }

            return;
        }

        // Fallback: synchronous mesh gen for non-streaming mode (editor on small worlds)
        let num_to_process = self.pending_chunks.len().min(Self::CHUNKS_PER_FRAME);
        let mut processed = HashSet::new();

        for _ in 0..num_to_process {
            if let Some(chunk_pos) = self.pending_chunks.pop_front() {
                self.pending_set.remove(&chunk_pos);

                let key = MeshKey::Chunk(chunk_pos);
                if self.world.get_chunk(chunk_pos).is_some() {
                    let vertices = generate_chunk_mesh(&self.world, chunk_pos);
                    if vertices.is_empty() {
                        self.chunk_meshes.remove(&key);
                    } else if let Some(mesh) = self.chunk_meshes.get_mut(&key) {
                        mesh.update(&self.render_ctx.device, &self.render_ctx.queue, &vertices);
                    } else {
                        self.chunk_meshes
                            .insert(key, MeshResources::new(&self.render_ctx.device, &vertices));
                    }
                } else {
                    self.chunk_meshes.remove(&key);
                }
                processed.insert(chunk_pos);
            }
        }

        if !processed.is_empty() {
            self.path_tracer.reset_accumulation();
        }
    }

    /// Update streaming system for large worlds
    pub(crate) fn update_streaming(&mut self) {
        let (
            max_mesh_uploads_per_frame,
            mesh_dispatch_multiplier,
            mesh_dispatch_min,
            mesh_dispatch_max,
            max_loads_per_frame,
            max_mesh_rebuilds_per_frame,
            lod_distances,
        ) = match self.game_settings.performance_preset {
            PerformancePreset::Potato => (
                20usize,
                2usize,
                8usize,
                32usize,
                24usize,
                48usize,
                [96.0, 384.0, 640.0, 896.0, 1152.0, 1408.0],
            ),
            PerformancePreset::Balanced => (
                32usize,
                3usize,
                12usize,
                48usize,
                36usize,
                72usize,
                [160.0, 640.0, 960.0, 1280.0, 1600.0, 2048.0],
            ),
            PerformancePreset::High => (
                48usize,
                4usize,
                16usize,
                64usize,
                48usize,
                96usize,
                [224.0, 768.0, 1152.0, 1536.0, 1920.0, 2304.0],
            ),
        };

        self.path_tracer.set_render_scale(
            &self.render_ctx.device,
            self.render_ctx.config.width,
            self.render_ctx.config.height,
            self.game_settings.render_scale,
        );

        let player_pos = self.player_position();

        if let Some(streamer) = &mut self.chunk_streamer {
            streamer.set_runtime_limits(
                max_loads_per_frame,
                max_mesh_rebuilds_per_frame,
                lod_distances,
            );
        }

        // 0. Region management: poll completed loads, proactive loading, unloading
        if let Some(ref mut region_mgr) = self.region_manager {
            // 0a. Poll completed region loads — insert chunks into world
            for result in region_mgr.poll_loads() {
                if let Some(err) = &result.error {
                    log::error!("[Region] Failed to load {:?}: {}", result.coord, err);
                }
                for (pos, chunk) in result.chunks {
                    if !chunk.is_empty() {
                        self.world.insert_chunk(pos, chunk);
                    }
                }
                region_mgr.mark_loaded(result.coord);
            }

            // 0b. Proactive region loading/unloading based on player position
            let px = player_pos.x;
            let pz = player_pos.z;
            let desired = region_mgr.desired_regions(px, pz);

            // Request loading of desired regions
            for coord in &desired {
                if !region_mgr.is_loaded(*coord) && !region_mgr.is_loading(*coord) {
                    region_mgr.request_load(*coord);
                }
            }

            // Region unloading disabled — all regions stay loaded permanently
        }

        // 1. Collect completed mesh results from background threads
        let mut uploads_this_frame = 0;
        if let Some(ref rx) = self.streaming_mesh_rx {
            while uploads_this_frame < max_mesh_uploads_per_frame {
                let Ok(result) = rx.try_recv() else { break };
                self.streaming_inflight.remove(&result.pos);

                // Only upload mesh if chunk is still tracked by streamer
                let still_loaded = self
                    .chunk_streamer
                    .as_ref()
                    .is_some_and(|s| s.is_loaded(result.pos));
                if !still_loaded {
                    continue; // Chunk was unloaded while in-flight, skip mesh
                }

                // Upload mesh to GPU
                let key = MeshKey::Chunk(result.pos);
                if !result.vertices.is_empty() {
                    if let Some(mesh) = self.chunk_meshes.get_mut(&key) {
                        mesh.update(
                            &self.render_ctx.device,
                            &self.render_ctx.queue,
                            &result.vertices,
                        );
                    } else {
                        self.chunk_meshes.insert(
                            key,
                            MeshResources::new(&self.render_ctx.device, &result.vertices),
                        );
                    }
                }

                // Mark mesh as built in streamer
                if let Some(s) = &mut self.chunk_streamer {
                    s.mark_mesh_built(result.pos);
                }
                uploads_this_frame += 1;
            }
        }

        // 1b. Collect completed LOD mesh results from background threads
        if let Some(ref rx) = self.lod_mesh_rx {
            for _ in 0..max_mesh_uploads_per_frame {
                let Ok(result) = rx.try_recv() else { break };
                let mesh_key = MeshKey::LodNode(result.key);
                if !result.vertices.is_empty() {
                    if let Some(mesh) = self.chunk_meshes.get_mut(&mesh_key) {
                        mesh.update(
                            &self.render_ctx.device,
                            &self.render_ctx.queue,
                            &result.vertices,
                        );
                    } else {
                        self.chunk_meshes.insert(
                            mesh_key,
                            MeshResources::new(&self.render_ctx.device, &result.vertices),
                        );
                    }
                } else {
                    self.chunk_meshes.remove(&mesh_key);
                }
                if let Some(s) = &mut self.chunk_streamer {
                    s.mark_lod_mesh_built(&result.key);
                }
            }
        }

        // Check if nearby chunks are ready (3-chunk radius around player)
        if !self.chunks_ready
            && let Some(ref streamer) = self.chunk_streamer
        {
            let player_chunk =
                ChunkPosition::from_world_pos(player_pos.x, player_pos.y, player_pos.z);
            const READY_RADIUS: i32 = 3;
            let max_y_chunk = Self::MAX_TERRAIN_VOXEL_HEIGHT / voxel::CHUNK_SIZE as i32;
            let mut all_ready = true;
            'outer: for dx in -READY_RADIUS..=READY_RADIUS {
                for dz in -READY_RADIUS..=READY_RADIUS {
                    // Only check support layers (player chunk and below).
                    // Air above the player should not block unfreeze.
                    for dy in -2..=0i32 {
                        let pos = ChunkPosition::new(
                            player_chunk.x + dx,
                            player_chunk.y + dy,
                            player_chunk.z + dz,
                        );
                        if pos.y < 0 || pos.y > max_y_chunk {
                            continue;
                        }

                        let center_vx =
                            pos.x * voxel::CHUNK_SIZE as i32 + voxel::CHUNK_SIZE as i32 / 2;
                        let center_vz =
                            pos.z * voxel::CHUNK_SIZE as i32 + voxel::CHUNK_SIZE as i32 / 2;
                        let surface_chunk_y =
                            Self::terrain_height(center_vx, center_vz) / voxel::CHUNK_SIZE as i32;
                        if pos.y > surface_chunk_y + 1 {
                            continue;
                        }

                        if !streamer.has_mesh(pos) && self.world.get_chunk(pos).is_none() {
                            // Not ready if chunk is neither meshed nor known-air
                            all_ready = false;
                            break 'outer;
                        }
                    }
                }
            }
            if all_ready {
                self.chunks_ready = true;
                log::info!("[Streaming] Nearby chunks ready — player unfrozen");
            }
        }

        let Some(streamer) = &mut self.chunk_streamer else {
            return;
        };

        // 2. Get streaming update
        let update = streamer.update(player_pos);
        let player_chunk = ChunkPosition::from_world_pos(player_pos.x, player_pos.y, player_pos.z);



        // 3. Dispatch mesh requests to background threads
        if let Some(ref tx) = self.streaming_mesh_tx {
            let mut mesh_requests = update.mesh_requests;
            mesh_requests.sort_by_key(|(pos, _lod)| {
                let dx = pos.x as i64 - player_chunk.x as i64;
                let dy = pos.y as i64 - player_chunk.y as i64;
                let dz = pos.z as i64 - player_chunk.z as i64;
                dx * dx + dy * dy + dz * dz
            });

            let max_mesh_dispatches = (rayon::current_num_threads() * mesh_dispatch_multiplier)
                .clamp(mesh_dispatch_min, mesh_dispatch_max);
            let mut dispatched_this_frame = 0usize;

            for (pos, _lod) in mesh_requests {
                if dispatched_this_frame >= max_mesh_dispatches {
                    break;
                }
                // Skip if already in-flight
                if self.streaming_inflight.contains(&pos) {
                    continue;
                }

                // Cap inflight tasks to prevent memory explosion
                if self.streaming_inflight.len() >= Self::MAX_INFLIGHT {
                    break;
                }

                // Skip if region isn't loaded yet (chunk data not available)
                if let Some(ref region_mgr) = self.region_manager {
                    let coord = RegionCoord::from_chunk_pos(pos);
                    if !region_mgr.is_loaded(coord) {
                        continue; // Region loading will trigger retry next frame
                    }
                }

                let needs_terrain = self.world.get_chunk(pos).is_none();

                // If chunk not in ChunkManager and region is loaded, it's air
                if needs_terrain {
                    streamer.mark_mesh_built(pos);
                    continue;
                }

                // Snapshot chunk + neighbors for background meshing (9 column lookups)
                let col_pos = ColumnPos::from_chunk_pos(pos);
                let chunk = self.world.get_chunk(pos).cloned().unwrap();
                let mut neighbor_chunks: HashMap<ChunkPosition, Chunk> = HashMap::new();
                for dx in -1..=1i32 {
                    for dz in -1..=1i32 {
                        let nc = ColumnPos::new(col_pos.x + dx, col_pos.z + dz);
                        if let Some(column) = self.world.get_column(nc) {
                            for dy in -1..=1i32 {
                                let sy = pos.y + dy;
                                if sy < 0 || sy >= voxel::NUM_SECTIONS as i32 {
                                    continue;
                                }
                                let np = ChunkPosition::new(nc.x, sy, nc.z);
                                if np == pos {
                                    continue;
                                }
                                if let Some(section) = column.get_section(sy as u8) {
                                    neighbor_chunks.insert(np, section.clone());
                                }
                            }
                        }
                    }
                }

                self.streaming_inflight.insert(pos);
                let tx = tx.clone();
                rayon::spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Build mini world with chunk + neighbors for meshing
                        let mut mini_world = ChunkManager::new();
                        mini_world.insert_chunk(pos, chunk);
                        for (np, nc) in neighbor_chunks {
                            mini_world.insert_chunk(np, nc);
                        }

                        generate_chunk_mesh(&mini_world, pos)
                    }));

                    let vertices = result.unwrap_or_else(|_| {
                        log::error!("[Streaming] Panic in mesh gen for chunk {:?}", pos);
                        Vec::new()
                    });
                    let _ = tx.send(StreamingMeshResult { pos, vertices });
                });
                dispatched_this_frame += 1;
            }
        }

        // 4. Process unload requests — always remove mesh so distant LOD0 doesn't
        //    overlap with LOD nodes. Chunk data stays in memory for re-meshing
        //    when the player returns.
        for pos in update.unload_requests {
            self.chunk_meshes.remove(&MeshKey::Chunk(pos));
            self.streaming_inflight.remove(&pos);
        }

        // 5. Process dirty LOD nodes — rebuild octree LOD data, dispatch mesh gen to background.
        if let Some(ref mut octree) = self.octree {
            let regenerated = octree.process_dirty_lods();
            let max_dirty_dispatches = rayon::current_num_threads().clamp(4, 16);
            let mut dirty_dispatched = 0usize;
            for key in regenerated {
                let is_tracked = self
                    .chunk_streamer
                    .as_ref()
                    .is_some_and(|s| s.is_lod_loaded(&key));
                if !is_tracked {
                    continue;
                }

                if dirty_dispatched >= max_dirty_dispatches {
                    // Remaining dirty nodes will be retried next frame via the
                    // LOD mesh retry mechanism in ChunkStreamer::update step 7.
                    break;
                }

                if let Some(ref tx) = self.lod_mesh_tx {
                    // Clone data and dispatch to background
                    let data = if let Some(d) = octree.get_node_lod_data(&key) {
                        Some(d.clone())
                    } else if let Some(v) = octree.get_node_homogeneous(&key) {
                        if v != AIR {
                            Some(world::lod::VoxelData::Homogeneous(v))
                        } else {
                            None
                        }
                    } else {
                        // Procedural LOD fallback
                        Self::generate_lod_data_static(&key)
                    };
                    if let Some(data) = data {
                        let tx = tx.clone();
                        rayon::spawn(move || {
                            let vertices = generate_octree_lod_mesh(&data, &key);
                            let _ = tx.send(LodMeshResult { key, vertices });
                        });
                        dirty_dispatched += 1;
                    } else {
                        self.chunk_meshes.remove(&MeshKey::LodNode(key));
                    }
                }
            }
        }

        // 6. Process LOD node mesh requests from streamer — dispatch to background
        if let Some(ref tx) = self.lod_mesh_tx {
            let mut lod_mesh_requests = update.lod_mesh_requests;
            lod_mesh_requests.sort_by_key(|key| {
                let center = key.center_world_pos();
                let dx = center.0 - player_pos.x;
                let dy = center.1 - player_pos.y;
                let dz = center.2 - player_pos.z;
                (key.lod_level, (dx * dx + dy * dy + dz * dz) as i64)
            });

            let max_lod_dispatches = rayon::current_num_threads().clamp(4, 16);
            let mut lod_dispatched_this_frame = 0usize;

            for key in lod_mesh_requests {
                if lod_dispatched_this_frame >= max_lod_dispatches {
                    break;
                }

                let data = if let Some(ref octree) = self.octree {
                    if let Some(d) = octree.get_node_lod_data(&key) {
                        Some(d.clone())
                    } else if let Some(v) = octree.get_node_homogeneous(&key) {
                        if v != voxel::AIR {
                            Some(world::lod::VoxelData::Homogeneous(v))
                        } else {
                            None
                        }
                    } else {
                        // Procedural LOD fallback: generate from terrain_height
                        Self::generate_lod_data_static(&key)
                    }
                } else {
                    // No octree at all — generate procedurally
                    Self::generate_lod_data_static(&key)
                };

                if let Some(data) = data {
                    let tx = tx.clone();
                    rayon::spawn(move || {
                        let vertices = generate_octree_lod_mesh(&data, &key);
                        let _ = tx.send(LodMeshResult { key, vertices });
                    });
                    lod_dispatched_this_frame += 1;
                }
                // mark_lod_mesh_built will happen when result arrives in step 1b
            }
        }

        // 7. Process LOD node unload requests
        for key in update.lod_unload_requests {
            self.chunk_meshes.remove(&MeshKey::LodNode(key));
        }

        // Note: we intentionally do NOT reset accumulation here.
        // Mesh/LOD uploads don't invalidate the voxel volume used for DDA shadows.
        // The G-buffer updates naturally each frame, and the accumulation blends in
        // the changes over a few frames — far less jarring than resetting every frame.
    }
}

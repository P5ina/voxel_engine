use std::collections::{HashMap, HashSet};

use crate::renderer::MeshResources;
use crate::voxel::{self, AIR, Chunk, generate_chunk_mesh, generate_octree_lod_mesh};
use crate::world::{self, ChunkManager, ChunkPosition, LodNodeKey, RegionCoord};
use crate::{AppState, ChunkSource, LodMeshResult, MeshKey, StreamingMeshResult};

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
        let player_chunk = ChunkPosition::from_world_pos(
            self.player.position.x,
            self.player.position.y,
            self.player.position.z,
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

                    // Snapshot chunk + neighbors for background meshing
                    let mut neighbor_chunks: HashMap<ChunkPosition, Chunk> = HashMap::new();
                    for dx in -1..=1i32 {
                        for dy in -1..=1i32 {
                            for dz in -1..=1i32 {
                                let np = ChunkPosition::new(
                                    chunk_pos.x + dx,
                                    chunk_pos.y + dy,
                                    chunk_pos.z + dz,
                                );
                                if let Some(chunk) = self.world.get_chunk(np) {
                                    neighbor_chunks.insert(np, chunk.clone());
                                } else if matches!(self.chunk_source, ChunkSource::Octree)
                                    && let Some(chunk) = self.octree.as_ref().and_then(|o| {
                                        if let Some(data) = o.get_chunk_data(np) {
                                            data.to_chunk()
                                        } else {
                                            None
                                        }
                                    })
                                {
                                    neighbor_chunks.insert(np, chunk);
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
                            new_chunk: None, // chunk already exists in world
                        });
                    });
                }
            }

            // Path tracer update for dirty chunks happens when results arrive
            // in update_streaming() step 1
            if !dispatched.is_empty() {
                self.path_tracer.reset_accumulation();
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
                        mesh.update(&self.render_ctx.device, &vertices);
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

        // Update path tracer only for processed chunks
        if !processed.is_empty() {
            if self.path_tracer.needs_resize(&self.world) {
                self.path_tracer.update_world_voxels(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    &self.world,
                );
            } else {
                self.path_tracer
                    .update_chunks(&self.render_ctx.queue, &self.world, &processed);
            }
            self.path_tracer.reset_accumulation();
        }
    }

    /// Update streaming system for large worlds
    pub(crate) fn update_streaming(&mut self) {
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
            let px = self.player.position.x;
            let pz = self.player.position.z;
            let desired = region_mgr.desired_regions(px, pz);

            // Request loading of desired regions
            for coord in &desired {
                if !region_mgr.is_loaded(*coord) && !region_mgr.is_loading(*coord) {
                    region_mgr.request_load(*coord);
                }
            }

            // Unload distant regions
            let to_unload = region_mgr.regions_to_unload(px, pz);
            for coord in to_unload {
                // In editor mode, flush dirty chunks back to octree before unloading
                if matches!(self.chunk_source, ChunkSource::Octree) {
                    let chunks = self.world.chunks_in_region(coord);
                    for (pos, chunk) in &chunks {
                        if self.world.dirty_chunks().contains(pos)
                            && let Some(ref mut octree) = self.octree
                        {
                            octree.insert_chunk(
                                *pos,
                                world::lod::VoxelData::from_full(*chunk.data()),
                            );
                        }
                    }
                }
                let removed = self.world.remove_region_chunks(coord);
                for pos in &removed {
                    self.chunk_meshes.remove(&MeshKey::Chunk(*pos));
                    self.streaming_inflight.remove(pos);
                    if let Some(s) = &mut self.chunk_streamer {
                        s.mark_dirty(*pos);
                    }
                }
                region_mgr.unload(coord);
            }
        }

        // 1. Collect completed mesh results from background threads
        const MAX_MESH_UPLOADS_PER_FRAME: usize = 64;
        let mut dirty_positions: HashSet<ChunkPosition> = HashSet::new();
        let mut uploads_this_frame = 0;
        if let Some(ref rx) = self.streaming_mesh_rx {
            while uploads_this_frame < MAX_MESH_UPLOADS_PER_FRAME {
                let Ok(result) = rx.try_recv() else { break };
                self.streaming_inflight.remove(&result.pos);

                // Insert newly generated terrain into world (skip octree — LOD data
                // was already computed during generation; re-inserting would dirty
                // LOD nodes whose siblings are stripped, causing incorrect LOD)
                if let Some(chunk) = result.new_chunk {
                    self.world.insert_chunk(result.pos, chunk);

                    // Clear dirty flag on the chunk itself — its mesh was just built
                    // by rayon. Only neighbors need synchronous rebuild.
                    self.world.clear_chunk_dirty(result.pos);

                    // Mark face neighbors dirty so their boundary meshes update
                    let p = result.pos;
                    for np in [
                        ChunkPosition::new(p.x - 1, p.y, p.z),
                        ChunkPosition::new(p.x + 1, p.y, p.z),
                        ChunkPosition::new(p.x, p.y - 1, p.z),
                        ChunkPosition::new(p.x, p.y + 1, p.z),
                        ChunkPosition::new(p.x, p.y, p.z - 1),
                        ChunkPosition::new(p.x, p.y, p.z + 1),
                    ] {
                        if self.world.get_chunk(np).is_some() {
                            self.world.mark_chunk_dirty(np);
                        }
                    }
                }

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
                        mesh.update(&self.render_ctx.device, &result.vertices);
                    } else {
                        self.chunk_meshes.insert(
                            key,
                            MeshResources::new(&self.render_ctx.device, &result.vertices),
                        );
                    }
                }

                dirty_positions.insert(result.pos);

                // Mark mesh as built in streamer
                if let Some(s) = &mut self.chunk_streamer {
                    s.mark_mesh_built(result.pos);
                }
                uploads_this_frame += 1;
            }
        }

        // 1b. Collect completed LOD mesh results from background threads
        if let Some(ref rx) = self.lod_mesh_rx {
            for _ in 0..MAX_MESH_UPLOADS_PER_FRAME {
                let Ok(result) = rx.try_recv() else { break };
                let mesh_key = MeshKey::LodNode(result.key);
                if !result.vertices.is_empty() {
                    if let Some(mesh) = self.chunk_meshes.get_mut(&mesh_key) {
                        mesh.update(&self.render_ctx.device, &result.vertices);
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
            let player_chunk = ChunkPosition::from_world_pos(
                self.player.position.x,
                self.player.position.y,
                self.player.position.z,
            );
            const READY_RADIUS: i32 = 3;
            let mut all_ready = true;
            'outer: for dx in -READY_RADIUS..=READY_RADIUS {
                for dz in -READY_RADIUS..=READY_RADIUS {
                    // Only check the Y layers around the player (feet and below)
                    for dy in -1..=1i32 {
                        let pos = ChunkPosition::new(
                            player_chunk.x + dx,
                            player_chunk.y + dy,
                            player_chunk.z + dz,
                        );
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
        let update = streamer.update(self.player.position);

        let has_unload_requests = !update.unload_requests.is_empty();

        // 3. Dispatch mesh requests to background threads
        if let Some(ref tx) = self.streaming_mesh_tx {
            for (pos, _lod) in update.mesh_requests {
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

                // Skip air chunks early — avoid cloning 26 neighbors for empty space
                if needs_terrain {
                    let is_air = match self.chunk_source {
                        ChunkSource::Octree => self.octree.as_ref().is_some_and(|o| {
                            let lod_key = LodNodeKey::new(pos.x, pos.y, pos.z, 0);
                            o.get_node_homogeneous(&lod_key) == Some(0)
                        }),
                        ChunkSource::Procedural => {
                            // Terrain gen returns air above max height or below y=0
                            let max_y_chunk =
                                Self::MAX_TERRAIN_VOXEL_HEIGHT / voxel::CHUNK_SIZE as i32;
                            pos.y > max_y_chunk || pos.y < 0
                        }
                    };
                    if is_air {
                        // Mark as built so streamer doesn't keep retrying and
                        // chunks_ready can become true for air regions
                        streamer.mark_mesh_built(pos);
                        continue;
                    }
                }

                // If chunk not in ChunkManager, try to source it
                let existing_chunk = if needs_terrain {
                    match self.chunk_source {
                        ChunkSource::Octree => {
                            // Read from octree on main thread
                            self.octree.as_ref().and_then(|o| {
                                if let Some(data) = o.get_chunk_data(pos) {
                                    data.to_chunk()
                                } else if let Some(v) =
                                    o.get_node_homogeneous(&LodNodeKey::new(pos.x, pos.y, pos.z, 0))
                                {
                                    if v != 0 {
                                        let mut chunk = Chunk::new();
                                        chunk.fill_ground(voxel::CHUNK_SIZE, v);
                                        Some(chunk)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                        }
                        ChunkSource::Procedural => None, // will generate in background
                    }
                } else {
                    self.world.get_chunk(pos).cloned()
                };

                // Snapshot neighbors for background meshing
                let mut neighbor_chunks: HashMap<ChunkPosition, Chunk> = HashMap::new();
                for dx in -1..=1i32 {
                    for dy in -1..=1i32 {
                        for dz in -1..=1i32 {
                            if dx == 0 && dy == 0 && dz == 0 {
                                continue;
                            }
                            let np = ChunkPosition::new(pos.x + dx, pos.y + dy, pos.z + dz);
                            if let Some(chunk) = self.world.get_chunk(np) {
                                neighbor_chunks.insert(np, chunk.clone());
                            } else if matches!(self.chunk_source, ChunkSource::Octree) {
                                // Read neighbor from octree
                                if let Some(chunk) = self.octree.as_ref().and_then(|o| {
                                    if let Some(data) = o.get_chunk_data(np) {
                                        data.to_chunk()
                                    } else if let Some(v) = o
                                        .get_node_homogeneous(&LodNodeKey::new(np.x, np.y, np.z, 0))
                                    {
                                        if v != 0 {
                                            let mut c = Chunk::new();
                                            c.fill_ground(voxel::CHUNK_SIZE, v);
                                            Some(c)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }) {
                                    neighbor_chunks.insert(np, chunk);
                                }
                            }
                        }
                    }
                }

                let chunk_source = self.chunk_source;
                self.streaming_inflight.insert(pos);
                let tx = tx.clone();
                rayon::spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Get chunk: from snapshot, generate procedurally, or air
                        let chunk = if let Some(c) = existing_chunk {
                            c
                        } else if matches!(chunk_source, ChunkSource::Procedural) && needs_terrain {
                            Self::generate_chunk_data_static(pos)
                        } else {
                            Chunk::new() // air fallback
                        };

                        // Build mini world with chunk + neighbors for meshing
                        let mut mini_world = ChunkManager::new();
                        mini_world.insert_chunk(pos, chunk.clone());
                        for (np, nc) in neighbor_chunks {
                            mini_world.insert_chunk(np, nc);
                        }

                        let vertices = generate_chunk_mesh(&mini_world, pos);
                        (vertices, if needs_terrain { Some(chunk) } else { None })
                    }));

                    let (vertices, new_chunk) = result.unwrap_or_else(|_| {
                        log::error!("[Streaming] Panic in mesh gen for chunk {:?}", pos);
                        (Vec::new(), None)
                    });
                    let _ = tx.send(StreamingMeshResult {
                        pos,
                        vertices,
                        new_chunk,
                    });
                });
            }
        }

        // 4. Process unload requests — remove mesh and chunk data (streamer regenerates on return)
        for pos in update.unload_requests {
            // In editor mode, flush dirty chunks back to octree before unloading
            if matches!(self.chunk_source, ChunkSource::Octree)
                && self.world.dirty_chunks().contains(&pos)
                && let Some(chunk) = self.world.get_chunk(pos)
                && let Some(ref mut octree) = self.octree
            {
                octree.insert_chunk(pos, world::lod::VoxelData::from_full(*chunk.data()));
            }
            self.chunk_meshes.remove(&MeshKey::Chunk(pos));
            self.streaming_inflight.remove(&pos);
            self.world.remove_chunk(pos);
        }

        // 5. Process dirty LOD nodes — rebuild octree LOD data, dispatch mesh gen to background.
        if let Some(ref mut octree) = self.octree {
            let regenerated = octree.process_dirty_lods();
            for key in regenerated {
                let is_tracked = self
                    .chunk_streamer
                    .as_ref()
                    .is_some_and(|s| s.is_lod_loaded(&key));
                if !is_tracked {
                    continue;
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
                    } else {
                        self.chunk_meshes.remove(&MeshKey::LodNode(key));
                    }
                }
            }
        }

        // 6. Process LOD node mesh requests from streamer — dispatch to background
        if let Some(ref tx) = self.lod_mesh_tx {
            for key in update.lod_mesh_requests {
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
                }
                // mark_lod_mesh_built will happen when result arrives in step 1b
            }
        }

        // 7. Process LOD node unload requests
        for key in update.lod_unload_requests {
            self.chunk_meshes.remove(&MeshKey::LodNode(key));
        }

        // 8. Update path tracer if chunks changed
        if !dirty_positions.is_empty() || has_unload_requests {
            if self.path_tracer.needs_resize(&self.world) {
                self.path_tracer.update_world_voxels(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    &self.world,
                );
            } else if !dirty_positions.is_empty() {
                self.path_tracer.update_chunks(
                    &self.render_ctx.queue,
                    &self.world,
                    &dirty_positions,
                );
            }
            self.path_tracer.reset_accumulation();
        }
    }
}

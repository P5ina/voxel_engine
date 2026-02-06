use std::sync::mpsc;

use glam::Vec3;
use rayon::prelude::*;

use crate::renderer::MeshResources;
use crate::ui::{LoadingState, UiScreen};
use crate::voxel::{self, generate_chunk_mesh, generate_octree_lod_mesh};
use crate::world::{
    ChunkManager, ChunkPosition, ChunkStreamer, LodNodeKey, RegionCoord, RegionManager,
    StreamingConfig, VoxelOctree,
};
use crate::{AppState, BigWorldGenMessage, BigWorldGenResult, ChunkSource, MeshKey};

impl AppState {
    /// Start generating a big world with loading screen
    pub(crate) fn start_big_world_generation(&mut self) {
        // Clear pending chunks queue
        self.pending_chunks.clear();
        self.pending_set.clear();

        // Disable streaming and octree (will be set up after loading)
        self.octree = None;
        self.chunk_streamer = None;
        self.use_streaming = false;
        self.region_manager = None;
        self.streaming_mesh_rx = None;
        self.streaming_mesh_tx = None;
        self.streaming_inflight.clear();
        self.lod_mesh_rx = None;
        self.lod_mesh_tx = None;

        // Clear existing meshes and world
        self.chunk_meshes.clear();
        self.world = ChunkManager::new();

        // Setup loading state and switch to loading screen immediately
        self.loading_state = LoadingState::new("Generating world...", 100);
        self.ui_screen = UiScreen::Loading;
        self.grab_mouse(false);

        // Spawn background thread for heavy generation work
        let (tx, rx) = mpsc::channel();
        self.big_world_gen_receiver = Some(rx);

        let level_name = self.editor_state.level_name.clone();
        std::thread::spawn(move || {
            Self::generate_big_world_background(tx, level_name);
        });

        log::info!("[BigWorld] Background generation thread started");
    }

    /// Heavy world generation work that runs on a background thread.
    /// Region-first: generates chunks and writes region files directly,
    /// never storing all chunk data in memory simultaneously.
    fn generate_big_world_background(tx: mpsc::Sender<BigWorldGenMessage>, level_name: String) {
        const WORLD_SIZE_CHUNKS: i32 = 512;
        const WORLD_SIZE_METERS: u32 = (WORLD_SIZE_CHUNKS as u32 * 32) / 16;

        // Spawn at center, compute terrain height for spawn Y
        let center = (WORLD_SIZE_CHUNKS as f32 * 32.0 * voxel::VOXEL_SCALE) / 2.0;
        let center_voxel_x = (center / voxel::VOXEL_SCALE) as i32;
        let center_voxel_z = (center / voxel::VOXEL_SCALE) as i32;
        let spawn_terrain_height = Self::terrain_height(center_voxel_x, center_voxel_z);
        let spawn_y = spawn_terrain_height as f32 * voxel::VOXEL_SCALE + 1.0;
        let spawn_pos = Vec3::new(center, spawn_y, center);

        log::info!(
            "[BigWorld] Generating {}x{} meter world ({}x{} chunks, terrain height at center: {} voxels = {:.1}m)...",
            WORLD_SIZE_METERS,
            WORLD_SIZE_METERS,
            WORLD_SIZE_CHUNKS,
            WORLD_SIZE_CHUNKS,
            spawn_terrain_height,
            spawn_terrain_height as f32 * voxel::VOXEL_SCALE
        );

        // Phase 1: Write region files directly (no octree, no bulk memory)
        let world_dir = std::path::PathBuf::from(format!("maps/{}", level_name));
        if let Err(e) = std::fs::create_dir_all(world_dir.join("regions")) {
            log::error!("[BigWorld] Failed to create regions directory: {}", e);
        }

        let region_size = crate::world::position::REGION_SIZE;
        let num_regions_x = (WORLD_SIZE_CHUNKS + region_size - 1) / region_size;
        let num_regions_z = (WORLD_SIZE_CHUNKS + region_size - 1) / region_size;
        let total_regions = (num_regions_x * num_regions_z) as usize;
        let mut regions_written = 0usize;
        let mut total_chunks_written = 0usize;

        let _ = tx.send(BigWorldGenMessage::Progress(
            "Generating terrain & writing regions...".into(),
            0,
            total_regions,
        ));

        for rrx in 0..num_regions_x {
            for rrz in 0..num_regions_z {
                let coord = RegionCoord::new(rrx, rrz);

                // Generate all non-empty chunks for this region in parallel
                let mut positions = Vec::new();
                for dx in 0..region_size {
                    for dz in 0..region_size {
                        let cx = rrx * region_size + dx;
                        let cz = rrz * region_size + dz;
                        if cx >= WORLD_SIZE_CHUNKS || cz >= WORLD_SIZE_CHUNKS {
                            continue;
                        }
                        for cy in 0..Self::WORLD_HEIGHT_CHUNKS {
                            let chunk_bottom_voxel = cy * voxel::CHUNK_SIZE as i32;
                            if chunk_bottom_voxel > Self::MAX_TERRAIN_VOXEL_HEIGHT || cy < 0 {
                                continue;
                            }
                            positions.push(ChunkPosition::new(cx, cy, cz));
                        }
                    }
                }

                let region_chunks: Vec<(ChunkPosition, voxel::Chunk)> = positions
                    .par_iter()
                    .filter_map(|&pos| {
                        let chunk = Self::generate_chunk_data_static(pos);
                        if chunk.is_empty() {
                            None
                        } else {
                            Some((pos, chunk))
                        }
                    })
                    .collect();

                if !region_chunks.is_empty() {
                    total_chunks_written += region_chunks.len();
                    let refs: Vec<(ChunkPosition, &voxel::Chunk)> =
                        region_chunks.iter().map(|(p, c)| (*p, c)).collect();
                    if let Err(e) = crate::world::region::write_region(&world_dir, coord, &refs) {
                        log::error!("[BigWorld] Failed to write region {:?}: {}", coord, e);
                    }
                }
                // region_chunks dropped here — memory freed

                regions_written += 1;
                if regions_written.is_multiple_of(20) || regions_written == total_regions {
                    let _ = tx.send(BigWorldGenMessage::Progress(
                        "Generating terrain & writing regions...".into(),
                        regions_written,
                        total_regions,
                    ));
                }
            }
        }
        log::info!(
            "[BigWorld] Wrote {} region files ({} non-empty chunks)",
            regions_written,
            total_chunks_written,
        );

        // Phase 2: Create minimal octree (empty, no chunk data) + save world.meta
        let _ = tx.send(BigWorldGenMessage::Progress(
            "Writing world.meta...".into(),
            0,
            1,
        ));
        let octree = VoxelOctree::for_world_size_meters(WORLD_SIZE_METERS);
        if let Err(e) =
            crate::world::region::save_world_meta(&world_dir, &octree, [center, spawn_y, center])
        {
            log::error!("[BigWorld] Failed to write world.meta: {}", e);
        }

        // Phase 3: Compute LOD mesh tasks and return
        let world = ChunkManager::new();
        let _ = tx.send(BigWorldGenMessage::Progress(
            "Computing mesh tasks...".into(),
            0,
            1,
        ));
        let (mesh_tasks, lod0_count) = Self::compute_mesh_tasks(&world, &octree, spawn_pos);

        let _ = tx.send(BigWorldGenMessage::Done(Box::new(BigWorldGenResult {
            world,
            octree,
            mesh_tasks,
            spawn_pos,
            lod0_count,
        })));
    }

    /// Compute LOD-aware mesh tasks from an existing octree, sorted by distance from spawn
    pub(crate) fn compute_mesh_tasks(
        world: &ChunkManager,
        _octree: &VoxelOctree,
        spawn_pos: Vec3,
    ) -> (Vec<MeshKey>, usize) {
        let all_positions: Vec<_> = world.chunk_positions().cloned().collect();

        let lod_config = crate::world::LodConfig::default();
        let chunk_world_size = voxel::CHUNK_SIZE as f32 * voxel::VOXEL_SCALE;
        let mut mesh_tasks: Vec<MeshKey> = Vec::new();

        let spawn_chunk = ChunkPosition::from_world_pos(spawn_pos.x, spawn_pos.y, spawn_pos.z);

        // LOD0: individual chunk meshes within LOD0 distance
        let lod0_distance = lod_config.distances[0];

        for pos in &all_positions {
            let chunk_center = pos.center_world_pos();
            let dist = Vec3::new(
                chunk_center.0 - spawn_pos.x,
                chunk_center.1 - spawn_pos.y,
                chunk_center.2 - spawn_pos.z,
            )
            .length();
            if dist <= lod0_distance {
                mesh_tasks.push(MeshKey::Chunk(*pos));
            }
        }

        let lod0_count = mesh_tasks.len();

        // LOD1-3: LOD node meshes for areas beyond LOD0
        // Iterate around spawn position rather than the full world
        for lod in 1..lod_config.levels {
            let lod_distance_min = lod_config.distances[(lod - 1) as usize];
            let lod_distance_max = lod_config.distances[lod as usize];

            let chunks_per_node = 1i32 << lod;
            let node_world_size = chunks_per_node as f32 * chunk_world_size;
            let max_node_radius = (lod_distance_max / node_world_size).ceil() as i32 + 1;

            let player_node_x = spawn_chunk.x >> lod;
            let player_node_y = spawn_chunk.y >> lod;
            let player_node_z = spawn_chunk.z >> lod;

            let vertical_range = (Self::WORLD_HEIGHT_CHUNKS >> lod).max(1);
            for dx in -max_node_radius..=max_node_radius {
                for dz in -max_node_radius..=max_node_radius {
                    for dy in -vertical_range..=vertical_range {
                        let key = LodNodeKey::new(
                            player_node_x + dx,
                            player_node_y + dy,
                            player_node_z + dz,
                            lod,
                        );
                        let center = key.center_world_pos();
                        let dist = Vec3::new(
                            center.0 - spawn_pos.x,
                            center.1 - spawn_pos.y,
                            center.2 - spawn_pos.z,
                        )
                        .length();

                        if dist >= lod_distance_min && dist < lod_distance_max {
                            mesh_tasks.push(MeshKey::LodNode(key));
                        }
                    }
                }
            }
        }

        // Sort by distance from spawn (closer first)
        mesh_tasks.sort_by(|a, b| {
            let dist_a = match a {
                MeshKey::Chunk(pos) => {
                    let c = pos.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
                MeshKey::LodNode(key) => {
                    let c = key.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
            };
            let dist_b = match b {
                MeshKey::Chunk(pos) => {
                    let c = pos.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
                MeshKey::LodNode(key) => {
                    let c = key.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
            };
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        log::info!(
            "[BigWorld] {} mesh tasks ({} LOD0 chunks + {} LOD nodes)",
            mesh_tasks.len(),
            lod0_count,
            mesh_tasks.len() - lod0_count,
        );

        (mesh_tasks, lod0_count)
    }

    /// Update loading screen - process LOD mesh tasks incrementally
    pub(crate) fn update_loading(&mut self) {
        const TASKS_PER_FRAME: usize = 64;

        // Check for background save completion
        if let Some(ref rx) = self.save_world_receiver {
            match rx.try_recv() {
                Ok((octree, result)) => {
                    match result {
                        Ok(()) => log::info!("[BigWorld] Save completed successfully"),
                        Err(e) => log::error!("[BigWorld] Save failed: {}", e),
                    }
                    if let Some(octree) = octree {
                        self.octree = Some(octree);
                    }
                    self.save_world_receiver = None;
                    self.ui_screen = self.prev_screen;
                    self.grab_mouse(true);
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still saving
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::error!("[BigWorld] Save thread disconnected!");
                    self.save_world_receiver = None;
                    self.ui_screen = self.prev_screen;
                    self.grab_mouse(true);
                    return;
                }
            }
        }

        // Phase 1: Check for messages from background generation thread
        if let Some(ref rx) = self.big_world_gen_receiver {
            // Drain all pending messages this frame
            loop {
                match rx.try_recv() {
                    Ok(BigWorldGenMessage::Progress(msg, loaded, total)) => {
                        self.loading_state.message = msg;
                        self.loading_state.chunks_total = total;
                        self.loading_state.update(loaded);
                    }
                    Ok(BigWorldGenMessage::Done(boxed_result)) => {
                        let result = *boxed_result;
                        // Background generation complete — transfer results
                        log::info!(
                            "[BigWorld] Background generation received. Loading {} mesh tasks ({} LOD0 + {} LOD)...",
                            result.mesh_tasks.len(),
                            result.lod0_count,
                            result.mesh_tasks.len() - result.lod0_count,
                        );
                        let total_tasks = result.mesh_tasks.len();
                        self.world = result.world;
                        self.octree = Some(result.octree);
                        self.big_world_lod_tasks = Some(result.mesh_tasks);
                        self.player.position = result.spawn_pos;
                        self.player.velocity = Vec3::ZERO;
                        self.camera.position = self.player.eye_position();
                        self.loading_state = LoadingState::new("Building meshes...", total_tasks);
                        self.big_world_gen_receiver = None;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // Still generating — keep showing loading screen
                        return;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Thread panicked or dropped sender
                        log::error!("[BigWorld] Generation thread disconnected!");
                        self.big_world_gen_receiver = None;
                        self.ui_screen = UiScreen::MainMenu;
                        return;
                    }
                }
            }
        }

        // Phase 2: Process mesh tasks
        // Take ownership of the tasks to process
        let tasks = match self.big_world_lod_tasks.take() {
            Some(tasks) => tasks,
            None => {
                // Already finished — shouldn't happen, but safe fallback
                self.finish_big_world_loading();
                return;
            }
        };

        if tasks.is_empty() {
            self.finish_big_world_loading();
            return;
        }

        // Process a batch of tasks this frame
        let to_process = tasks.len().min(TASKS_PER_FRAME);
        let (tasks_to_process, remaining): (Vec<_>, Vec<_>) = tasks
            .into_iter()
            .enumerate()
            .partition(|(i, _)| *i < to_process);

        let tasks_to_process: Vec<_> = tasks_to_process.into_iter().map(|(_, t)| t).collect();
        let remaining: Vec<_> = remaining.into_iter().map(|(_, t)| t).collect();

        // Generate meshes based on task type
        for task in &tasks_to_process {
            match task {
                MeshKey::Chunk(pos) => {
                    let vertices = generate_chunk_mesh(&self.world, *pos);
                    if !vertices.is_empty() {
                        self.chunk_meshes.insert(
                            MeshKey::Chunk(*pos),
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                }
                MeshKey::LodNode(key) => {
                    let vertices = if let Some(ref octree) = self.octree {
                        if let Some(data) = octree.get_node_lod_data(key) {
                            generate_octree_lod_mesh(data, key)
                        } else if let Some(v) = octree.get_node_homogeneous(key) {
                            if v != voxel::AIR {
                                let data = crate::world::lod::VoxelData::Homogeneous(v);
                                generate_octree_lod_mesh(&data, key)
                            } else {
                                Vec::new()
                            }
                        } else if let Some(data) = Self::generate_lod_data_static(key) {
                            generate_octree_lod_mesh(&data, key)
                        } else {
                            Vec::new()
                        }
                    } else if let Some(data) = Self::generate_lod_data_static(key) {
                        generate_octree_lod_mesh(&data, key)
                    } else {
                        Vec::new()
                    };

                    if !vertices.is_empty() {
                        self.chunk_meshes.insert(
                            MeshKey::LodNode(*key),
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                }
            }
        }

        // Update loading progress
        let loaded = self.loading_state.chunks_total - remaining.len();
        self.loading_state.update(loaded);

        // Put remaining tasks back, or finish loading if done
        if !remaining.is_empty() {
            self.big_world_lod_tasks = Some(remaining);
        } else {
            self.finish_big_world_loading();
        }
    }

    /// Finish big world loading: set up path tracer, streaming, and switch to game
    pub(crate) fn finish_big_world_loading(&mut self) {
        log::info!("[BigWorld] Initial loading complete! Setting up streaming...");

        // Update path tracer
        self.path_tracer.update_world_voxels(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &self.world,
        );
        self.path_tracer.reset_accumulation();

        // Create streaming system with world bounds
        let mut config = StreamingConfig::default();
        if let Some(ref octree) = self.octree {
            // Use octree bounds for X/Z, constrain Y to terrain range
            let has_bounds = octree.world_max.x > octree.world_min.x;
            if has_bounds {
                config.world_min = glam::IVec3::new(octree.world_min.x, 0, octree.world_min.z);
                config.world_max = glam::IVec3::new(
                    octree.world_max.x + 1,    // exclusive
                    Self::WORLD_HEIGHT_CHUNKS, // Y layers for terrain
                    octree.world_max.z + 1,
                );
            } else {
                // Empty octree (region-first gen) — use world constants
                config.world_min = glam::IVec3::new(0, 0, 0);
                config.world_max = glam::IVec3::new(512, Self::WORLD_HEIGHT_CHUNKS, 512);
            }
        } else {
            config.world_min = glam::IVec3::new(0, 0, 0);
            config.world_max = glam::IVec3::new(512, Self::WORLD_HEIGHT_CHUNKS, 512);
        }
        let mut streamer = ChunkStreamer::new(config);
        streamer.set_player_position(self.player.position);

        // Preload streamer with all currently loaded meshes
        let mut loaded_chunks = Vec::new();
        let mut loaded_lod_nodes = Vec::new();
        for key in self.chunk_meshes.keys() {
            match key {
                MeshKey::Chunk(pos) => loaded_chunks.push(*pos),
                MeshKey::LodNode(key) => loaded_lod_nodes.push(*key),
            }
        }

        streamer.preload_chunks(loaded_chunks);
        streamer.preload_lod_nodes(loaded_lod_nodes);

        self.chunk_streamer = Some(streamer);
        self.use_streaming = true;

        // Set up background mesh generation channels
        let (tx, rx) = mpsc::channel();
        self.streaming_mesh_tx = Some(tx);
        self.streaming_mesh_rx = Some(rx);
        self.streaming_inflight.clear();
        let (lod_tx, lod_rx) = mpsc::channel();
        self.lod_mesh_tx = Some(lod_tx);
        self.lod_mesh_rx = Some(lod_rx);
        self.chunks_ready = false; // freeze player until nearby chunks are meshed

        log::info!(
            "[BigWorld] Streaming enabled! {} chunk meshes, {} LOD meshes",
            self.chunk_meshes
                .keys()
                .filter(|k| matches!(k, MeshKey::Chunk(_)))
                .count(),
            self.chunk_meshes
                .keys()
                .filter(|k| matches!(k, MeshKey::LodNode(_)))
                .count(),
        );

        // Set chunk source based on mode
        self.chunk_source = if self.enter_editor_after_gen {
            ChunkSource::Octree
        } else {
            ChunkSource::Procedural
        };

        // Set up region manager if world directory exists
        let world_dir = std::path::PathBuf::from(format!("maps/{}", self.editor_state.level_name));
        if world_dir.join("world.meta").exists() {
            let mut region_mgr = RegionManager::new(world_dir);
            region_mgr.mark_existing_regions_loaded(&self.world);
            self.region_manager = Some(region_mgr);
            log::info!("[BigWorld] Region manager initialized");
        }

        // Seed the streamer: run one update so mesh requests are dispatched immediately
        // on the first frame instead of waiting one frame for the pipeline to start.
        self.update_streaming();

        if self.enter_editor_after_gen {
            self.ui_screen = UiScreen::Editor;
        } else {
            self.ui_screen = UiScreen::InGame;
        }
        self.grab_mouse(true);
    }
}

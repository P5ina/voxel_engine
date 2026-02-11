#![cfg_attr(not(feature = "dev-tools"), allow(dead_code))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use glam::Vec3;
use rayon::prelude::*;

use crate::renderer::MeshResources;
use crate::ui::{LoadingState, UiScreen};
use crate::voxel::{self, generate_chunk_mesh, generate_octree_lod_mesh};
use crate::world::{
    ChunkManager, ChunkPosition, ChunkStreamer, ColumnPos, RegionCoord, RegionManager,
    StreamingConfig, VoxelOctree,
};
use crate::{AppState, BigWorldGenMessage, BigWorldGenResult, MeshKey};

impl AppState {
    pub(crate) const WORLD_SIZE_CHUNKS: i32 = 64;
    pub(crate) const WORLD_SIZE_METERS: u32 = 1024;

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
        let world_size_chunks = Self::WORLD_SIZE_CHUNKS;
        let world_size_meters = Self::WORLD_SIZE_METERS;

        // Spawn at center, compute terrain height for spawn Y
        let center =
            (world_size_chunks as f32 * voxel::CHUNK_SIZE as f32 * voxel::VOXEL_SCALE) / 2.0;
        let center_voxel_x = (center / voxel::VOXEL_SCALE) as i32;
        let center_voxel_z = (center / voxel::VOXEL_SCALE) as i32;
        let spawn_terrain_height = Self::terrain_height(center_voxel_x, center_voxel_z);
        let spawn_y = spawn_terrain_height as f32 * voxel::VOXEL_SCALE + 1.0;
        let spawn_pos = Vec3::new(center, spawn_y, center);

        log::info!(
            "[BigWorld] Generating {}x{} meter world ({}x{} chunks, terrain height at center: {} voxels = {:.1}m)...",
            world_size_meters,
            world_size_meters,
            world_size_chunks,
            world_size_chunks,
            spawn_terrain_height,
            spawn_terrain_height as f32 * voxel::VOXEL_SCALE
        );

        // Phase 1: Write region files directly (no octree, no bulk memory)
        let world_dir = std::path::PathBuf::from(format!("maps/{}", level_name));
        if let Err(e) = std::fs::create_dir_all(world_dir.join("regions")) {
            log::error!("[BigWorld] Failed to create regions directory: {}", e);
        }

        let region_size = crate::world::position::REGION_SIZE;
        let num_regions_x = (world_size_chunks + region_size - 1) / region_size;
        let num_regions_z = (world_size_chunks + region_size - 1) / region_size;
        let total_regions = (num_regions_x * num_regions_z) as usize;
        let regions_written = AtomicUsize::new(0);
        let total_chunks_written = AtomicUsize::new(0);

        let _ = tx.send(BigWorldGenMessage::Progress(
            "Generating terrain & writing regions...".into(),
            0,
            total_regions,
        ));

        let mut region_coords = Vec::with_capacity(total_regions);
        for rrx in 0..num_regions_x {
            for rrz in 0..num_regions_z {
                region_coords.push(RegionCoord::new(rrx, rrz));
            }
        }

        region_coords.into_par_iter().for_each(|coord| {
            let tx = tx.clone();

            // Collect column XZ positions in this region
            let mut col_positions = Vec::new();
            for dx in 0..region_size {
                for dz in 0..region_size {
                    let cx = coord.rx * region_size + dx;
                    let cz = coord.rz * region_size + dz;
                    if cx >= world_size_chunks || cz >= world_size_chunks {
                        continue;
                    }
                    col_positions.push(ColumnPos::new(cx, cz));
                }
            }

            // Generate columns and flatten to (ChunkPosition, Chunk) pairs
            let mut region_chunks: Vec<(ChunkPosition, voxel::Chunk)> = Vec::new();
            for col in col_positions {
                let column = Self::generate_column_data_static(col);
                for (sy, section) in column.sections_iter() {
                    region_chunks.push((col.to_chunk_pos(sy), section.clone()));
                }
            }

            if !region_chunks.is_empty() {
                total_chunks_written.fetch_add(region_chunks.len(), Ordering::Relaxed);
                let refs: Vec<(ChunkPosition, &voxel::Chunk)> =
                    region_chunks.iter().map(|(p, c)| (*p, c)).collect();
                if let Err(e) = crate::world::region::write_region(&world_dir, coord, &refs) {
                    log::error!("[BigWorld] Failed to write region {:?}: {}", coord, e);
                }
            }

            let written = regions_written.fetch_add(1, Ordering::Relaxed) + 1;
            if written.is_multiple_of(20) || written == total_regions {
                let _ = tx.send(BigWorldGenMessage::Progress(
                    "Generating terrain & writing regions...".into(),
                    written,
                    total_regions,
                ));
            }
        });
        log::info!(
            "[BigWorld] Wrote {} region files ({} non-empty chunks)",
            regions_written.load(Ordering::Relaxed),
            total_chunks_written.load(Ordering::Relaxed),
        );

        // Phase 2: Create minimal octree (empty, no chunk data) + save world.meta
        let _ = tx.send(BigWorldGenMessage::Progress(
            "Writing world.meta...".into(),
            0,
            1,
        ));
        let octree = VoxelOctree::for_world_size_meters(Self::WORLD_SIZE_METERS);
        if let Err(e) =
            crate::world::region::save_world_meta(&world_dir, &octree, [center, spawn_y, center])
        {
            log::error!("[BigWorld] Failed to write world.meta: {}", e);
        }

        // Phase 3: Load all regions back into memory
        let regions_dir = world_dir.join("regions");
        let mut region_files: Vec<(RegionCoord, std::path::PathBuf)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&regions_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname_str = fname.to_string_lossy();
                if let Some(rest) = fname_str.strip_prefix("r_") {
                    if let Some(rest) = rest.strip_suffix(".region") {
                        let parts: Vec<&str> = rest.splitn(2, '_').collect();
                        if parts.len() == 2 {
                            if let (Ok(rx), Ok(rz)) =
                                (parts[0].parse::<i32>(), parts[1].parse::<i32>())
                            {
                                region_files.push((RegionCoord::new(rx, rz), entry.path()));
                            }
                        }
                    }
                }
            }
        }

        let total_to_load = region_files.len();
        let _ = tx.send(BigWorldGenMessage::Progress(
            "Loading regions into memory...".into(),
            0,
            total_to_load,
        ));

        let mut world = ChunkManager::new();
        let mut loaded_chunks = 0usize;
        for (i, (coord, _path)) in region_files.iter().enumerate() {
            match crate::world::region::read_region(&world_dir, *coord) {
                Ok(chunks) => {
                    loaded_chunks += chunks.len();
                    for (pos, chunk) in chunks {
                        if !chunk.is_empty() {
                            world.insert_chunk(pos, chunk);
                        }
                    }
                }
                Err(e) => {
                    log::error!("[BigWorld] Failed to read region {:?}: {}", coord, e);
                }
            }
            if (i + 1) % 4 == 0 || i + 1 == total_to_load {
                let _ = tx.send(BigWorldGenMessage::Progress(
                    "Loading regions into memory...".into(),
                    i + 1,
                    total_to_load,
                ));
            }
        }
        log::info!(
            "[BigWorld] Loaded {} regions ({} chunks) into memory",
            total_to_load,
            loaded_chunks,
        );

        // Phase 4: Compute mesh tasks and return
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

    /// Compute mesh tasks: LOD0 for all loaded chunks (world is fully preloaded).
    /// No LOD node tasks — LOD0 covers the entire world.
    pub(crate) fn compute_mesh_tasks(
        world: &ChunkManager,
        _octree: &VoxelOctree,
        spawn_pos: Vec3,
    ) -> (Vec<MeshKey>, usize) {
        use crate::world::LodNodeKey;

        let all_positions: Vec<_> = world.chunk_positions().collect();
        let mut mesh_tasks: Vec<MeshKey> = Vec::with_capacity(all_positions.len());

        for pos in &all_positions {
            mesh_tasks.push(MeshKey::Chunk(*pos));
        }

        let lod0_count = mesh_tasks.len();

        // Sort LOD0 by distance from spawn (closer first)
        mesh_tasks.sort_by(|a, b| {
            let dist = |key: &MeshKey| {
                let MeshKey::Chunk(pos) = key else {
                    unreachable!()
                };
                let c = pos.center_world_pos();
                Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
            };
            dist(a)
                .partial_cmp(&dist(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Generate LOD node mesh tasks for levels 1..6
        let world_x = Self::WORLD_SIZE_CHUNKS;
        let world_y = Self::WORLD_HEIGHT_CHUNKS;
        let world_z = Self::WORLD_SIZE_CHUNKS;
        let mut lod_tasks: Vec<MeshKey> = Vec::new();

        for lod in 1..6u8 {
            let chunks_per_node = 1i32 << lod;
            let nx = (world_x + chunks_per_node - 1) / chunks_per_node;
            let ny = (world_y + chunks_per_node - 1) / chunks_per_node;
            let nz = (world_z + chunks_per_node - 1) / chunks_per_node;
            for x in 0..nx {
                for y in 0..ny {
                    for z in 0..nz {
                        lod_tasks.push(MeshKey::LodNode(LodNodeKey::new(x, y, z, lod)));
                    }
                }
            }
        }

        // Sort LOD tasks: lower levels first, then by distance
        lod_tasks.sort_by(|a, b| {
            let key = |m: &MeshKey| {
                let MeshKey::LodNode(k) = m else {
                    unreachable!()
                };
                let c = k.center_world_pos();
                let dist =
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length();
                (k.lod_level, dist as i64)
            };
            key(a).cmp(&key(b))
        });

        log::info!(
            "[BigWorld] {} mesh tasks ({} LOD0 chunks + {} LOD nodes)",
            lod0_count + lod_tasks.len(),
            lod0_count,
            lod_tasks.len(),
        );

        mesh_tasks.extend(lod_tasks);
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

        // Generate meshes based on task type — build ALL meshes (no caps)
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
                config.world_max = glam::IVec3::new(
                    Self::WORLD_SIZE_CHUNKS,
                    Self::WORLD_HEIGHT_CHUNKS,
                    Self::WORLD_SIZE_CHUNKS,
                );
            }
        } else {
            config.world_min = glam::IVec3::new(0, 0, 0);
            config.world_max = glam::IVec3::new(
                Self::WORLD_SIZE_CHUNKS,
                Self::WORLD_HEIGHT_CHUNKS,
                Self::WORLD_SIZE_CHUNKS,
            );
        }

        // Clamp to configured playable bounds so stale metadata from older scales
        // cannot expand streaming/LOD far beyond the intended world.
        let clamp_min = glam::IVec3::new(0, 0, 0);
        let clamp_max = glam::IVec3::new(
            Self::WORLD_SIZE_CHUNKS,
            Self::WORLD_HEIGHT_CHUNKS,
            Self::WORLD_SIZE_CHUNKS,
        );
        config.world_min = glam::IVec3::new(
            config.world_min.x.max(clamp_min.x),
            config.world_min.y.max(clamp_min.y),
            config.world_min.z.max(clamp_min.z),
        );
        config.world_max = glam::IVec3::new(
            config.world_max.x.min(clamp_max.x),
            config.world_max.y.min(clamp_max.y),
            config.world_max.z.min(clamp_max.z),
        );

        self.path_tracer.reset_accumulation();
        let mut streamer = ChunkStreamer::new(config);
        streamer.set_player_position(self.player.position);

        // Cull LOD0 meshes outside the LOD0 streaming distance so we don't
        // start the game with 100K+ mesh entries to iterate every frame.
        let lod0_distance = streamer.lod0_distance();
        let chunk_world_size = voxel::CHUNK_SIZE as f32 * voxel::VOXEL_SCALE;
        let player_pos = self.player.position;
        let mut loaded_chunks = Vec::new();
        let mut loaded_lod_nodes = Vec::new();
        let mut to_remove: Vec<MeshKey> = Vec::new();

        for key in self.chunk_meshes.keys() {
            match key {
                MeshKey::Chunk(pos) => {
                    // XZ distance from chunk center to player
                    let cx = (pos.x as f32 + 0.5) * chunk_world_size;
                    let cz = (pos.z as f32 + 0.5) * chunk_world_size;
                    let dx = cx - player_pos.x;
                    let dz = cz - player_pos.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist <= lod0_distance {
                        loaded_chunks.push(*pos);
                    } else {
                        to_remove.push(*key);
                    }
                }
                MeshKey::LodNode(key) => loaded_lod_nodes.push(*key),
            }
        }

        let removed_count = to_remove.len();
        for key in to_remove {
            self.chunk_meshes.remove(&key);
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
            "[BigWorld] Streaming enabled! {} chunk meshes, {} LOD meshes ({} distant LOD0 culled)",
            self.chunk_meshes
                .keys()
                .filter(|k| matches!(k, MeshKey::Chunk(_)))
                .count(),
            self.chunk_meshes
                .keys()
                .filter(|k| matches!(k, MeshKey::LodNode(_)))
                .count(),
            removed_count,
        );

        // Set up region manager if world directory exists
        let world_dir = std::path::PathBuf::from(format!("maps/{}", self.editor_state.level_name));
        if world_dir.join("world.meta").exists() {
            let mut region_mgr = RegionManager::new(world_dir);
            // Mark ALL potential region coordinates as loaded (world is fully preloaded)
            // This prevents spurious background region load requests for empty regions.
            let num_regions = (Self::WORLD_SIZE_CHUNKS + crate::world::position::REGION_SIZE - 1)
                / crate::world::position::REGION_SIZE;
            for rx in 0..num_regions {
                for rz in 0..num_regions {
                    region_mgr.mark_loaded(RegionCoord::new(rx, rz));
                }
            }
            self.region_manager = Some(region_mgr);
            log::info!("[BigWorld] Region manager initialized (all regions marked loaded)");
        }

        // Seed the streamer: run one update so mesh requests are dispatched immediately
        // on the first frame instead of waiting one frame for the pipeline to start.
        self.update_streaming();

        #[cfg(feature = "dev-tools")]
        {
            if self.enter_editor_after_gen {
                self.ui_screen = UiScreen::Editor;
            } else {
                self.ui_screen = UiScreen::InGame;
            }
        }
        #[cfg(not(feature = "dev-tools"))]
        {
            self.ui_screen = UiScreen::InGame;
        }
        self.grab_mouse(true);
    }
}

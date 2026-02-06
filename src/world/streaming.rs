use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use glam::Vec3;

use super::ChunkPosition;
use super::lod::{LodConfig, LodLevel};
use super::position::LodNodeKey;
use crate::voxel::VOXEL_SCALE;

/// Streaming configuration
#[derive(Clone, Debug)]
pub struct StreamingConfig {
    /// LOD configuration
    pub lod: LodConfig,

    /// Maximum chunks to load per frame
    pub max_loads_per_frame: usize,

    /// Maximum chunks to unload per frame
    pub max_unloads_per_frame: usize,

    /// Maximum mesh rebuilds per frame
    pub max_mesh_rebuilds_per_frame: usize,

    /// Hysteresis factor to prevent thrashing (1.1 = 10% buffer)
    pub hysteresis: f32,

    /// Vertical range (chunks above/below player to load)
    pub vertical_range: i32,

    /// World bounds in chunk coordinates (min inclusive, max exclusive)
    /// Only generate chunks within these bounds
    pub world_min: glam::IVec3,
    pub world_max: glam::IVec3,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            lod: LodConfig::default(),
            max_loads_per_frame: 32,
            max_unloads_per_frame: 32,
            max_mesh_rebuilds_per_frame: 16,
            hysteresis: 1.15,
            vertical_range: 8,
            world_min: glam::IVec3::new(i32::MIN, i32::MIN, i32::MIN),
            world_max: glam::IVec3::new(i32::MAX, i32::MAX, i32::MAX),
        }
    }
}

/// Priority for chunk operations (lower = higher priority)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ChunkPriority(u32);

impl ChunkPriority {
    pub fn from_distance_and_lod(distance: f32, lod: LodLevel, is_visible: bool) -> Self {
        let base = (distance * 10.0) as u32;
        let lod_penalty = (lod as u32) * 1000;
        let visibility_penalty = if is_visible { 0 } else { 500 };
        Self(base + lod_penalty + visibility_penalty)
    }
}

/// Request to load a chunk at specific LOD
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChunkLoadRequest {
    pub position: ChunkPosition,
    pub lod_level: LodLevel,
    pub priority: ChunkPriority,
}

impl PartialOrd for ChunkLoadRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChunkLoadRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

/// State of a loaded chunk
#[derive(Clone, Copy, Debug)]
pub struct LoadedChunkState {
    pub position: ChunkPosition,
    pub lod_level: LodLevel,
    pub has_mesh: bool,
}

/// State of a loaded LOD node
#[derive(Clone, Debug)]
pub struct LoadedLodNodeState {
    pub key: LodNodeKey,
    pub has_mesh: bool,
}

/// Result of streaming update
#[derive(Default)]
pub struct StreamingUpdate {
    /// Chunks to generate/regenerate mesh for
    pub mesh_requests: Vec<(ChunkPosition, LodLevel)>,

    /// Chunks to unload
    pub unload_requests: Vec<ChunkPosition>,

    /// Chunks that need LOD data generated
    pub lod_requests: Vec<(ChunkPosition, LodLevel)>,

    /// LOD nodes to generate meshes for
    pub lod_mesh_requests: Vec<LodNodeKey>,

    /// LOD nodes to unload
    pub lod_unload_requests: Vec<LodNodeKey>,
}

/// Chunk streaming system
pub struct ChunkStreamer {
    /// Priority queue for loading (min-heap using Reverse)
    load_queue: BinaryHeap<Reverse<ChunkLoadRequest>>,

    /// Set of chunks currently in queue (for dedup)
    queued: HashSet<(ChunkPosition, LodLevel)>,

    /// Currently loaded chunks with their state
    loaded: Vec<LoadedChunkState>,

    /// Set of loaded chunk positions for fast lookup
    loaded_set: HashSet<ChunkPosition>,

    /// Currently loaded LOD nodes
    loaded_lod_nodes: Vec<LoadedLodNodeState>,

    /// Set of loaded LOD node keys for fast lookup
    loaded_lod_set: HashSet<LodNodeKey>,

    /// Last known player position
    last_player_pos: Vec3,

    /// Configuration
    config: StreamingConfig,
}

impl ChunkStreamer {
    pub fn new(config: StreamingConfig) -> Self {
        Self {
            load_queue: BinaryHeap::new(),
            queued: HashSet::new(),
            loaded: Vec::new(),
            loaded_set: HashSet::new(),
            loaded_lod_nodes: Vec::new(),
            loaded_lod_set: HashSet::new(),
            last_player_pos: Vec3::ZERO,
            config,
        }
    }

    /// Update streaming based on player position
    /// Returns requests for mesh generation and chunk unloading
    pub fn update(&mut self, player_pos: Vec3) -> StreamingUpdate {
        self.last_player_pos = player_pos;

        let mut result = StreamingUpdate::default();

        // 1. Calculate which chunks should be loaded at which LOD
        let desired = self.calculate_desired_chunks(player_pos);

        // 2. Queue chunks that need to be loaded
        for (pos, desired_lod) in &desired {
            if !self.loaded_set.contains(pos) {
                // Need to load this chunk
                if !self.queued.contains(&(*pos, *desired_lod)) {
                    let distance = self.chunk_distance(*pos, player_pos);
                    let priority =
                        ChunkPriority::from_distance_and_lod(distance, *desired_lod, true);

                    self.load_queue.push(Reverse(ChunkLoadRequest {
                        position: *pos,
                        lod_level: *desired_lod,
                        priority,
                    }));
                    self.queued.insert((*pos, *desired_lod));
                }
            } else {
                // Check if LOD needs to change
                if let Some(state) = self.loaded.iter().find(|s| s.position == *pos)
                    && state.lod_level != *desired_lod
                {
                    // LOD change needed
                    result.lod_requests.push((*pos, *desired_lod));
                }
            }
        }

        // 3. Process load queue
        let mut loads_this_frame = 0;
        while loads_this_frame < self.config.max_loads_per_frame {
            if let Some(Reverse(request)) = self.load_queue.pop() {
                self.queued.remove(&(request.position, request.lod_level));

                // Skip if already loaded or no longer needed
                if self.loaded_set.contains(&request.position) {
                    continue;
                }
                if !desired.contains_key(&request.position) {
                    continue;
                }

                // Mark as loaded
                self.loaded.push(LoadedChunkState {
                    position: request.position,
                    lod_level: request.lod_level,
                    has_mesh: false,
                });
                self.loaded_set.insert(request.position);

                // Request mesh generation
                result
                    .mesh_requests
                    .push((request.position, request.lod_level));

                loads_this_frame += 1;
            } else {
                break;
            }
        }

        // 4. Find chunks to unload
        let mut unloads_this_frame = 0;
        let mut to_remove = Vec::new();

        for (i, state) in self.loaded.iter().enumerate() {
            if unloads_this_frame >= self.config.max_unloads_per_frame {
                break;
            }

            let should_be_loaded = desired.contains_key(&state.position);
            let distance = self.chunk_distance(state.position, player_pos);
            let max_distance = self.config.lod.distances[self.config.lod.levels as usize - 1];

            // Unload if too far with hysteresis
            if !should_be_loaded || distance > max_distance * self.config.hysteresis {
                to_remove.push(i);
                result.unload_requests.push(state.position);
                unloads_this_frame += 1;
            }
        }

        // Remove unloaded chunks (reverse order to preserve indices)
        for i in to_remove.into_iter().rev() {
            let state = self.loaded.swap_remove(i);
            self.loaded_set.remove(&state.position);
        }

        // 5. Request mesh updates for loaded chunks that need it (independent budget)
        let mut retry_count = 0;
        let max_retries = self.config.max_mesh_rebuilds_per_frame;
        for state in &self.loaded {
            if retry_count >= max_retries {
                break;
            }
            if !state.has_mesh {
                result.mesh_requests.push((state.position, state.lod_level));
                retry_count += 1;
            }
        }

        // 6. LOD node management
        let desired_lod_nodes = self.calculate_desired_lod_nodes(player_pos);

        // Queue new LOD nodes
        for key in &desired_lod_nodes {
            if !self.loaded_lod_set.contains(key) {
                self.loaded_lod_nodes.push(LoadedLodNodeState {
                    key: *key,
                    has_mesh: false,
                });
                self.loaded_lod_set.insert(*key);
                result.lod_mesh_requests.push(*key);
            }
        }

        // Unload LOD nodes that are no longer needed
        let mut lod_to_remove = Vec::new();
        for (i, state) in self.loaded_lod_nodes.iter().enumerate() {
            if !desired_lod_nodes.contains(&state.key) {
                lod_to_remove.push(i);
                result.lod_unload_requests.push(state.key);
            }
        }
        for i in lod_to_remove.into_iter().rev() {
            let state = self.loaded_lod_nodes.swap_remove(i);
            self.loaded_lod_set.remove(&state.key);
        }

        result
    }

    /// Calculate which individual chunks should be loaded (LOD0 only).
    /// Higher LOD levels are handled by LOD nodes, not individual chunks.
    fn calculate_desired_chunks(
        &self,
        player_pos: Vec3,
    ) -> std::collections::HashMap<ChunkPosition, LodLevel> {
        let mut result = std::collections::HashMap::new();

        let player_chunk = ChunkPosition::from_world_pos(player_pos.x, player_pos.y, player_pos.z);

        // Only load individual chunks within LOD0 distance
        let lod0_distance = self.config.lod.distances[0];
        let chunk_world_size = crate::voxel::CHUNK_SIZE as f32 * VOXEL_SCALE;
        let max_chunk_radius = (lod0_distance / chunk_world_size).ceil() as i32 + 1;

        for dx in -max_chunk_radius..=max_chunk_radius {
            for dz in -max_chunk_radius..=max_chunk_radius {
                for dy in -self.config.vertical_range..=self.config.vertical_range {
                    let pos = ChunkPosition::new(
                        player_chunk.x + dx,
                        player_chunk.y + dy,
                        player_chunk.z + dz,
                    );

                    // Skip chunks outside world bounds
                    if pos.x < self.config.world_min.x
                        || pos.x >= self.config.world_max.x
                        || pos.y < self.config.world_min.y
                        || pos.y >= self.config.world_max.y
                        || pos.z < self.config.world_min.z
                        || pos.z >= self.config.world_max.z
                    {
                        continue;
                    }

                    let distance = self.chunk_distance(pos, player_pos);

                    if distance > lod0_distance {
                        continue;
                    }

                    result.insert(pos, 0); // Always LOD 0
                }
            }
        }

        result
    }

    /// Calculate distance from chunk center to player
    fn chunk_distance(&self, pos: ChunkPosition, player_pos: Vec3) -> f32 {
        let chunk_center = pos.center_world_pos();
        let diff = Vec3::new(
            chunk_center.0 - player_pos.x,
            chunk_center.1 - player_pos.y,
            chunk_center.2 - player_pos.z,
        );
        diff.length()
    }

    /// Mark a chunk as having its mesh built
    pub fn mark_mesh_built(&mut self, pos: ChunkPosition) {
        if let Some(state) = self.loaded.iter_mut().find(|s| s.position == pos) {
            state.has_mesh = true;
        }
    }

    /// Mark a chunk as dirty (needs mesh rebuild)
    pub fn mark_dirty(&mut self, pos: ChunkPosition) {
        if let Some(state) = self.loaded.iter_mut().find(|s| s.position == pos) {
            state.has_mesh = false;
        }
    }

    /// Get loaded chunks count
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Get queue size
    pub fn queue_size(&self) -> usize {
        self.load_queue.len()
    }

    /// Get loaded LOD nodes count
    pub fn loaded_lod_count(&self) -> usize {
        self.loaded_lod_nodes.len()
    }

    /// Check if a chunk is loaded
    pub fn is_loaded(&self, pos: ChunkPosition) -> bool {
        self.loaded_set.contains(&pos)
    }

    /// Check if a chunk is loaded AND has its mesh built
    pub fn has_mesh(&self, pos: ChunkPosition) -> bool {
        self.loaded.iter().any(|s| s.position == pos && s.has_mesh)
    }

    /// Mark a LOD node as having its mesh built
    pub fn mark_lod_mesh_built(&mut self, key: &LodNodeKey) {
        if let Some(state) = self.loaded_lod_nodes.iter_mut().find(|s| s.key == *key) {
            state.has_mesh = true;
        }
    }

    /// Check if a LOD node is loaded
    pub fn is_lod_loaded(&self, key: &LodNodeKey) -> bool {
        self.loaded_lod_set.contains(key)
    }

    /// Calculate which LOD nodes should be loaded.
    /// For LOD 1+, we group chunks into LodNodeKeys (deduplicated).
    fn calculate_desired_lod_nodes(&self, player_pos: Vec3) -> HashSet<LodNodeKey> {
        let mut result = HashSet::new();

        let player_chunk = ChunkPosition::from_world_pos(player_pos.x, player_pos.y, player_pos.z);

        // For each LOD level > 0, calculate which LOD nodes should be loaded
        for lod in 1..self.config.lod.levels {
            let lod_distance_min = self.config.lod.distances[(lod - 1) as usize];
            let lod_distance_max = self.config.lod.distances[lod as usize];

            // Each LOD node at level L covers 2^L chunks per axis
            let chunks_per_node = 1i32 << lod;
            let chunk_world_size = crate::voxel::CHUNK_SIZE as f32 * VOXEL_SCALE;
            let node_world_size = chunks_per_node as f32 * chunk_world_size;
            let max_node_radius = (lod_distance_max / node_world_size).ceil() as i32 + 1;

            // Player's node position at this LOD level
            let player_node_x = player_chunk.x >> lod;
            let player_node_y = player_chunk.y >> lod;
            let player_node_z = player_chunk.z >> lod;

            for dx in -max_node_radius..=max_node_radius {
                for dz in -max_node_radius..=max_node_radius {
                    for dy in -(self.config.vertical_range >> lod).max(1)
                        ..=(self.config.vertical_range >> lod).max(1)
                    {
                        let key = LodNodeKey::new(
                            player_node_x + dx,
                            player_node_y + dy,
                            player_node_z + dz,
                            lod,
                        );

                        // Bounds check: reject LOD nodes entirely outside world
                        let node_chunk_min_x = key.x * chunks_per_node;
                        let node_chunk_min_y = key.y * chunks_per_node;
                        let node_chunk_min_z = key.z * chunks_per_node;
                        let node_chunk_max_x = node_chunk_min_x + chunks_per_node;
                        let node_chunk_max_y = node_chunk_min_y + chunks_per_node;
                        let node_chunk_max_z = node_chunk_min_z + chunks_per_node;

                        if node_chunk_max_x <= self.config.world_min.x
                            || node_chunk_min_x >= self.config.world_max.x
                            || node_chunk_max_y <= self.config.world_min.y
                            || node_chunk_min_y >= self.config.world_max.y
                            || node_chunk_max_z <= self.config.world_min.z
                            || node_chunk_min_z >= self.config.world_max.z
                        {
                            continue;
                        }

                        let center = key.center_world_pos();
                        let distance = Vec3::new(
                            center.0 - player_pos.x,
                            center.1 - player_pos.y,
                            center.2 - player_pos.z,
                        )
                        .length();

                        // Only load if in the right distance band
                        if distance >= lod_distance_min && distance < lod_distance_max {
                            // For LOD level 1, only exclude if all constituent chunks
                            // have their meshes built (not just queued for loading).
                            // This prevents gaps where LOD1 is unloaded before LOD0 is visible.
                            if lod == 1 {
                                let all_meshed = key
                                    .child_chunk_positions()
                                    .iter()
                                    .all(|cp| self.has_mesh(*cp));

                                if !all_meshed {
                                    result.insert(key);
                                }
                            } else {
                                result.insert(key);
                            }
                        }
                    }
                }
            }
        }

        result
    }

    /// Set player position before first update (e.g. after preloading)
    pub fn set_player_position(&mut self, pos: Vec3) {
        self.last_player_pos = pos;
    }

    /// Batch-register already-loaded chunks into the streamer state
    pub fn preload_chunks(&mut self, chunks: Vec<ChunkPosition>) {
        for pos in chunks {
            if !self.loaded_set.contains(&pos) {
                self.loaded.push(LoadedChunkState {
                    position: pos,
                    lod_level: 0,
                    has_mesh: true,
                });
                self.loaded_set.insert(pos);
            }
        }
    }

    /// Batch-register already-loaded LOD nodes into the streamer state
    pub fn preload_lod_nodes(&mut self, keys: Vec<LodNodeKey>) {
        for key in keys {
            if !self.loaded_lod_set.contains(&key) {
                self.loaded_lod_nodes.push(LoadedLodNodeState {
                    key,
                    has_mesh: true,
                });
                self.loaded_lod_set.insert(key);
            }
        }
    }
}

impl ChunkPosition {
    /// Get center position in world coordinates
    pub fn center_world_pos(&self) -> (f32, f32, f32) {
        let chunk_size = crate::voxel::CHUNK_SIZE as f32;
        let half_chunk = chunk_size / 2.0;
        (
            (self.x as f32 * chunk_size + half_chunk) * VOXEL_SCALE,
            (self.y as f32 * chunk_size + half_chunk) * VOXEL_SCALE,
            (self.z as f32 * chunk_size + half_chunk) * VOXEL_SCALE,
        )
    }
}

impl Default for ChunkStreamer {
    fn default() -> Self {
        Self::new(StreamingConfig::default())
    }
}

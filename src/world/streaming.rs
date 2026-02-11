use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

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

    /// World bounds in chunk coordinates (min inclusive, max exclusive)
    /// Only generate chunks within these bounds
    pub world_min: glam::IVec3,
    pub world_max: glam::IVec3,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            lod: LodConfig::default(),
            max_loads_per_frame: 48,
            max_unloads_per_frame: 32,
            max_mesh_rebuilds_per_frame: 96,
            hysteresis: 1.15,
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

    /// Currently loaded chunks with their state (O(1) lookup)
    loaded: HashMap<ChunkPosition, LoadedChunkState>,

    /// Currently loaded LOD nodes: key → has_mesh (O(1) lookup)
    loaded_lod_nodes: HashMap<LodNodeKey, bool>,

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
            loaded: HashMap::new(),
            loaded_lod_nodes: HashMap::new(),
            last_player_pos: Vec3::ZERO,
            config,
        }
    }

    /// LOD0 maximum distance from config
    pub fn lod0_distance(&self) -> f32 {
        self.config.lod.distances[0]
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
            if !self.loaded.contains_key(pos) {
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
                if let Some(state) = self.loaded.get(pos)
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
                if self.loaded.contains_key(&request.position) {
                    continue;
                }
                if !desired.contains_key(&request.position) {
                    continue;
                }

                // Mark as loaded
                self.loaded.insert(
                    request.position,
                    LoadedChunkState {
                        position: request.position,
                        lod_level: request.lod_level,
                        has_mesh: false,
                    },
                );

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

        for (pos, state) in &self.loaded {
            if unloads_this_frame >= self.config.max_unloads_per_frame {
                break;
            }

            let should_be_loaded = desired.contains_key(pos);
            let distance = self.chunk_distance(*pos, player_pos);
            let max_distance = self.config.lod.distances[self.config.lod.levels as usize - 1];

            // Unload if too far with hysteresis
            if !should_be_loaded || distance > max_distance * self.config.hysteresis {
                to_remove.push(state.position);
                result.unload_requests.push(state.position);
                unloads_this_frame += 1;
            }
        }

        // Remove unloaded chunks
        for pos in to_remove {
            self.loaded.remove(&pos);
        }

        // 5. Retry missing chunk meshes, prioritized by distance to player.
        // This avoids starvation from HashMap iteration order, which can leave
        // persistent holes in large worlds when dispatch budget is limited.
        let mut requested: HashSet<ChunkPosition> =
            result.mesh_requests.iter().map(|(p, _)| *p).collect();
        let mut missing: Vec<(f32, ChunkPosition, LodLevel)> = self
            .loaded
            .values()
            .filter(|s| !s.has_mesh && !requested.contains(&s.position))
            .map(|s| {
                (
                    self.chunk_distance(s.position, player_pos),
                    s.position,
                    s.lod_level,
                )
            })
            .collect();
        missing.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        for (_, pos, lod) in missing
            .into_iter()
            .take(self.config.max_mesh_rebuilds_per_frame)
        {
            if requested.insert(pos) {
                result.mesh_requests.push((pos, lod));
            }
        }

        // 6. LOD node management
        let desired_lod_nodes = self.calculate_desired_lod_nodes(player_pos);

        // Queue new LOD nodes
        for key in &desired_lod_nodes {
            if !self.loaded_lod_nodes.contains_key(key) {
                self.loaded_lod_nodes.insert(*key, false);
                result.lod_mesh_requests.push(*key);
            }
        }

        // Unload LOD nodes that are no longer needed.
        // Nodes no longer desired are unloaded unconditionally.
        let keys_to_remove: Vec<LodNodeKey> = self
            .loaded_lod_nodes
            .keys()
            .filter(|key| !desired_lod_nodes.contains(key))
            .copied()
            .collect();
        for key in keys_to_remove {
            self.loaded_lod_nodes.remove(&key);
            result.lod_unload_requests.push(key);
        }

        // 7. Retry LOD nodes that are loaded but still missing meshes.
        // This handles nodes that were dropped due to per-frame dispatch caps.
        let already_requested: HashSet<LodNodeKey> =
            result.lod_mesh_requests.iter().copied().collect();
        let mut missing_lod: Vec<(f32, LodNodeKey)> = self
            .loaded_lod_nodes
            .iter()
            .filter(|(key, has_mesh)| !**has_mesh && !already_requested.contains(key))
            .map(|(key, _)| {
                let center = key.center_world_pos();
                let dx = center.0 - player_pos.x;
                let dz = center.2 - player_pos.z;
                ((dx * dx + dz * dz).sqrt(), *key)
            })
            .collect();
        missing_lod.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, key) in missing_lod
            .into_iter()
            .take(self.config.max_mesh_rebuilds_per_frame)
        {
            result.lod_mesh_requests.push(key);
        }

        result
    }

    /// Calculate which individual chunks should be loaded (LOD0 only).
    /// Higher LOD levels are handled by LOD nodes, not individual chunks.
    /// Iterates XZ only; for each desired column, emits all Y sections within world bounds.
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

        let y_min = self.config.world_min.y.max(0);
        let y_max = self.config.world_max.y;

        for dx in -max_chunk_radius..=max_chunk_radius {
            for dz in -max_chunk_radius..=max_chunk_radius {
                let cx = player_chunk.x + dx;
                let cz = player_chunk.z + dz;

                // Skip columns outside world XZ bounds
                if cx < self.config.world_min.x
                    || cx >= self.config.world_max.x
                    || cz < self.config.world_min.z
                    || cz >= self.config.world_max.z
                {
                    continue;
                }

                // XZ distance check (use a representative chunk at player Y)
                let check_pos = ChunkPosition::new(cx, player_chunk.y, cz);
                let distance = self.chunk_distance(check_pos, player_pos);

                if distance > lod0_distance {
                    continue;
                }

                // Emit all Y sections for this column
                for cy in y_min..y_max {
                    result.insert(ChunkPosition::new(cx, cy, cz), 0);
                }
            }
        }

        result
    }

    /// Calculate distance from chunk center to player
    fn chunk_distance(&self, pos: ChunkPosition, player_pos: Vec3) -> f32 {
        let chunk_center = pos.center_world_pos();
        let dx = chunk_center.0 - player_pos.x;
        let dz = chunk_center.2 - player_pos.z;
        (dx * dx + dz * dz).sqrt()
    }

    fn point_to_aabb_min_distance(player_pos: Vec3, min: Vec3, max: Vec3) -> f32 {
        let dx = if player_pos.x < min.x {
            min.x - player_pos.x
        } else if player_pos.x > max.x {
            player_pos.x - max.x
        } else {
            0.0
        };
        let dz = if player_pos.z < min.z {
            min.z - player_pos.z
        } else if player_pos.z > max.z {
            player_pos.z - max.z
        } else {
            0.0
        };
        (dx * dx + dz * dz).sqrt()
    }

    fn point_to_aabb_max_distance(player_pos: Vec3, min: Vec3, max: Vec3) -> f32 {
        let dx = (player_pos.x - min.x)
            .abs()
            .max((player_pos.x - max.x).abs());
        let dz = (player_pos.z - min.z)
            .abs()
            .max((player_pos.z - max.z).abs());
        (dx * dx + dz * dz).sqrt()
    }

    /// Mark a chunk as having its mesh built
    pub fn mark_mesh_built(&mut self, pos: ChunkPosition) {
        if let Some(state) = self.loaded.get_mut(&pos) {
            state.has_mesh = true;
        }
    }

    /// Mark a chunk as dirty (needs mesh rebuild)
    pub fn mark_dirty(&mut self, pos: ChunkPosition) {
        if let Some(state) = self.loaded.get_mut(&pos) {
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
        self.loaded.contains_key(&pos)
    }

    pub fn set_runtime_limits(
        &mut self,
        max_loads_per_frame: usize,
        max_mesh_rebuilds_per_frame: usize,
        lod_distances: [f32; 6],
    ) {
        self.config.max_loads_per_frame = max_loads_per_frame;
        self.config.max_mesh_rebuilds_per_frame = max_mesh_rebuilds_per_frame;
        self.config.lod.distances = lod_distances;
    }

    /// Check if a chunk is still waiting in the load queue at LOD0.
    pub fn is_queued_lod0(&self, pos: ChunkPosition) -> bool {
        self.queued.contains(&(pos, 0))
    }

    /// Check if a chunk is loaded AND has its mesh built
    pub fn has_mesh(&self, pos: ChunkPosition) -> bool {
        self.loaded.get(&pos).is_some_and(|s| s.has_mesh)
    }

    /// Check if a chunk is loaded but waiting for mesh generation.
    pub fn needs_mesh(&self, pos: ChunkPosition) -> bool {
        self.loaded.get(&pos).is_some_and(|s| !s.has_mesh)
    }

    /// Mark a LOD node as having its mesh built
    pub fn mark_lod_mesh_built(&mut self, key: &LodNodeKey) {
        if let Some(has_mesh) = self.loaded_lod_nodes.get_mut(key) {
            *has_mesh = true;
        }
    }

    /// Check if a LOD node is loaded
    pub fn is_lod_loaded(&self, key: &LodNodeKey) -> bool {
        self.loaded_lod_nodes.contains_key(key)
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
            let transition_margin = chunk_world_size * 2.0;
            let max_node_radius = (lod_distance_max / node_world_size).ceil() as i32 + 1;

            // Player's node position at this LOD level
            let player_node_x = player_chunk.x >> lod;
            let player_node_z = player_chunk.z >> lod;
            let min_node_y = self.config.world_min.y.div_euclid(chunks_per_node);
            let max_node_y = (self.config.world_max.y - 1).div_euclid(chunks_per_node);

            for dx in -max_node_radius..=max_node_radius {
                for dz in -max_node_radius..=max_node_radius {
                    for node_y in min_node_y..=max_node_y {
                        let key =
                            LodNodeKey::new(player_node_x + dx, node_y, player_node_z + dz, lod);

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

                        let node_min = Vec3::new(
                            node_chunk_min_x as f32 * chunk_world_size,
                            node_chunk_min_y as f32 * chunk_world_size,
                            node_chunk_min_z as f32 * chunk_world_size,
                        );
                        let node_max = Vec3::new(
                            node_chunk_max_x as f32 * chunk_world_size,
                            node_chunk_max_y as f32 * chunk_world_size,
                            node_chunk_max_z as f32 * chunk_world_size,
                        );
                        let min_distance =
                            Self::point_to_aabb_min_distance(player_pos, node_min, node_max);
                        let max_distance =
                            Self::point_to_aabb_max_distance(player_pos, node_min, node_max);

                        // Select node if its volume intersects this LOD shell.
                        // This avoids diagonal holes caused by center-only tests.
                        let in_band = max_distance
                            >= (lod_distance_min - transition_margin).max(0.0)
                            && min_distance < lod_distance_max + transition_margin;
                        if lod == 1 {
                            if in_band {
                                result.insert(key);
                            }
                            let any_child_missing = key
                                .child_chunk_positions()
                                .iter()
                                .any(|cp| !self.has_mesh(*cp));
                            let overlap_lod0 = min_distance < lod_distance_min + transition_margin;
                            if any_child_missing && overlap_lod0 {
                                result.insert(key);
                            }
                        } else if in_band {
                            result.insert(key);
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
            self.loaded.entry(pos).or_insert(LoadedChunkState {
                position: pos,
                lod_level: 0,
                has_mesh: true,
            });
        }
    }

    /// Batch-register already-loaded LOD nodes into the streamer state
    pub fn preload_lod_nodes(&mut self, keys: Vec<LodNodeKey>) {
        for key in keys {
            self.loaded_lod_nodes.entry(key).or_insert(true);
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

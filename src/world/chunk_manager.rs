use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::position::ChunkPosition;
use crate::voxel::block::BlockType;
use crate::voxel::chunk::Chunk;

#[derive(Clone, Serialize, Deserialize)]
pub struct WorldMetadata {
    pub name: String,
    pub spawn_position: [f32; 3],
}

impl Default for WorldMetadata {
    fn default() -> Self {
        Self {
            name: "Untitled".to_string(),
            spawn_position: [16.0, 20.0, 16.0],
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WorldData {
    pub metadata: WorldMetadata,
    pub chunks: HashMap<ChunkPosition, Chunk>,
}

pub struct ChunkManager {
    chunks: HashMap<ChunkPosition, Chunk>,
    dirty_chunks: HashSet<ChunkPosition>,
    metadata: WorldMetadata,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: HashSet::new(),
            metadata: WorldMetadata::default(),
        }
    }

    pub fn with_metadata(name: impl Into<String>, spawn: [f32; 3]) -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: HashSet::new(),
            metadata: WorldMetadata {
                name: name.into(),
                spawn_position: spawn,
            },
        }
    }

    /// Get a block at world coordinates. Returns Air if out of bounds or chunk doesn't exist.
    pub fn get_block(&self, wx: i32, wy: i32, wz: i32) -> BlockType {
        let (chunk_pos, lx, ly, lz) = ChunkPosition::world_to_local(wx, wy, wz);
        self.chunks
            .get(&chunk_pos)
            .map(|chunk| chunk.get(lx, ly, lz))
            .unwrap_or(BlockType::Air)
    }

    /// Set a block at world coordinates. Creates chunk if needed, marks it dirty.
    pub fn set_block(&mut self, wx: i32, wy: i32, wz: i32, block: BlockType) {
        let (chunk_pos, lx, ly, lz) = ChunkPosition::world_to_local(wx, wy, wz);

        let chunk = self.chunks.entry(chunk_pos).or_insert_with(Chunk::new);
        chunk.set(lx, ly, lz, block);
        self.dirty_chunks.insert(chunk_pos);

        // Mark neighboring chunks dirty if at chunk boundary (for mesh updates)
        if lx == 0 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x - 1, chunk_pos.y, chunk_pos.z));
        }
        if lx == 31 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x + 1, chunk_pos.y, chunk_pos.z));
        }
        if ly == 0 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x, chunk_pos.y - 1, chunk_pos.z));
        }
        if ly == 31 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x, chunk_pos.y + 1, chunk_pos.z));
        }
        if lz == 0 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x, chunk_pos.y, chunk_pos.z - 1));
        }
        if lz == 31 {
            self.dirty_chunks
                .insert(ChunkPosition::new(chunk_pos.x, chunk_pos.y, chunk_pos.z + 1));
        }
    }

    /// Check if a block is solid at world coordinates
    pub fn is_solid(&self, wx: i32, wy: i32, wz: i32) -> bool {
        self.get_block(wx, wy, wz).is_solid()
    }

    /// Get a reference to a chunk at the given position
    pub fn get_chunk(&self, pos: ChunkPosition) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    /// Get a mutable reference to a chunk at the given position
    pub fn get_chunk_mut(&mut self, pos: ChunkPosition) -> Option<&mut Chunk> {
        self.dirty_chunks.insert(pos);
        self.chunks.get_mut(&pos)
    }

    /// Insert a chunk at the given position
    pub fn insert_chunk(&mut self, pos: ChunkPosition, chunk: Chunk) {
        self.chunks.insert(pos, chunk);
        self.dirty_chunks.insert(pos);
    }

    /// Remove a chunk at the given position
    pub fn remove_chunk(&mut self, pos: ChunkPosition) -> Option<Chunk> {
        self.dirty_chunks.remove(&pos);
        self.chunks.remove(&pos)
    }

    /// Iterate over all chunks
    pub fn chunks(&self) -> impl Iterator<Item = (&ChunkPosition, &Chunk)> {
        self.chunks.iter()
    }

    /// Get all chunk positions
    pub fn chunk_positions(&self) -> impl Iterator<Item = &ChunkPosition> {
        self.chunks.keys()
    }

    /// Get the set of dirty chunks (chunks that need mesh rebuild)
    pub fn dirty_chunks(&self) -> &HashSet<ChunkPosition> {
        &self.dirty_chunks
    }

    /// Take the dirty chunks set, clearing it
    pub fn take_dirty_chunks(&mut self) -> HashSet<ChunkPosition> {
        std::mem::take(&mut self.dirty_chunks)
    }

    /// Mark all chunks as dirty
    pub fn mark_all_dirty(&mut self) {
        for pos in self.chunks.keys() {
            self.dirty_chunks.insert(*pos);
        }
    }

    /// Get the bounding box of occupied chunks (min, max)
    pub fn bounds(&self) -> Option<(ChunkPosition, ChunkPosition)> {
        if self.chunks.is_empty() {
            return None;
        }

        let mut min = ChunkPosition::new(i32::MAX, i32::MAX, i32::MAX);
        let mut max = ChunkPosition::new(i32::MIN, i32::MIN, i32::MIN);

        for pos in self.chunks.keys() {
            min.x = min.x.min(pos.x);
            min.y = min.y.min(pos.y);
            min.z = min.z.min(pos.z);
            max.x = max.x.max(pos.x);
            max.y = max.y.max(pos.y);
            max.z = max.z.max(pos.z);
        }

        Some((min, max))
    }

    /// Get world metadata
    pub fn metadata(&self) -> &WorldMetadata {
        &self.metadata
    }

    /// Set spawn position
    pub fn set_spawn(&mut self, spawn: [f32; 3]) {
        self.metadata.spawn_position = spawn;
    }

    /// Get spawn position
    pub fn spawn_position(&self) -> [f32; 3] {
        self.metadata.spawn_position
    }

    /// Convert to serializable WorldData
    pub fn to_world_data(&self) -> WorldData {
        WorldData {
            metadata: self.metadata.clone(),
            chunks: self.chunks.clone(),
        }
    }

    /// Create from WorldData
    pub fn from_world_data(data: WorldData) -> Self {
        let mut manager = Self {
            chunks: data.chunks,
            dirty_chunks: HashSet::new(),
            metadata: data.metadata,
        };
        manager.mark_all_dirty();
        manager
    }

    /// Get the number of chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self::new()
    }
}

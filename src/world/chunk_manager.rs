use std::collections::{HashMap, HashSet};

use super::position::{ChunkPosition, RegionCoord};
use crate::voxel::Voxel;
use crate::voxel::chunk::{AIR, Chunk};

pub struct ChunkManager {
    chunks: HashMap<ChunkPosition, Chunk>,
    dirty_chunks: HashSet<ChunkPosition>,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: HashSet::new(),
        }
    }

    pub fn with_metadata(_name: impl Into<String>, _spawn: [f32; 3]) -> Self {
        Self::new()
    }

    /// Get a voxel at world coordinates. Returns AIR if out of bounds or chunk doesn't exist.
    #[inline]
    pub fn get_voxel(&self, wx: i32, wy: i32, wz: i32) -> Voxel {
        let (chunk_pos, lx, ly, lz) = ChunkPosition::world_to_local(wx, wy, wz);
        self.chunks
            .get(&chunk_pos)
            .map(|chunk| chunk.get(lx, ly, lz))
            .unwrap_or(AIR)
    }

    /// Check if a voxel is solid at world coordinates
    #[inline]
    pub fn is_solid(&self, wx: i32, wy: i32, wz: i32) -> bool {
        self.get_voxel(wx, wy, wz) != AIR
    }

    /// Set multiple voxels at once (batch operation, more efficient than individual set_voxel calls)
    pub fn set_voxels_batch(&mut self, positions: &[(i32, i32, i32, Voxel)]) {
        // Group positions by chunk
        let mut chunk_updates: HashMap<ChunkPosition, Vec<(usize, usize, usize, Voxel)>> =
            HashMap::new();

        for &(wx, wy, wz, voxel) in positions {
            let (chunk_pos, lx, ly, lz) = ChunkPosition::world_to_local(wx, wy, wz);
            chunk_updates
                .entry(chunk_pos)
                .or_default()
                .push((lx, ly, lz, voxel));
        }

        // Apply updates per chunk
        for (chunk_pos, updates) in chunk_updates {
            let chunk = self.chunks.entry(chunk_pos).or_default();

            let mut has_boundary_x_low = false;
            let mut has_boundary_x_high = false;
            let mut has_boundary_y_low = false;
            let mut has_boundary_y_high = false;
            let mut has_boundary_z_low = false;
            let mut has_boundary_z_high = false;

            for (lx, ly, lz, voxel) in updates {
                chunk.set(lx, ly, lz, voxel);

                // Track boundary touches
                if lx == 0 {
                    has_boundary_x_low = true;
                }
                if lx == 31 {
                    has_boundary_x_high = true;
                }
                if ly == 0 {
                    has_boundary_y_low = true;
                }
                if ly == 31 {
                    has_boundary_y_high = true;
                }
                if lz == 0 {
                    has_boundary_z_low = true;
                }
                if lz == 31 {
                    has_boundary_z_high = true;
                }
            }

            self.dirty_chunks.insert(chunk_pos);

            // Mark neighboring chunks dirty only once per boundary
            if has_boundary_x_low {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x - 1,
                    chunk_pos.y,
                    chunk_pos.z,
                ));
            }
            if has_boundary_x_high {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x + 1,
                    chunk_pos.y,
                    chunk_pos.z,
                ));
            }
            if has_boundary_y_low {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x,
                    chunk_pos.y - 1,
                    chunk_pos.z,
                ));
            }
            if has_boundary_y_high {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x,
                    chunk_pos.y + 1,
                    chunk_pos.z,
                ));
            }
            if has_boundary_z_low {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x,
                    chunk_pos.y,
                    chunk_pos.z - 1,
                ));
            }
            if has_boundary_z_high {
                self.dirty_chunks.insert(ChunkPosition::new(
                    chunk_pos.x,
                    chunk_pos.y,
                    chunk_pos.z + 1,
                ));
            }
        }
    }

    /// Get a reference to a chunk at the given position
    pub fn get_chunk(&self, pos: ChunkPosition) -> Option<&Chunk> {
        self.chunks.get(&pos)
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

    /// Mark a single chunk as dirty (needs mesh rebuild)
    pub fn mark_chunk_dirty(&mut self, pos: ChunkPosition) {
        self.dirty_chunks.insert(pos);
    }

    /// Clear dirty flag for a single chunk (e.g. after streaming mesh was just built)
    pub fn clear_chunk_dirty(&mut self, pos: ChunkPosition) {
        self.dirty_chunks.remove(&pos);
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

    /// Get the number of chunks
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl ChunkManager {
    /// Collect all chunks belonging to a region (borrows).
    pub fn chunks_in_region(&self, coord: RegionCoord) -> Vec<(ChunkPosition, &Chunk)> {
        let min_x = coord.rx * super::position::REGION_SIZE;
        let max_x = min_x + super::position::REGION_SIZE;
        let min_z = coord.rz * super::position::REGION_SIZE;
        let max_z = min_z + super::position::REGION_SIZE;
        self.chunks
            .iter()
            .filter(|(pos, _)| pos.x >= min_x && pos.x < max_x && pos.z >= min_z && pos.z < max_z)
            .map(|(pos, chunk)| (*pos, chunk))
            .collect()
    }

    /// Remove all chunks in a region, returning their positions for mesh cleanup.
    pub fn remove_region_chunks(&mut self, coord: RegionCoord) -> Vec<ChunkPosition> {
        let min_x = coord.rx * super::position::REGION_SIZE;
        let max_x = min_x + super::position::REGION_SIZE;
        let min_z = coord.rz * super::position::REGION_SIZE;
        let max_z = min_z + super::position::REGION_SIZE;
        let to_remove: Vec<ChunkPosition> = self
            .chunks
            .keys()
            .filter(|pos| pos.x >= min_x && pos.x < max_x && pos.z >= min_z && pos.z < max_z)
            .copied()
            .collect();
        for pos in &to_remove {
            self.chunks.remove(pos);
            self.dirty_chunks.remove(pos);
        }
        to_remove
    }
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self::new()
    }
}

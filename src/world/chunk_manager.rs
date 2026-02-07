use std::collections::{HashMap, HashSet};

use super::position::{ChunkPosition, ColumnPos, RegionCoord};
use crate::voxel::Voxel;
use crate::voxel::chunk::{AIR, Chunk, Column};

pub struct ChunkManager {
    columns: HashMap<ColumnPos, Column>,
    dirty_chunks: HashSet<ChunkPosition>,
    cached_min: Option<ChunkPosition>,
    cached_max: Option<ChunkPosition>,
    bounds_dirty: bool,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
            dirty_chunks: HashSet::new(),
            cached_min: None,
            cached_max: None,
            bounds_dirty: false,
        }
    }

    pub fn with_metadata(_name: impl Into<String>, _spawn: [f32; 3]) -> Self {
        Self::new()
    }

    /// Get a voxel at world coordinates. Returns AIR if out of bounds or chunk doesn't exist.
    #[inline]
    pub fn get_voxel(&self, wx: i32, wy: i32, wz: i32) -> Voxel {
        let (chunk_pos, lx, ly, lz) = ChunkPosition::world_to_local(wx, wy, wz);
        let col_pos = chunk_pos.column_pos();
        self.columns
            .get(&col_pos)
            .and_then(|col| col.get_section(chunk_pos.y as u8))
            .map(|section| section.get(lx, ly, lz))
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
            let col_pos = chunk_pos.column_pos();
            let section_y = chunk_pos.y as u8;
            let col = self.columns.entry(col_pos).or_default();
            if !col.has_section(section_y) {
                col.set_section(section_y, Chunk::new());
            }
            let section = col.get_section_mut(section_y).unwrap();

            let mut has_boundary_x_low = false;
            let mut has_boundary_x_high = false;
            let mut has_boundary_y_low = false;
            let mut has_boundary_y_high = false;
            let mut has_boundary_z_low = false;
            let mut has_boundary_z_high = false;

            for (lx, ly, lz, voxel) in updates {
                section.set(lx, ly, lz, voxel);

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

    /// Get a reference to a chunk (section) at the given position
    pub fn get_chunk(&self, pos: ChunkPosition) -> Option<&Chunk> {
        let col_pos = pos.column_pos();
        self.columns
            .get(&col_pos)
            .and_then(|col| col.get_section(pos.y as u8))
    }

    /// Insert a chunk (section) at the given position
    pub fn insert_chunk(&mut self, pos: ChunkPosition, chunk: Chunk) {
        let col_pos = pos.column_pos();
        let col = self.columns.entry(col_pos).or_default();
        col.set_section(pos.y as u8, chunk);
        self.dirty_chunks.insert(pos);

        // Expand cached bounds incrementally
        match (&mut self.cached_min, &mut self.cached_max) {
            (Some(cmin), Some(cmax)) => {
                cmin.x = cmin.x.min(pos.x);
                cmin.y = cmin.y.min(pos.y);
                cmin.z = cmin.z.min(pos.z);
                cmax.x = cmax.x.max(pos.x);
                cmax.y = cmax.y.max(pos.y);
                cmax.z = cmax.z.max(pos.z);
            }
            _ => {
                self.cached_min = Some(pos);
                self.cached_max = Some(pos);
            }
        }
    }

    /// Remove a chunk (section) at the given position
    pub fn remove_chunk(&mut self, pos: ChunkPosition) -> Option<Chunk> {
        self.dirty_chunks.remove(&pos);
        let col_pos = pos.column_pos();
        let col = self.columns.get_mut(&col_pos)?;
        let result = col.remove_section(pos.y as u8);
        // Drop column if all sections are gone
        if col.section_count() == 0 {
            self.columns.remove(&col_pos);
        }
        if result.is_some() {
            self.bounds_dirty = true;
        }
        result
    }

    /// Iterate over all chunks (sections). Yields (&ChunkPosition, &Chunk) pairs.
    /// Note: positions are reconstructed on the fly.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPosition, &Chunk)> {
        self.columns.iter().flat_map(|(col_pos, col)| {
            col.sections_iter()
                .map(move |(sy, section)| (col_pos.to_chunk_pos(sy), section))
        })
    }

    /// Get all chunk positions (reconstructed from columns + sections).
    pub fn chunk_positions(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        self.columns.iter().flat_map(|(col_pos, col)| {
            col.sections_iter()
                .map(move |(sy, _)| col_pos.to_chunk_pos(sy))
        })
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
    pub fn bounds(&mut self) -> Option<(ChunkPosition, ChunkPosition)> {
        if self.columns.is_empty() {
            self.cached_min = None;
            self.cached_max = None;
            self.bounds_dirty = false;
            return None;
        }

        if self.bounds_dirty || self.cached_min.is_none() {
            let mut min = ChunkPosition::new(i32::MAX, i32::MAX, i32::MAX);
            let mut max = ChunkPosition::new(i32::MIN, i32::MIN, i32::MIN);

            for (col_pos, col) in &self.columns {
                for (sy, _) in col.sections_iter() {
                    let pos = col_pos.to_chunk_pos(sy);
                    min.x = min.x.min(pos.x);
                    min.y = min.y.min(pos.y);
                    min.z = min.z.min(pos.z);
                    max.x = max.x.max(pos.x);
                    max.y = max.y.max(pos.y);
                    max.z = max.z.max(pos.z);
                }
            }

            self.cached_min = Some(min);
            self.cached_max = Some(max);
            self.bounds_dirty = false;
        }

        Some((self.cached_min.unwrap(), self.cached_max.unwrap()))
    }

    /// Get the number of chunks (sections) across all columns
    pub fn chunk_count(&self) -> usize {
        self.columns.values().map(|col| col.section_count()).sum()
    }

    /// Get a reference to a column.
    pub fn get_column(&self, col_pos: ColumnPos) -> Option<&Column> {
        self.columns.get(&col_pos)
    }

    /// Insert a full column, marking all its sections dirty.
    pub fn insert_column(&mut self, col_pos: ColumnPos, column: Column) {
        // Mark all sections dirty
        for (sy, _) in column.sections_iter() {
            let pos = col_pos.to_chunk_pos(sy);
            self.dirty_chunks.insert(pos);

            // Update bounds
            match (&mut self.cached_min, &mut self.cached_max) {
                (Some(cmin), Some(cmax)) => {
                    cmin.x = cmin.x.min(pos.x);
                    cmin.y = cmin.y.min(pos.y);
                    cmin.z = cmin.z.min(pos.z);
                    cmax.x = cmax.x.max(pos.x);
                    cmax.y = cmax.y.max(pos.y);
                    cmax.z = cmax.z.max(pos.z);
                }
                _ => {
                    self.cached_min = Some(pos);
                    self.cached_max = Some(pos);
                }
            }
        }
        self.columns.insert(col_pos, column);
    }

    /// Remove a full column, returning section positions for mesh cleanup.
    pub fn remove_column(&mut self, col_pos: ColumnPos) -> Vec<ChunkPosition> {
        let mut removed = Vec::new();
        if let Some(col) = self.columns.remove(&col_pos) {
            for (sy, _) in col.sections_iter() {
                let pos = col_pos.to_chunk_pos(sy);
                self.dirty_chunks.remove(&pos);
                removed.push(pos);
            }
            if !removed.is_empty() {
                self.bounds_dirty = true;
            }
        }
        removed
    }

    /// Iterate over column positions.
    pub fn column_positions(&self) -> impl Iterator<Item = &ColumnPos> {
        self.columns.keys()
    }
}

impl ChunkManager {
    /// Collect all chunks belonging to a region (borrows).
    pub fn chunks_in_region(&self, coord: RegionCoord) -> Vec<(ChunkPosition, &Chunk)> {
        let min_x = coord.rx * super::position::REGION_SIZE;
        let max_x = min_x + super::position::REGION_SIZE;
        let min_z = coord.rz * super::position::REGION_SIZE;
        let max_z = min_z + super::position::REGION_SIZE;
        self.columns
            .iter()
            .filter(|(col_pos, _)| {
                col_pos.x >= min_x && col_pos.x < max_x && col_pos.z >= min_z && col_pos.z < max_z
            })
            .flat_map(|(col_pos, col)| {
                col.sections_iter()
                    .map(move |(sy, section)| (col_pos.to_chunk_pos(sy), section))
            })
            .collect()
    }

    /// Remove all chunks in a region, returning their positions for mesh cleanup.
    pub fn remove_region_chunks(&mut self, coord: RegionCoord) -> Vec<ChunkPosition> {
        let min_x = coord.rx * super::position::REGION_SIZE;
        let max_x = min_x + super::position::REGION_SIZE;
        let min_z = coord.rz * super::position::REGION_SIZE;
        let max_z = min_z + super::position::REGION_SIZE;

        let to_remove: Vec<ColumnPos> = self
            .columns
            .keys()
            .filter(|col_pos| {
                col_pos.x >= min_x && col_pos.x < max_x && col_pos.z >= min_z && col_pos.z < max_z
            })
            .copied()
            .collect();

        let mut removed_positions = Vec::new();
        for col_pos in to_remove {
            if let Some(col) = self.columns.remove(&col_pos) {
                for (sy, _) in col.sections_iter() {
                    let pos = col_pos.to_chunk_pos(sy);
                    self.dirty_chunks.remove(&pos);
                    removed_positions.push(pos);
                }
            }
        }
        if !removed_positions.is_empty() {
            self.bounds_dirty = true;
        }
        removed_positions
    }

    /// Number of columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self::new()
    }
}

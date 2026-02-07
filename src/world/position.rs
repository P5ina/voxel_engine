use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::voxel::chunk::CHUNK_SIZE;

/// Size of a region in chunks along X and Z axes.
/// Each region covers REGION_SIZE x WORLD_HEIGHT x REGION_SIZE chunks.
pub const REGION_SIZE: i32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPosition {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Converts world block coordinates to chunk position
    pub fn from_world_coords(wx: i32, wy: i32, wz: i32) -> Self {
        let chunk_size = CHUNK_SIZE as i32;
        Self {
            x: wx.div_euclid(chunk_size),
            y: wy.div_euclid(chunk_size),
            z: wz.div_euclid(chunk_size),
        }
    }

    /// Converts world position (in world units, f32) to chunk position
    pub fn from_world_pos(wx: f32, wy: f32, wz: f32) -> Self {
        use crate::voxel::chunk::VOXEL_SCALE;
        // Convert world units to voxel coordinates, then to chunk position
        let vx = (wx / VOXEL_SCALE).floor() as i32;
        let vy = (wy / VOXEL_SCALE).floor() as i32;
        let vz = (wz / VOXEL_SCALE).floor() as i32;
        Self::from_world_coords(vx, vy, vz)
    }

    /// Returns the world-space origin (minimum corner) of this chunk
    pub fn world_origin(&self) -> (i32, i32, i32) {
        let chunk_size = CHUNK_SIZE as i32;
        (
            self.x * chunk_size,
            self.y * chunk_size,
            self.z * chunk_size,
        )
    }

    /// Converts world coordinates to chunk position and local coordinates within that chunk
    pub fn world_to_local(wx: i32, wy: i32, wz: i32) -> (Self, usize, usize, usize) {
        let chunk_size = CHUNK_SIZE as i32;
        let chunk_pos = Self::from_world_coords(wx, wy, wz);
        let local_x = wx.rem_euclid(chunk_size) as usize;
        let local_y = wy.rem_euclid(chunk_size) as usize;
        let local_z = wz.rem_euclid(chunk_size) as usize;
        (chunk_pos, local_x, local_y, local_z)
    }
}

impl ChunkPosition {
    /// Get the column position (XZ) for this chunk
    pub fn column_pos(&self) -> ColumnPos {
        ColumnPos {
            x: self.x,
            z: self.z,
        }
    }

    /// Get the section Y index within a column (0..NUM_SECTIONS)
    pub fn section_y(&self) -> Option<u8> {
        if self.y >= 0 && self.y < crate::voxel::chunk::NUM_SECTIONS as i32 {
            Some(self.y as u8)
        } else {
            None
        }
    }
}

impl Default for ChunkPosition {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

/// Identifies a vertical column of sections at (x, z) in chunk coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColumnPos {
    pub x: i32,
    pub z: i32,
}

impl ColumnPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Derive column position from a chunk position (drops Y).
    pub fn from_chunk_pos(pos: ChunkPosition) -> Self {
        Self { x: pos.x, z: pos.z }
    }

    /// Derive column position from world-space position (f32).
    pub fn from_world_pos(wx: f32, wz: f32) -> Self {
        let cp = ChunkPosition::from_world_pos(wx, 0.0, wz);
        Self { x: cp.x, z: cp.z }
    }

    /// Center of this column in world coordinates (XZ only).
    pub fn center_world_pos(&self) -> (f32, f32) {
        use crate::voxel::chunk::VOXEL_SCALE;
        let chunk_size = CHUNK_SIZE as f32;
        let half_chunk = chunk_size / 2.0;
        (
            (self.x as f32 * chunk_size + half_chunk) * VOXEL_SCALE,
            (self.z as f32 * chunk_size + half_chunk) * VOXEL_SCALE,
        )
    }

    /// Convert to a ChunkPosition at the given section Y.
    pub fn to_chunk_pos(self, section_y: u8) -> ChunkPosition {
        ChunkPosition::new(self.x, section_y as i32, self.z)
    }
}

/// Key identifying an LOD node in the octree.
/// At lod_level L, each node represents 2^L x 2^L x 2^L chunks.
/// Coordinates are in "LOD-space" (chunk coords >> lod_level).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LodNodeKey {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub lod_level: u8,
}

impl LodNodeKey {
    pub fn new(x: i32, y: i32, z: i32, lod_level: u8) -> Self {
        Self { x, y, z, lod_level }
    }

    /// World-space origin (minimum corner) in voxel coordinates.
    /// Each LOD node covers 2^lod_level chunks per axis, each chunk is CHUNK_SIZE voxels.
    pub fn world_origin(&self) -> (i32, i32, i32) {
        let chunks_per_axis = 1i32 << self.lod_level;
        let chunk_size = CHUNK_SIZE as i32;
        (
            self.x * chunks_per_axis * chunk_size,
            self.y * chunks_per_axis * chunk_size,
            self.z * chunks_per_axis * chunk_size,
        )
    }

    /// Center of this LOD node in world units (for distance calculations).
    pub fn center_world_pos(&self) -> (f32, f32, f32) {
        use crate::voxel::chunk::VOXEL_SCALE;
        let chunks_per_axis = (1u32 << self.lod_level) as f32;
        let chunk_size = CHUNK_SIZE as f32;
        let half = chunks_per_axis * chunk_size / 2.0;
        (
            (self.x as f32 * chunks_per_axis * chunk_size + half) * VOXEL_SCALE,
            (self.y as f32 * chunks_per_axis * chunk_size + half) * VOXEL_SCALE,
            (self.z as f32 * chunks_per_axis * chunk_size + half) * VOXEL_SCALE,
        )
    }

    /// World-unit size of one LOD voxel.
    /// The node's 32^3 grid covers 2^lod_level chunks worth of space,
    /// so each LOD voxel is (2^lod_level * CHUNK_SIZE / 32) real voxels wide.
    pub fn voxel_scale(&self) -> f32 {
        use crate::voxel::chunk::VOXEL_SCALE;
        let real_voxels_per_lod_voxel = (1u32 << self.lod_level) as f32;
        // Each LOD node is 32^3, covering (2^L * 32) real voxels per axis
        // So each LOD voxel = 2^L real voxels
        real_voxels_per_lod_voxel * VOXEL_SCALE
    }

    /// Child chunk positions covered by this LOD node (at lod_level 1, returns 8 chunks).
    pub fn child_chunk_positions(&self) -> Vec<ChunkPosition> {
        let chunks_per_axis = 1i32 << self.lod_level;
        let base_x = self.x * chunks_per_axis;
        let base_y = self.y * chunks_per_axis;
        let base_z = self.z * chunks_per_axis;
        let mut positions = Vec::new();
        for dx in 0..chunks_per_axis {
            for dy in 0..chunks_per_axis {
                for dz in 0..chunks_per_axis {
                    positions.push(ChunkPosition::new(base_x + dx, base_y + dy, base_z + dz));
                }
            }
        }
        positions
    }
}

/// Identifies a region (16x16 chunk column in XZ, full Y height).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionCoord {
    pub rx: i32,
    pub rz: i32,
}

impl RegionCoord {
    pub fn new(rx: i32, rz: i32) -> Self {
        Self { rx, rz }
    }

    /// Derive region coordinate from a chunk position.
    pub fn from_chunk_pos(pos: ChunkPosition) -> Self {
        Self {
            rx: pos.x.div_euclid(REGION_SIZE),
            rz: pos.z.div_euclid(REGION_SIZE),
        }
    }

    /// Region file name (e.g. `r_0_0.region`).
    pub fn file_name(&self) -> String {
        format!("r_{}_{}.region", self.rx, self.rz)
    }

    /// Full path to this region file within a world directory.
    pub fn file_path(&self, world_dir: &std::path::Path) -> PathBuf {
        world_dir.join("regions").join(self.file_name())
    }

    /// Center of this region in world-space (meters), for distance calculations.
    pub fn center_world_pos(&self) -> (f32, f32) {
        use crate::voxel::chunk::VOXEL_SCALE;
        let half = REGION_SIZE as f32 * CHUNK_SIZE as f32 / 2.0;
        (
            (self.rx as f32 * REGION_SIZE as f32 * CHUNK_SIZE as f32 + half) * VOXEL_SCALE,
            (self.rz as f32 * REGION_SIZE as f32 * CHUNK_SIZE as f32 + half) * VOXEL_SCALE,
        )
    }
}

use serde::{Deserialize, Serialize};

use crate::voxel::chunk::CHUNK_SIZE;

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

impl Default for ChunkPosition {
    fn default() -> Self {
        Self::new(0, 0, 0)
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
        Self {
            x,
            y,
            z,
            lod_level,
        }
    }

    /// Map a chunk position to the LOD node that contains it at a given level.
    pub fn from_chunk_pos(pos: ChunkPosition, lod_level: u8) -> Self {
        Self {
            x: pos.x >> lod_level,
            y: pos.y >> lod_level,
            z: pos.z >> lod_level,
            lod_level,
        }
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

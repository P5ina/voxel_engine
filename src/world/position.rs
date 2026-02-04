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

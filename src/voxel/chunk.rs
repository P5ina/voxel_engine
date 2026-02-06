use serde::{Deserialize, Serialize};

use super::block::Voxel;

pub const CHUNK_SIZE: usize = 32;
/// Scale factor for voxel size in world units.
/// 1/16 = 0.0625, meaning 16 voxels per world unit (Teardown-style).
pub const VOXEL_SCALE: f32 = 1.0 / 16.0;

/// Air voxel (empty space)
pub const AIR: Voxel = 0;

#[derive(Clone, Serialize, Deserialize)]
pub struct Chunk {
    voxels: [[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            voxels: [[[AIR; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Voxel {
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            self.voxels[x][y][z]
        } else {
            AIR
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: Voxel) {
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            self.voxels[x][y][z] = voxel;
        }
    }

    #[inline]
    pub fn get_signed(&self, x: i32, y: i32, z: i32) -> Voxel {
        if x < 0 || y < 0 || z < 0 {
            return AIR;
        }
        self.get(x as usize, y as usize, z as usize)
    }

    #[inline]
    pub fn is_solid(&self, x: usize, y: usize, z: usize) -> bool {
        self.get(x, y, z) != AIR
    }

    #[inline]
    pub fn is_solid_signed(&self, x: i32, y: i32, z: i32) -> bool {
        self.get_signed(x, y, z) != AIR
    }

    /// Get raw voxel data array reference
    pub fn data(&self) -> &[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE] {
        &self.voxels
    }

    /// Fill ground with a specific color
    pub fn fill_ground(&mut self, height: usize, color: Voxel) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..height.min(CHUNK_SIZE) {
                    self.voxels[x][y][z] = color;
                }
            }
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

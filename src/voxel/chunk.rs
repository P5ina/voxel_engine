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

    /// Create a chunk from existing voxel data
    pub fn from_data(data: [[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]) -> Self {
        Self { voxels: data }
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

    /// Check if all voxels are air
    pub fn is_empty(&self) -> bool {
        // Check as flat byte slice for speed
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                self.voxels.as_ptr() as *const u8,
                CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE,
            )
        };
        bytes.iter().all(|&v| v == AIR)
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

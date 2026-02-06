use crate::AppState;
use crate::voxel::{CHUNK_SIZE, Chunk};
use crate::world::{self, LodNodeKey};

impl AppState {
    /// Height of big world terrain in Y chunks
    pub(crate) const WORLD_HEIGHT_CHUNKS: i32 = 32;

    /// Maximum terrain height in voxels (used for air-skip optimizations)
    pub(crate) const MAX_TERRAIN_VOXEL_HEIGHT: i32 = 900;

    /// Static version of chunk data generation for parallel use
    pub(crate) fn generate_chunk_data_static(pos: crate::world::ChunkPosition) -> Chunk {
        let mut chunk = Chunk::new();

        // Quick reject: chunks above max terrain height are all air
        let chunk_bottom_voxel = pos.y * CHUNK_SIZE as i32;
        if chunk_bottom_voxel > Self::MAX_TERRAIN_VOXEL_HEIGHT || pos.y < 0 {
            return chunk;
        }

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                // World voxel coordinates
                let wx = pos.x * CHUNK_SIZE as i32 + lx as i32;
                let wz = pos.z * CHUNK_SIZE as i32 + lz as i32;

                // Compute terrain height at this column
                let height = Self::terrain_height(wx, wz);

                // Skip if entire column in this chunk is above terrain
                if chunk_bottom_voxel >= height {
                    continue;
                }

                for ly in 0..CHUNK_SIZE {
                    let wy = chunk_bottom_voxel + ly as i32;

                    if wy >= height {
                        break; // Rest of column is air
                    }

                    let depth_below_surface = height - wy;
                    let voxel = if height > 700 && depth_below_surface <= 2 {
                        // High altitude: snow caps
                        let snow_noise =
                            Self::hash_2d(wx.wrapping_add(5000), wz.wrapping_add(6000));
                        if snow_noise < 0.7 { 15 } // White (snow)
                        else { 42 } // Exposed rock
                    } else if height > 500 && depth_below_surface <= 1 {
                        // Mountain zone: bare rock surface
                        let rock_noise =
                            Self::hash_2d(wx.wrapping_add(3000), wz.wrapping_add(4000));
                        if rock_noise < 0.5 {
                            42
                        }
                        // Stone gray
                        else if rock_noise < 0.8 {
                            43
                        }
                        // Light stone
                        else {
                            44
                        } // Dark stone
                    } else if depth_below_surface <= 1 {
                        // Surface: grass (vary shade by position)
                        let grass_noise =
                            Self::hash_2d(wx.wrapping_add(1000), wz.wrapping_add(2000));
                        if grass_noise < 0.33 {
                            49
                        }
                        // Grass green
                        else if grass_noise < 0.66 {
                            50
                        }
                        // Light grass
                        else {
                            51
                        } // Dark grass
                    } else if depth_below_surface <= 5 {
                        // Subsurface: dirt
                        33 // Dirt
                    } else if depth_below_surface <= 20 {
                        // Shallow rock layer
                        let stone_noise =
                            Self::hash_2d(wx.wrapping_add(3000), wz.wrapping_add(4000));
                        if stone_noise < 0.4 {
                            42
                        }
                        // Stone gray
                        else if stone_noise < 0.7 {
                            43
                        }
                        // Light stone
                        else {
                            44
                        } // Dark stone
                    } else {
                        // Deep bedrock (vary shade)
                        let deep_noise =
                            Self::hash_2d(wx.wrapping_add(7000), wz.wrapping_add(8000));
                        if deep_noise < 0.5 { 44 } // Dark stone
                        else { 42 } // Stone gray
                    };

                    chunk.set(lx, ly, lz, voxel);
                }
            }
        }

        chunk
    }

    /// Compute terrain height in voxels at world voxel coordinates (wx, wz)
    pub(crate) fn terrain_height(wx: i32, wz: i32) -> i32 {
        let x = wx as f32;
        let z = wz as f32;

        // Continental base shape — very large, slow undulation
        let continental = Self::fbm(x * 0.0003, z * 0.0003, 2);
        // Remap: [-1,1] → [0,1], then power curve for flatter lowlands, steeper highlands
        let continental = ((continental + 1.0) * 0.5).powf(1.8);

        // Mountain ridges — sharp, high-frequency features
        let ridge_raw = Self::fbm(x * 0.0012 + 500.0, z * 0.0012 + 700.0, 4);
        let ridges = (1.0 - ridge_raw.abs()) * (1.0 - ridge_raw.abs()); // squared ridged noise

        // Large rolling hills
        let h1 = Self::fbm(x * 0.0008, z * 0.0008, 4) * 200.0;
        // Medium terrain variation
        let h2 = Self::fbm(x * 0.003 + 100.0, z * 0.003 + 200.0, 4) * 80.0;
        // Small bumps and detail
        let h3 = Self::fbm(x * 0.012 + 300.0, z * 0.012 + 400.0, 3) * 20.0;
        // Fine detail
        let h4 = Self::fbm(x * 0.05 + 600.0, z * 0.05 + 800.0, 2) * 5.0;

        // Combine: continental controls overall elevation, ridges add mountains
        let base = 120.0 + continental * 400.0 + ridges * 250.0 * continental;
        let height = base + h1 + h2 + h3 + h4;

        // Range: ~120 to ~900 voxels (7.5m to 56m world height)
        height.clamp(16.0, Self::MAX_TERRAIN_VOXEL_HEIGHT as f32) as i32
    }

    /// Fractional Brownian Motion (layered noise)
    pub(crate) fn fbm(x: f32, z: f32, octaves: u32) -> f32 {
        let mut value = 0.0f32;
        let mut amplitude = 1.0f32;
        let mut frequency = 1.0f32;
        let mut max_value = 0.0f32;

        for _ in 0..octaves {
            value += Self::smooth_noise(x * frequency, z * frequency) * amplitude;
            max_value += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }

        // Returns [-1, 1] range
        (value / max_value) * 2.0 - 1.0
    }

    /// Smooth value noise with bicubic-like interpolation
    pub(crate) fn smooth_noise(x: f32, z: f32) -> f32 {
        let ix = x.floor() as i32;
        let iz = z.floor() as i32;
        let fx = x - x.floor();
        let fz = z - z.floor();

        // Smoothstep interpolation
        let fx = fx * fx * (3.0 - 2.0 * fx);
        let fz = fz * fz * (3.0 - 2.0 * fz);

        let v00 = Self::hash_2d(ix, iz);
        let v10 = Self::hash_2d(ix + 1, iz);
        let v01 = Self::hash_2d(ix, iz + 1);
        let v11 = Self::hash_2d(ix + 1, iz + 1);

        let a = v00 + (v10 - v00) * fx;
        let b = v01 + (v11 - v01) * fx;
        a + (b - a) * fz
    }

    /// Hash function: maps integer (x,z) to pseudo-random float in [0, 1]
    pub(crate) fn hash_2d(x: i32, z: i32) -> f32 {
        let n = (x.wrapping_mul(374761393) as u32).wrapping_add(z.wrapping_mul(668265263) as u32);
        let n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        let n = n ^ (n >> 16);
        (n & 0x7FFFFFFF) as f32 / 0x7FFFFFFF as f32
    }

    /// Generate LOD data procedurally by sampling terrain_height.
    /// Returns None if the entire LOD node is air.
    pub(crate) fn generate_lod_data_static(key: &LodNodeKey) -> Option<world::lod::VoxelData> {
        // Each LOD node at level L covers 2^L chunks per axis = 2^L * CHUNK_SIZE voxels per axis.
        // The 32^3 grid maps each cell to (2^L) real voxels per axis.
        let real_voxels_per_cell = 1i32 << key.lod_level;
        let chunks_per_axis = 1i32 << key.lod_level;
        let chunk_size = CHUNK_SIZE as i32;

        // World voxel origin of this LOD node
        let origin_vx = key.x * chunks_per_axis * chunk_size;
        let origin_vy = key.y * chunks_per_axis * chunk_size;
        let origin_vz = key.z * chunks_per_axis * chunk_size;

        // Quick reject: if entire node is above max terrain, it's all air
        if origin_vy > Self::MAX_TERRAIN_VOXEL_HEIGHT {
            return None;
        }

        let mut result = Box::new([[[0u8; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]);
        let mut has_solid = false;

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                // Sample terrain_height at the center of this LOD cell's XZ footprint
                let wx = origin_vx + lx as i32 * real_voxels_per_cell + real_voxels_per_cell / 2;
                let wz = origin_vz + lz as i32 * real_voxels_per_cell + real_voxels_per_cell / 2;
                let height = Self::terrain_height(wx, wz);

                for ly in 0..CHUNK_SIZE {
                    let wy =
                        origin_vy + ly as i32 * real_voxels_per_cell + real_voxels_per_cell / 2;

                    if wy >= height {
                        break; // Rest of column is air
                    }

                    // Assign material based on depth below surface (same logic as generate_chunk_data_static)
                    let depth = height - wy;
                    let v = if height > 700 && depth <= 2 {
                        15 // Snow
                    } else if height > 500 && depth <= 1 {
                        42 // Mountain rock
                    } else if depth <= 1 {
                        49 // Grass
                    } else if depth <= 5 {
                        33 // Dirt
                    } else if depth <= 20 {
                        42 // Stone
                    } else {
                        44 // Dark stone
                    };
                    result[lx][ly][lz] = v;
                    has_solid = true;
                }
            }
        }

        if !has_solid {
            return None;
        }

        Some(world::lod::VoxelData::Full(result))
    }
}

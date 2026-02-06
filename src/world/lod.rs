use crate::voxel::{CHUNK_SIZE, Voxel};

/// LOD level: 0 = full resolution, higher = coarser
pub type LodLevel = u8;

/// LOD configuration
#[derive(Clone, Debug)]
pub struct LodConfig {
    /// Number of LOD levels (recommend 4)
    pub levels: u8,

    /// Distance thresholds in world units (meters)
    /// Index 0 = max distance for LOD0, etc.
    pub distances: [f32; 4],

    /// Voxel resolution at each level
    pub resolutions: [usize; 4],
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            levels: 4,
            distances: [
                32.0,  // LOD0 -> LOD1 at 32m
                96.0,  // LOD1 -> LOD2 at 96m
                192.0, // LOD2 -> LOD3 at 192m
                384.0, // LOD3 -> cull beyond 384m
            ],
            resolutions: [32, 16, 8, 4],
        }
    }
}

impl LodConfig {
    /// Get appropriate LOD level for given distance
    pub fn lod_for_distance(&self, distance: f32) -> LodLevel {
        for (i, &threshold) in self.distances.iter().enumerate() {
            if distance < threshold {
                return i as LodLevel;
            }
        }
        self.levels - 1
    }

    /// Get resolution for LOD level
    pub fn resolution(&self, lod: LodLevel) -> usize {
        self.resolutions[lod as usize].min(CHUNK_SIZE)
    }

    /// Get scale factor (how many voxels are merged)
    pub fn scale_factor(&self, lod: LodLevel) -> usize {
        CHUNK_SIZE / self.resolution(lod)
    }
}

/// Voxel data storage at different LOD levels
#[derive(Clone)]
pub enum VoxelData {
    /// Full resolution chunk (32^3 = 32768 bytes)
    Full(Box<[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]>),

    /// LOD1: 16^3 = 4096 bytes (2x2x2 voxels merged)
    Lod1(Box<[[[Voxel; 16]; 16]; 16]>),

    /// LOD2: 8^3 = 512 bytes (4x4x4 voxels merged)
    Lod2(Box<[[[Voxel; 8]; 8]; 8]>),

    /// LOD3: 4^3 = 64 bytes (8x8x8 voxels merged)
    Lod3(Box<[[[Voxel; 4]; 4]; 4]>),

    /// Homogeneous: all voxels same value
    Homogeneous(Voxel),
}

impl VoxelData {
    /// Create new full resolution data filled with air
    pub fn new_full() -> Self {
        Self::Full(Box::new([[[0; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]))
    }

    /// Create from existing chunk data
    pub fn from_full(data: [[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]) -> Self {
        // Check if homogeneous
        let first = data[0][0][0];
        let is_homogeneous = data
            .iter()
            .flat_map(|plane| plane.iter())
            .flat_map(|row| row.iter())
            .all(|&v| v == first);

        if is_homogeneous {
            Self::Homogeneous(first)
        } else {
            Self::Full(Box::new(data))
        }
    }

    /// Get voxel at local coordinates
    pub fn get(&self, x: usize, y: usize, z: usize) -> Voxel {
        match self {
            Self::Full(data) => {
                if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
                    data[x][y][z]
                } else {
                    0
                }
            }
            Self::Lod1(data) => {
                let lx = x / 2;
                let ly = y / 2;
                let lz = z / 2;
                if lx < 16 && ly < 16 && lz < 16 {
                    data[lx][ly][lz]
                } else {
                    0
                }
            }
            Self::Lod2(data) => {
                let lx = x / 4;
                let ly = y / 4;
                let lz = z / 4;
                if lx < 8 && ly < 8 && lz < 8 {
                    data[lx][ly][lz]
                } else {
                    0
                }
            }
            Self::Lod3(data) => {
                let lx = x / 8;
                let ly = y / 8;
                let lz = z / 8;
                if lx < 4 && ly < 4 && lz < 4 {
                    data[lx][ly][lz]
                } else {
                    0
                }
            }
            Self::Homogeneous(v) => *v,
        }
    }

    /// Set voxel at local coordinates (only for Full resolution)
    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: Voxel) {
        if let Self::Full(data) = self {
            if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
                data[x][y][z] = voxel;
            }
        }
    }

    /// Get LOD level of this data
    pub fn lod_level(&self) -> LodLevel {
        match self {
            Self::Full(_) => 0,
            Self::Lod1(_) => 1,
            Self::Lod2(_) => 2,
            Self::Lod3(_) => 3,
            Self::Homogeneous(_) => 0, // Can be used at any level
        }
    }

    /// Get resolution of this data
    pub fn resolution(&self) -> usize {
        match self {
            Self::Full(_) => 32,
            Self::Lod1(_) => 16,
            Self::Lod2(_) => 8,
            Self::Lod3(_) => 4,
            Self::Homogeneous(_) => 1,
        }
    }

    /// Check if data is homogeneous, return the value if so
    pub fn is_homogeneous(&self) -> Option<Voxel> {
        match self {
            Self::Homogeneous(v) => Some(*v),
            Self::Full(data) => {
                let first = data[0][0][0];
                let all_same = data
                    .iter()
                    .flat_map(|plane| plane.iter())
                    .flat_map(|row| row.iter())
                    .all(|&v| v == first);
                if all_same { Some(first) } else { None }
            }
            _ => None,
        }
    }

    /// Get memory size in bytes
    pub fn memory_size(&self) -> usize {
        match self {
            Self::Full(_) => 32 * 32 * 32,
            Self::Lod1(_) => 16 * 16 * 16,
            Self::Lod2(_) => 8 * 8 * 8,
            Self::Lod3(_) => 4 * 4 * 4,
            Self::Homogeneous(_) => 1,
        }
    }

    /// Downsample to lower LOD level
    pub fn downsample(&self, target_lod: LodLevel) -> Self {
        match (self, target_lod) {
            (Self::Full(data), 1) => Self::Lod1(Box::new(downsample_32_to_16(data))),
            (Self::Full(data), 2) => Self::Lod2(Box::new(downsample_32_to_8(data))),
            (Self::Full(data), 3) => Self::Lod3(Box::new(downsample_32_to_4(data))),
            (Self::Lod1(data), 2) => Self::Lod2(Box::new(downsample_16_to_8(data))),
            (Self::Lod1(data), 3) => Self::Lod3(Box::new(downsample_16_to_4(data))),
            (Self::Lod2(data), 3) => Self::Lod3(Box::new(downsample_8_to_4(data))),
            (Self::Homogeneous(v), _) => Self::Homogeneous(*v),
            _ => self.clone(), // Already at target or lower
        }
    }

    /// Get raw data as slice for Full resolution (for meshing)
    pub fn as_full(&self) -> Option<&[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]> {
        match self {
            Self::Full(data) => Some(data),
            _ => None,
        }
    }

    /// Fill with ground (for terrain generation)
    pub fn fill_ground(&mut self, height: usize, voxel: Voxel) {
        if let Self::Full(data) = self {
            for x in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    for y in 0..height.min(CHUNK_SIZE) {
                        data[x][y][z] = voxel;
                    }
                }
            }
        }
    }
}

impl Default for VoxelData {
    fn default() -> Self {
        Self::Homogeneous(0) // Air
    }
}

/// Sample a 2x2x2 region and return dominant voxel
/// Biased towards solid voxels to prevent holes
fn sample_region_2x2x2<const N: usize>(
    data: &[[[Voxel; N]; N]; N],
    sx: usize,
    sy: usize,
    sz: usize,
) -> Voxel {
    let mut counts = [0u8; 256];
    let mut solid_count = 0u32;

    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                let x = sx + dx;
                let y = sy + dy;
                let z = sz + dz;
                if x < N && y < N && z < N {
                    let v = data[x][y][z];
                    counts[v as usize] += 1;
                    if v != 0 {
                        solid_count += 1;
                    }
                }
            }
        }
    }

    // If at least 2 solid voxels (25%), find most common solid
    if solid_count >= 2 {
        let mut best = 1u8;
        let mut best_count = 0u8;
        for i in 1..=255 {
            if counts[i] > best_count {
                best_count = counts[i];
                best = i as u8;
            }
        }
        best
    } else {
        0 // Air
    }
}

/// Sample a 4x4x4 region
fn sample_region_4x4x4<const N: usize>(
    data: &[[[Voxel; N]; N]; N],
    sx: usize,
    sy: usize,
    sz: usize,
) -> Voxel {
    let mut counts = [0u16; 256];
    let mut solid_count = 0u32;

    for dx in 0..4 {
        for dy in 0..4 {
            for dz in 0..4 {
                let x = sx + dx;
                let y = sy + dy;
                let z = sz + dz;
                if x < N && y < N && z < N {
                    let v = data[x][y][z];
                    counts[v as usize] += 1;
                    if v != 0 {
                        solid_count += 1;
                    }
                }
            }
        }
    }

    // If at least 16 solid voxels (25%), find most common solid
    if solid_count >= 16 {
        let mut best = 1u8;
        let mut best_count = 0u16;
        for i in 1..=255 {
            if counts[i] > best_count {
                best_count = counts[i];
                best = i as u8;
            }
        }
        best
    } else {
        0 // Air
    }
}

/// Sample an 8x8x8 region
fn sample_region_8x8x8(
    data: &[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
    sx: usize,
    sy: usize,
    sz: usize,
) -> Voxel {
    let mut counts = [0u16; 256];
    let mut solid_count = 0u32;

    for dx in 0..8 {
        for dy in 0..8 {
            for dz in 0..8 {
                let x = sx + dx;
                let y = sy + dy;
                let z = sz + dz;
                if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
                    let v = data[x][y][z];
                    counts[v as usize] += 1;
                    if v != 0 {
                        solid_count += 1;
                    }
                }
            }
        }
    }

    // If at least 128 solid voxels (25%), find most common solid
    if solid_count >= 128 {
        let mut best = 1u8;
        let mut best_count = 0u16;
        for i in 1..=255 {
            if counts[i] > best_count {
                best_count = counts[i];
                best = i as u8;
            }
        }
        best
    } else {
        0 // Air
    }
}

fn downsample_32_to_16(
    data: &[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
) -> [[[Voxel; 16]; 16]; 16] {
    let mut result = [[[0u8; 16]; 16]; 16];
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                result[x][y][z] = sample_region_2x2x2(data, x * 2, y * 2, z * 2);
            }
        }
    }
    result
}

fn downsample_32_to_8(
    data: &[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
) -> [[[Voxel; 8]; 8]; 8] {
    let mut result = [[[0u8; 8]; 8]; 8];
    for x in 0..8 {
        for y in 0..8 {
            for z in 0..8 {
                result[x][y][z] = sample_region_4x4x4(data, x * 4, y * 4, z * 4);
            }
        }
    }
    result
}

fn downsample_32_to_4(
    data: &[[[Voxel; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
) -> [[[Voxel; 4]; 4]; 4] {
    let mut result = [[[0u8; 4]; 4]; 4];
    for x in 0..4 {
        for y in 0..4 {
            for z in 0..4 {
                result[x][y][z] = sample_region_8x8x8(data, x * 8, y * 8, z * 8);
            }
        }
    }
    result
}

fn downsample_16_to_8(data: &[[[Voxel; 16]; 16]; 16]) -> [[[Voxel; 8]; 8]; 8] {
    let mut result = [[[0u8; 8]; 8]; 8];
    for x in 0..8 {
        for y in 0..8 {
            for z in 0..8 {
                result[x][y][z] = sample_region_2x2x2(data, x * 2, y * 2, z * 2);
            }
        }
    }
    result
}

fn downsample_16_to_4(data: &[[[Voxel; 16]; 16]; 16]) -> [[[Voxel; 4]; 4]; 4] {
    let mut result = [[[0u8; 4]; 4]; 4];
    for x in 0..4 {
        for y in 0..4 {
            for z in 0..4 {
                result[x][y][z] = sample_region_4x4x4(data, x * 4, y * 4, z * 4);
            }
        }
    }
    result
}

fn downsample_8_to_4(data: &[[[Voxel; 8]; 8]; 8]) -> [[[Voxel; 4]; 4]; 4] {
    let mut result = [[[0u8; 4]; 4]; 4];
    for x in 0..4 {
        for y in 0..4 {
            for z in 0..4 {
                result[x][y][z] = sample_region_2x2x2(data, x * 2, y * 2, z * 2);
            }
        }
    }
    result
}

/// Build LOD data from 8 child nodes arranged in octree order.
///
/// Each child is a 32^3 VoxelData (or None for missing/unloaded children treated as air).
/// The 8 children are arranged in a virtual 64^3 grid and downsampled 2x to produce 32^3 output.
///
/// Octant layout: bit 0 = +X, bit 1 = +Y, bit 2 = +Z
///   octant 0: (0,0,0)  octant 1: (1,0,0)  octant 2: (0,1,0)  octant 3: (1,1,0)
///   octant 4: (0,0,1)  octant 5: (1,0,1)  octant 6: (0,1,1)  octant 7: (1,1,1)
pub fn build_lod_from_children(children: &[Option<&VoxelData>; 8]) -> VoxelData {
    // Fast path: check if all children are homogeneous with the same value
    let mut all_same = true;
    let mut common_value: Option<Voxel> = None;

    for child in children.iter() {
        match child {
            None => {
                // Missing child = air
                match common_value {
                    None => common_value = Some(0),
                    Some(v) if v == 0 => {}
                    _ => {
                        all_same = false;
                        break;
                    }
                }
            }
            Some(VoxelData::Homogeneous(v)) => match common_value {
                None => common_value = Some(*v),
                Some(cv) if cv == *v => {}
                _ => {
                    all_same = false;
                    break;
                }
            },
            _ => {
                all_same = false;
                break;
            }
        }
    }

    if all_same {
        return VoxelData::Homogeneous(common_value.unwrap_or(0));
    }

    // Build Full(32^3) by sampling 2x2x2 from each child
    let mut result = Box::new([[[0u8; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]);

    for ox in 0..CHUNK_SIZE {
        for oy in 0..CHUNK_SIZE {
            for oz in 0..CHUNK_SIZE {
                // Determine which octant this output voxel maps to
                let octant = ((ox >= 16) as usize)
                    | (((oy >= 16) as usize) << 1)
                    | (((oz >= 16) as usize) << 2);

                // Local coordinates within the child's 32^3 space
                let cx = (ox % 16) * 2;
                let cy = (oy % 16) * 2;
                let cz = (oz % 16) * 2;

                result[ox][oy][oz] = match &children[octant] {
                    None => 0, // Air
                    Some(data) => sample_child_2x2x2(data, cx, cy, cz),
                };
            }
        }
    }

    VoxelData::Full(result)
}

/// Sample a 2x2x2 region from a VoxelData at the given start coordinates.
/// Returns the dominant voxel (biased toward solid).
fn sample_child_2x2x2(data: &VoxelData, sx: usize, sy: usize, sz: usize) -> Voxel {
    match data {
        VoxelData::Homogeneous(v) => *v,
        VoxelData::Full(arr) => sample_region_2x2x2(arr, sx, sy, sz),
        VoxelData::Lod1(arr) => {
            // Lod1 is 16^3, coordinates need scaling: divide by 2
            let lx = sx / 2;
            let ly = sy / 2;
            let lz = sz / 2;
            if lx < 16 && ly < 16 && lz < 16 {
                arr[lx][ly][lz]
            } else {
                0
            }
        }
        VoxelData::Lod2(arr) => {
            let lx = sx / 4;
            let ly = sy / 4;
            let lz = sz / 4;
            if lx < 8 && ly < 8 && lz < 8 {
                arr[lx][ly][lz]
            } else {
                0
            }
        }
        VoxelData::Lod3(arr) => {
            let lx = sx / 8;
            let ly = sy / 8;
            let lz = sz / 8;
            if lx < 4 && ly < 4 && lz < 4 {
                arr[lx][ly][lz]
            } else {
                0
            }
        }
    }
}

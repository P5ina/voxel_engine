use serde::{Deserialize, Serialize};

use super::block::Voxel;

pub const CHUNK_SIZE: usize = 32;
/// Scale factor for voxel size in world units.
/// 1/2 = 0.5, meaning 2 voxels per meter.
pub const VOXEL_SCALE: f32 = 1.0 / 2.0;

/// Air voxel (empty space)
pub const AIR: Voxel = 0;

/// Allocate a zeroed `[[[u8; 32]; 32]; 32]` directly on the heap,
/// avoiding the 32KB stack temporary that `Box::new([[[0u8; 32]; 32]; 32])` creates.
pub fn boxed_zero_chunk_data() -> Box<[[[u8; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]> {
    let v: Vec<u8> = vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
    let raw =
        Box::into_raw(v.into_boxed_slice()) as *mut [[[u8; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE];
    // SAFETY: [[[u8; 32]; 32]; 32] has identical layout to [u8; 32768],
    // and the Vec was zero-initialized with the correct total size.
    unsafe { Box::from_raw(raw) }
}

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

/// A section is a 32^3 chunk (alias for clarity in column context).
pub type Section = Chunk;

/// Number of sections in a column (1024 voxels / 32 = 32).
pub const NUM_SECTIONS: usize = 32;

/// A vertical column of up to NUM_SECTIONS sparse sections.
/// Each section is 32^3 voxels; the column spans the full world height (1024 voxels).
pub struct Column {
    sections: [Option<Box<Section>>; NUM_SECTIONS],
}

impl Column {
    pub fn new() -> Self {
        Self {
            sections: std::array::from_fn(|_| None),
        }
    }

    /// Get a reference to a section by Y index.
    #[inline]
    pub fn get_section(&self, y: u8) -> Option<&Section> {
        self.sections.get(y as usize).and_then(|s| s.as_deref())
    }

    /// Get a mutable reference to a section by Y index.
    #[inline]
    pub fn get_section_mut(&mut self, y: u8) -> Option<&mut Section> {
        self.sections
            .get_mut(y as usize)
            .and_then(|s| s.as_deref_mut())
    }

    /// Set a section at the given Y index. Takes ownership of the section.
    pub fn set_section(&mut self, y: u8, section: Section) {
        if (y as usize) < NUM_SECTIONS {
            self.sections[y as usize] = Some(Box::new(section));
        }
    }

    /// Remove a section at the given Y index, returning it if present.
    pub fn remove_section(&mut self, y: u8) -> Option<Section> {
        if (y as usize) < NUM_SECTIONS {
            self.sections[y as usize].take().map(|b| *b)
        } else {
            None
        }
    }

    /// Check if a section exists at the given Y index.
    #[inline]
    pub fn has_section(&self, y: u8) -> bool {
        self.sections.get(y as usize).is_some_and(|s| s.is_some())
    }

    /// Get a voxel at local x, world-voxel y, local z within this column.
    #[inline]
    pub fn get_voxel(&self, lx: usize, wy: i32, lz: usize) -> Voxel {
        if wy < 0 {
            return AIR;
        }
        let section_y = (wy as usize) / CHUNK_SIZE;
        let local_y = (wy as usize) % CHUNK_SIZE;
        if section_y >= NUM_SECTIONS {
            return AIR;
        }
        self.sections[section_y]
            .as_ref()
            .map(|s| s.get(lx, local_y, lz))
            .unwrap_or(AIR)
    }

    /// Set a voxel at local x, world-voxel y, local z within this column.
    #[inline]
    pub fn set_voxel(&mut self, lx: usize, wy: i32, lz: usize, voxel: Voxel) {
        if wy < 0 {
            return;
        }
        let section_y = (wy as usize) / CHUNK_SIZE;
        let local_y = (wy as usize) % CHUNK_SIZE;
        if section_y >= NUM_SECTIONS {
            return;
        }
        if self.sections[section_y].is_none() {
            self.sections[section_y] = Some(Box::new(Section::new()));
        }
        self.sections[section_y]
            .as_mut()
            .unwrap()
            .set(lx, local_y, lz, voxel);
    }

    /// Check if all sections are either absent or empty (all air).
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|s| match s {
            None => true,
            Some(section) => section.is_empty(),
        })
    }

    /// Count of non-None sections.
    pub fn section_count(&self) -> usize {
        self.sections.iter().filter(|s| s.is_some()).count()
    }

    /// Iterate over populated sections: yields (section_y, &Section).
    pub fn sections_iter(&self) -> impl Iterator<Item = (u8, &Section)> {
        self.sections
            .iter()
            .enumerate()
            .filter_map(|(y, s)| s.as_deref().map(|sec| (y as u8, sec)))
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Column {
    fn clone(&self) -> Self {
        Self {
            sections: std::array::from_fn(|i| self.sections[i].clone()),
        }
    }
}

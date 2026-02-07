#![cfg_attr(not(feature = "dev-tools"), allow(dead_code))]

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::bigworld_io::BigWorldError;
use super::chunk_manager::ChunkManager;
use super::octree::VoxelOctree;
use super::position::{ChunkPosition, RegionCoord};
use crate::voxel::chunk::Chunk;

// ============================================================================
// Region file format
// ============================================================================

const REGION_MAGIC: &[u8; 8] = b"VXREGION";
const REGION_VERSION: u32 = 2;

/// RLE compressed section within a column.
#[derive(Serialize, Deserialize)]
struct CompressedSection {
    section_y: u8,
    rle_data: Vec<(u16, u8)>,
}

/// A compressed column: XZ position + per-section RLE data.
#[derive(Serialize, Deserialize)]
struct CompressedColumn {
    col: [i32; 2],
    sections: Vec<CompressedSection>,
}

fn compress_section(chunk: &Chunk) -> Vec<(u16, u8)> {
    let mut rle_data = Vec::new();
    let mut current_voxel = chunk.get(0, 0, 0);
    let mut count: u16 = 0;

    for z in 0..32 {
        for y in 0..32 {
            for x in 0..32 {
                let voxel = chunk.get(x, y, z);
                if voxel == current_voxel && count < u16::MAX {
                    count += 1;
                } else {
                    if count > 0 {
                        rle_data.push((count, current_voxel));
                    }
                    current_voxel = voxel;
                    count = 1;
                }
            }
        }
    }
    if count > 0 {
        rle_data.push((count, current_voxel));
    }

    rle_data
}

fn decompress_section(rle_data: &[(u16, u8)]) -> Chunk {
    let mut chunk = Chunk::new();
    let mut idx = 0usize;

    for &(count, voxel) in rle_data {
        for _ in 0..count {
            let x = idx % 32;
            let y = (idx / 32) % 32;
            let z = idx / 1024;
            chunk.set(x, y, z, voxel);
            idx += 1;
        }
    }

    chunk
}

/// Write a single region file to disk.
/// Accepts flat (ChunkPosition, &Chunk) pairs and groups them into columns internally.
pub fn write_region(
    world_dir: &Path,
    coord: RegionCoord,
    chunks: &[(ChunkPosition, &Chunk)],
) -> Result<(), BigWorldError> {
    let regions_dir = world_dir.join("regions");
    std::fs::create_dir_all(&regions_dir)?;

    let path = coord.file_path(world_dir);

    // Group chunks by column (XZ)
    let mut column_map: HashMap<[i32; 2], Vec<(u8, &Chunk)>> = HashMap::new();
    for (pos, chunk) in chunks {
        column_map
            .entry([pos.x, pos.z])
            .or_default()
            .push((pos.y as u8, chunk));
    }

    // Compress columns (parallel over columns)
    let column_entries: Vec<_> = column_map.into_iter().collect();
    let compressed: Vec<CompressedColumn> = column_entries
        .par_iter()
        .map(|(col, sections)| {
            let compressed_sections: Vec<CompressedSection> = sections
                .iter()
                .map(|(sy, chunk)| CompressedSection {
                    section_y: *sy,
                    rle_data: compress_section(chunk),
                })
                .collect();
            CompressedColumn {
                col: *col,
                sections: compressed_sections,
            }
        })
        .collect();

    let file = std::fs::File::create(&path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(REGION_MAGIC)?;
    writer.write_all(&REGION_VERSION.to_le_bytes())?;
    writer.write_all(&coord.rx.to_le_bytes())?;
    writer.write_all(&coord.rz.to_le_bytes())?;
    writer.write_all(&(compressed.len() as u32).to_le_bytes())?;

    let mut encoder = lz4_flex::frame::FrameEncoder::new(writer);
    bincode::serialize_into(&mut encoder, &compressed).map_err(BigWorldError::Bincode)?;
    let mut writer = encoder.finish().map_err(|e| BigWorldError::Io(e.into()))?;
    writer.flush()?;

    Ok(())
}

/// Read a single region file from disk.
/// Returns flat (ChunkPosition, Chunk) pairs for compatibility with callers.
pub fn read_region(
    world_dir: &Path,
    coord: RegionCoord,
) -> Result<Vec<(ChunkPosition, Chunk)>, BigWorldError> {
    let path = coord.file_path(world_dir);

    let file = std::fs::File::open(&path)?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != REGION_MAGIC {
        return Err(BigWorldError::InvalidMagic);
    }

    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != REGION_VERSION {
        return Err(BigWorldError::UnsupportedVersion(version));
    }

    // Skip rx, rz, column_count (we already know coord)
    let mut _buf = [0u8; 4];
    reader.read_exact(&mut _buf)?; // rx
    reader.read_exact(&mut _buf)?; // rz
    reader.read_exact(&mut _buf)?; // column_count

    let decoder = lz4_flex::frame::FrameDecoder::new(reader);
    let compressed: Vec<CompressedColumn> =
        bincode::deserialize_from(decoder).map_err(BigWorldError::Bincode)?;

    // Flatten columns back to (ChunkPosition, Chunk) pairs
    let chunks: Vec<_> = compressed
        .par_iter()
        .flat_map_iter(|col| {
            col.sections.iter().map(move |sec| {
                let pos = ChunkPosition::new(col.col[0], sec.section_y as i32, col.col[1]);
                let chunk = decompress_section(&sec.rle_data);
                (pos, chunk)
            })
        })
        .collect();
    Ok(chunks)
}

// ============================================================================
// World meta file format
// ============================================================================

const META_MAGIC: &[u8; 8] = b"REGWORLD";
const META_VERSION: u32 = 1;

/// Metadata stored in world.meta alongside the octree.
#[derive(Serialize, Deserialize)]
pub struct WorldMeta {
    pub version: u32,
    pub world_size: [i32; 3],
    pub region_size: i32,
    pub spawn_position: [f32; 3],
}

/// Serializable reference wrapper — borrows octree for saving.
#[derive(Serialize)]
struct WorldMetaFileRef<'a> {
    meta: WorldMeta,
    octree: &'a VoxelOctree,
}

/// Owned version for deserialization.
#[derive(Deserialize)]
struct WorldMetaFile {
    meta: WorldMeta,
    octree: VoxelOctree,
}

/// Save world.meta (header + octree with LOD data).
pub fn save_world_meta(
    world_dir: &Path,
    octree: &VoxelOctree,
    spawn_position: [f32; 3],
) -> Result<(), BigWorldError> {
    std::fs::create_dir_all(world_dir)?;
    let path = world_dir.join("world.meta");

    let world_size = [
        octree.world_max.x - octree.world_min.x + 1,
        octree.world_max.y - octree.world_min.y + 1,
        octree.world_max.z - octree.world_min.z + 1,
    ];

    let data = WorldMetaFileRef {
        meta: WorldMeta {
            version: META_VERSION,
            world_size,
            region_size: super::position::REGION_SIZE,
            spawn_position,
        },
        octree,
    };

    let file = std::fs::File::create(&path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(META_MAGIC)?;
    writer.write_all(&META_VERSION.to_le_bytes())?;

    let mut encoder = lz4_flex::frame::FrameEncoder::new(writer);
    bincode::serialize_into(&mut encoder, &data).map_err(BigWorldError::Bincode)?;
    let mut writer = encoder.finish().map_err(|e| BigWorldError::Io(e.into()))?;
    writer.flush()?;

    Ok(())
}

/// Result of loading world.meta.
pub struct LoadedWorldMeta {
    pub meta: WorldMeta,
    pub octree: VoxelOctree,
}

/// Load world.meta from disk.
pub fn load_world_meta(world_dir: &Path) -> Result<LoadedWorldMeta, BigWorldError> {
    let path = world_dir.join("world.meta");

    let file = std::fs::File::open(&path)?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != META_MAGIC {
        return Err(BigWorldError::InvalidMagic);
    }

    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != META_VERSION {
        return Err(BigWorldError::UnsupportedVersion(version));
    }

    let decoder = lz4_flex::frame::FrameDecoder::new(reader);
    let data: WorldMetaFile = bincode::deserialize_from(decoder).map_err(BigWorldError::Bincode)?;

    Ok(LoadedWorldMeta {
        meta: data.meta,
        octree: data.octree,
    })
}

// ============================================================================
// RegionManager — lifecycle tracking for region load/unload
// ============================================================================

/// Region lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionState {
    Unloaded,
    Loading,
    Loaded,
}

/// Result of a background region load.
pub struct RegionLoadResult {
    pub coord: RegionCoord,
    pub chunks: Vec<(ChunkPosition, Chunk)>,
    pub error: Option<BigWorldError>,
}

/// Manages region lifecycle, dirty tracking, and background IO.
pub struct RegionManager {
    states: HashMap<RegionCoord, RegionState>,
    dirty_regions: HashSet<RegionCoord>,
    loading_inflight: HashSet<RegionCoord>,
    load_tx: mpsc::Sender<RegionLoadResult>,
    load_rx: mpsc::Receiver<RegionLoadResult>,
    world_dir: PathBuf,
}

/// Distances for proactive region loading (in world-space meters).
const REGION_LOAD_DISTANCE: f32 = 720.0;
const REGION_UNLOAD_DISTANCE: f32 = 900.0;

impl RegionManager {
    pub fn new(world_dir: PathBuf) -> Self {
        let (load_tx, load_rx) = mpsc::channel();
        Self {
            states: HashMap::new(),
            dirty_regions: HashSet::new(),
            loading_inflight: HashSet::new(),
            load_tx,
            load_rx,
            world_dir,
        }
    }

    pub fn world_dir(&self) -> &Path {
        &self.world_dir
    }

    pub fn is_loaded(&self, coord: RegionCoord) -> bool {
        self.states.get(&coord) == Some(&RegionState::Loaded)
    }

    pub fn is_loading(&self, coord: RegionCoord) -> bool {
        self.states.get(&coord) == Some(&RegionState::Loading)
    }

    /// Mark a region as dirty (has unsaved edits).
    pub fn mark_dirty(&mut self, coord: RegionCoord) {
        self.dirty_regions.insert(coord);
    }

    /// Mark a region as loaded (called when background load completes).
    pub fn mark_loaded(&mut self, coord: RegionCoord) {
        self.states.insert(coord, RegionState::Loaded);
        self.loading_inflight.remove(&coord);
    }

    /// Mark a region as unloaded.
    pub fn unload(&mut self, coord: RegionCoord) {
        self.states.insert(coord, RegionState::Unloaded);
        self.loading_inflight.remove(&coord);
    }

    /// Spawn a background task to load a region from disk.
    pub fn request_load(&mut self, coord: RegionCoord) {
        if self.loading_inflight.contains(&coord) || self.is_loaded(coord) {
            return;
        }
        self.states.insert(coord, RegionState::Loading);
        self.loading_inflight.insert(coord);

        let tx = self.load_tx.clone();
        let world_dir = self.world_dir.clone();
        rayon::spawn(move || {
            let result = if coord.file_path(&world_dir).exists() {
                match read_region(&world_dir, coord) {
                    Ok(chunks) => RegionLoadResult {
                        coord,
                        chunks,
                        error: None,
                    },
                    Err(e) => RegionLoadResult {
                        coord,
                        chunks: Vec::new(),
                        error: Some(e),
                    },
                }
            } else {
                // No file on disk — region is all air
                RegionLoadResult {
                    coord,
                    chunks: Vec::new(),
                    error: None,
                }
            };
            let _ = tx.send(result);
        });
    }

    /// Poll for completed background loads.
    pub fn poll_loads(&mut self) -> Vec<RegionLoadResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.load_rx.try_recv() {
            self.loading_inflight.remove(&result.coord);
            results.push(result);
        }
        results
    }

    /// Compute which regions should be loaded based on player XZ position.
    pub fn desired_regions(&self, player_x: f32, player_z: f32) -> HashSet<RegionCoord> {
        use crate::voxel::chunk::{CHUNK_SIZE, VOXEL_SCALE};
        let region_world_size =
            super::position::REGION_SIZE as f32 * CHUNK_SIZE as f32 * VOXEL_SCALE;
        let max_radius = (REGION_LOAD_DISTANCE / region_world_size).ceil() as i32 + 1;

        // Player's region
        let player_chunk = ChunkPosition::from_world_pos(player_x, 0.0, player_z);
        let player_region = RegionCoord::from_chunk_pos(player_chunk);

        let mut result = HashSet::new();
        for drx in -max_radius..=max_radius {
            for drz in -max_radius..=max_radius {
                let coord = RegionCoord::new(player_region.rx + drx, player_region.rz + drz);
                let (cx, cz) = coord.center_world_pos();
                let dx = cx - player_x;
                let dz = cz - player_z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist <= REGION_LOAD_DISTANCE {
                    result.insert(coord);
                }
            }
        }
        result
    }

    /// Compute which loaded regions should be unloaded (beyond unload distance).
    pub fn regions_to_unload(&self, player_x: f32, player_z: f32) -> Vec<RegionCoord> {
        let mut to_unload = Vec::new();
        for (&coord, &state) in &self.states {
            if state != RegionState::Loaded {
                continue;
            }
            let (cx, cz) = coord.center_world_pos();
            let dx = cx - player_x;
            let dz = cz - player_z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > REGION_UNLOAD_DISTANCE {
                to_unload.push(coord);
            }
        }
        to_unload
    }

    /// Save all dirty regions to disk from the given world.
    /// Returns the set of regions that were saved.
    pub fn save_dirty_regions(&mut self, world: &ChunkManager) -> HashSet<RegionCoord> {
        let dirty: Vec<RegionCoord> = self.dirty_regions.drain().collect();
        let saved: HashSet<RegionCoord> = HashSet::new();
        for coord in &dirty {
            let chunks = world.chunks_in_region(*coord);
            if chunks.is_empty() {
                // No chunks to save — delete the region file if it exists
                let path = coord.file_path(&self.world_dir);
                let _ = std::fs::remove_file(path);
            } else if let Err(e) = write_region(&self.world_dir, *coord, &chunks) {
                log::error!("[Region] Failed to save region {:?}: {}", coord, e);
                // Re-mark as dirty so we try again next save
                self.dirty_regions.insert(*coord);
            }
        }
        saved
    }

    /// Mark all regions that currently have chunks in the world as loaded.
    pub fn mark_existing_regions_loaded(&mut self, world: &ChunkManager) {
        for pos in world.chunk_positions() {
            let coord = RegionCoord::from_chunk_pos(pos);
            self.states.insert(coord, RegionState::Loaded);
        }
    }
}

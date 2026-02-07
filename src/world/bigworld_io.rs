#![cfg_attr(not(feature = "dev-tools"), allow(dead_code))]

//! Big World save/load with pre-computed meshes
//!
//! File format (.bigworld):
//! - Header: magic, version, world size, mesh count
//! - Chunk data: compressed voxel data
//! - Mesh data: pre-computed vertices for each mesh group

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::ChunkPosition;
use super::chunk_manager::ChunkManager;
use super::octree::VoxelOctree;
use crate::voxel::Chunk;

/// Compute world size in chunks from ChunkManager bounds.
fn world_size(world: &mut ChunkManager) -> (i32, i32, i32) {
    match world.bounds() {
        Some((min, max)) => (max.x - min.x + 1, max.y - min.y + 1, max.z - min.z + 1),
        None => (0, 0, 0),
    }
}

const MAGIC_FAST: &[u8; 8] = b"BIGWFAST";
const VERSION: u32 = 1;
const VERSION_FAST_STREAMING: u32 = 2;

#[derive(Debug)]
pub enum BigWorldError {
    Io(std::io::Error),
    InvalidMagic,
    UnsupportedVersion(u32),
    Bincode(bincode::Error),
    Lz4(lz4_flex::block::DecompressError),
}

impl std::fmt::Display for BigWorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BigWorldError::Io(e) => write!(f, "IO error: {}", e),
            BigWorldError::InvalidMagic => write!(f, "Invalid file magic"),
            BigWorldError::UnsupportedVersion(v) => write!(f, "Unsupported version: {}", v),
            BigWorldError::Bincode(e) => write!(f, "Serialization error: {}", e),
            BigWorldError::Lz4(e) => write!(f, "Decompression error: {}", e),
        }
    }
}

impl std::error::Error for BigWorldError {}

impl From<std::io::Error> for BigWorldError {
    fn from(e: std::io::Error) -> Self {
        BigWorldError::Io(e)
    }
}

impl From<bincode::Error> for BigWorldError {
    fn from(e: bincode::Error) -> Self {
        BigWorldError::Bincode(e)
    }
}

impl From<lz4_flex::block::DecompressError> for BigWorldError {
    fn from(e: lz4_flex::block::DecompressError) -> Self {
        BigWorldError::Lz4(e)
    }
}

/// RLE compressed chunk data
#[derive(Serialize, Deserialize)]
struct CompressedChunk {
    pos: [i32; 3],
    /// RLE: pairs of (count, voxel_id)
    rle_data: Vec<(u16, u8)>,
}

impl CompressedChunk {
    fn from_chunk(pos: ChunkPosition, chunk: &Chunk) -> Self {
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

        Self {
            pos: [pos.x, pos.y, pos.z],
            rle_data,
        }
    }

    fn to_chunk(&self) -> (ChunkPosition, Chunk) {
        let mut chunk = Chunk::new();
        let mut idx = 0usize;

        for &(count, voxel) in &self.rle_data {
            for _ in 0..count {
                let x = idx % 32;
                let y = (idx / 32) % 32;
                let z = idx / 1024;
                chunk.set(x, y, z, voxel);
                idx += 1;
            }
        }

        (
            ChunkPosition::new(self.pos[0], self.pos[1], self.pos[2]),
            chunk,
        )
    }
}

// ============================================================================
// Save/load with octree - chunks + octree, meshes built on load
// ============================================================================

/// Header for world file with octree
#[derive(Serialize, Deserialize)]
struct FastWorldHeader {
    world_size_x: i32,
    world_size_y: i32,
    world_size_z: i32,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
}

/// World data: chunks + octree
#[derive(Serialize, Deserialize)]
struct FastWorldData {
    header: FastWorldHeader,
    chunk_data: Vec<CompressedChunk>,
    octree: VoxelOctree,
}

/// Serializable reference wrapper — borrows octree instead of cloning it
#[derive(Serialize)]
struct FastWorldDataRef<'a> {
    header: FastWorldHeader,
    chunk_data: Vec<CompressedChunk>,
    octree: &'a VoxelOctree,
}

/// Pre-compressed save data (cheap to move across threads).
pub struct PreparedSaveData {
    compressed_chunks: Vec<CompressedChunk>,
    world_size: (i32, i32, i32),
}

/// Compress chunks from a ChunkManager for later saving.
/// Fast (parallel RLE compression), designed to run on the main thread.
pub fn prepare_save(world: &mut ChunkManager) -> PreparedSaveData {
    let chunks: Vec<_> = world.chunks().collect();
    let compressed_chunks: Vec<CompressedChunk> = chunks
        .par_iter()
        .map(|(pos, chunk)| CompressedChunk::from_chunk(*pos, chunk))
        .collect();
    PreparedSaveData {
        compressed_chunks,
        world_size: world_size(world),
    }
}

/// Write a prepared save to disk (streaming serialization).
/// Designed to run on a background thread with an owned octree.
pub fn write_prepared_save(
    path: impl AsRef<Path>,
    data: PreparedSaveData,
    octree: &VoxelOctree,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
) -> Result<(), BigWorldError> {
    let (size_x, size_y, size_z) = data.world_size;

    let data_ref = FastWorldDataRef {
        header: FastWorldHeader {
            world_size_x: size_x,
            world_size_y: size_y,
            world_size_z: size_z,
            mesh_group_size,
            spawn_position,
        },
        chunk_data: data.compressed_chunks,
        octree,
    };

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(MAGIC_FAST)?;
    writer.write_all(&VERSION_FAST_STREAMING.to_le_bytes())?;

    let mut encoder = lz4_flex::frame::FrameEncoder::new(writer);
    bincode::serialize_into(&mut encoder, &data_ref).map_err(BigWorldError::Bincode)?;
    let mut writer = encoder.finish().map_err(|e| BigWorldError::Io(e.into()))?;
    writer.flush()?;

    log::info!(
        "[BigWorld] Saved {} chunks + octree (streaming, background)",
        data_ref.chunk_data.len(),
    );

    Ok(())
}

/// Result of loading world with octree
pub struct LoadedChunks {
    pub world: ChunkManager,
    pub octree: VoxelOctree,
    pub spawn_position: [f32; 3],
}

/// Load world with octree (supports v1 block format and v2 streaming format)
pub fn load_big_world_fast(path: impl AsRef<Path>) -> Result<LoadedChunks, BigWorldError> {
    let path = path.as_ref();

    log::info!("[BigWorld] Reading file...");

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read and verify magic
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC_FAST {
        return Err(BigWorldError::InvalidMagic);
    }

    // Read version
    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);

    let data: FastWorldData = match version {
        VERSION => {
            // V1: block-compressed (read all, decompress, deserialize)
            let mut compressed = Vec::new();
            reader.read_to_end(&mut compressed)?;
            log::info!("[BigWorld] Decompressing (v1 block format)...");
            let decompressed = lz4_flex::decompress_size_prepended(&compressed)?;
            bincode::deserialize(&decompressed)?
        }
        VERSION_FAST_STREAMING => {
            // V2: stream-decompress directly from file (much lower memory)
            log::info!("[BigWorld] Stream-decompressing (v2 frame format)...");
            let decoder = lz4_flex::frame::FrameDecoder::new(reader);
            bincode::deserialize_from(decoder)?
        }
        _ => return Err(BigWorldError::UnsupportedVersion(version)),
    };

    log::info!(
        "[BigWorld] Loading {} chunks + octree...",
        data.chunk_data.len()
    );

    // Create world storage
    let mut world = ChunkManager::new();

    // Load chunks in parallel
    let chunks: Vec<_> = data.chunk_data.par_iter().map(|c| c.to_chunk()).collect();

    for (pos, chunk) in chunks {
        world.insert_chunk(pos, chunk);
    }

    log::info!(
        "[BigWorld] Loaded {} chunks, octree: {} nodes",
        world.chunk_count(),
        data.octree.node_count()
    );

    Ok(LoadedChunks {
        world,
        octree: data.octree,
        spawn_position: data.header.spawn_position,
    })
}

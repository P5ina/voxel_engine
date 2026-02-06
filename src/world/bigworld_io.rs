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
use crate::renderer::Vertex;
use crate::voxel::{Chunk, generate_merged_mesh_simple};

/// Compute world size in chunks from ChunkManager bounds.
fn world_size(world: &ChunkManager) -> (i32, i32, i32) {
    match world.bounds() {
        Some((min, max)) => (max.x - min.x + 1, max.y - min.y + 1, max.z - min.z + 1),
        None => (0, 0, 0),
    }
}

const MAGIC: &[u8; 8] = b"BIGWORLD";
const MAGIC_FAST: &[u8; 8] = b"BIGWFAST";
const VERSION: u32 = 1;

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

/// Header for big world file
#[derive(Serialize, Deserialize)]
struct BigWorldHeader {
    world_size_x: i32,
    world_size_y: i32,
    world_size_z: i32,
    mesh_group_size: i32,
    mesh_count: u32,
    spawn_position: [f32; 3],
}

/// Serializable vertex (same layout as Vertex but with Serialize/Deserialize)
#[derive(Serialize, Deserialize, Clone, Copy)]
struct SerVertex {
    position: [f32; 3],
    normal: [f32; 3],
    ao: f32,
    color_index: f32,
}

impl From<Vertex> for SerVertex {
    fn from(v: Vertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            ao: v.ao,
            color_index: v.color_index,
        }
    }
}

impl From<SerVertex> for Vertex {
    fn from(v: SerVertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            ao: v.ao,
            color_index: v.color_index,
        }
    }
}

/// Pre-computed mesh for a group of chunks
#[derive(Serialize, Deserialize)]
struct MeshData {
    base_chunk: [i32; 3],
    vertices: Vec<SerVertex>,
}

/// Data for saving/loading big world
#[derive(Serialize, Deserialize)]
struct BigWorldData {
    header: BigWorldHeader,
    /// Compressed chunk voxel data (RLE encoded)
    chunk_data: Vec<CompressedChunk>,
    /// Pre-computed meshes
    meshes: Vec<MeshData>,
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

/// Progress callback for save/load operations
pub type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

/// Save a big world with pre-computed meshes
pub fn save_big_world(
    path: impl AsRef<Path>,
    world: &ChunkManager,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
    progress: Option<ProgressCallback>,
) -> Result<(), BigWorldError> {
    let path = path.as_ref();
    let (size_x, size_y, size_z) = world_size(world);

    let report = |pct: f32, msg: &str| {
        if let Some(ref cb) = progress {
            cb(pct, msg);
        }
    };

    report(0.0, "Compressing chunk data...");

    // Compress chunks in parallel
    let chunks: Vec<_> = world.chunks().collect();
    let compressed_chunks: Vec<CompressedChunk> = chunks
        .par_iter()
        .map(|(pos, chunk)| CompressedChunk::from_chunk(**pos, chunk))
        .collect();

    report(0.2, "Generating meshes...");

    // Calculate mesh positions
    let mesh_count_x = (size_x + mesh_group_size - 1) / mesh_group_size;
    let mesh_count_z = (size_z + mesh_group_size - 1) / mesh_group_size;
    let total_meshes = (mesh_count_x * mesh_count_z) as usize;

    let mesh_positions: Vec<ChunkPosition> = (0..mesh_count_z)
        .flat_map(|mz| {
            (0..mesh_count_x)
                .map(move |mx| ChunkPosition::new(mx * mesh_group_size, 0, mz * mesh_group_size))
        })
        .collect();

    // Generate meshes (sequential — world isn't Sync)
    let meshes: Vec<MeshData> = mesh_positions
        .iter()
        .enumerate()
        .map(|(i, &pos)| {
            if i % 100 == 0 {
                let pct = 0.2 + 0.6 * (i as f32 / total_meshes as f32);
                report(pct, &format!("Generating mesh {}/{}...", i, total_meshes));
            }

            let vertices = generate_merged_mesh_simple(world, pos, mesh_group_size);
            MeshData {
                base_chunk: [pos.x, pos.y, pos.z],
                vertices: vertices.into_iter().map(SerVertex::from).collect(),
            }
        })
        .collect();

    report(0.8, "Writing file...");

    let data = BigWorldData {
        header: BigWorldHeader {
            world_size_x: size_x,
            world_size_y: size_y,
            world_size_z: size_z,
            mesh_group_size,
            mesh_count: meshes.len() as u32,
            spawn_position,
        },
        chunk_data: compressed_chunks,
        meshes,
    };

    // Serialize to bytes
    let serialized = bincode::serialize(&data)?;

    // Compress with LZ4
    let compressed = lz4_flex::compress_prepend_size(&serialized);

    // Write file
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&compressed)?;

    report(1.0, "Done!");

    log::info!(
        "[BigWorld] Saved {} chunks, {} meshes, {:.1} MB compressed",
        data.chunk_data.len(),
        data.meshes.len(),
        compressed.len() as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

/// Save a big world with existing meshes (much faster - no mesh regeneration)
pub fn save_big_world_with_meshes<'a>(
    path: impl AsRef<Path>,
    world: &ChunkManager,
    meshes: impl Iterator<Item = (ChunkPosition, &'a [Vertex])>,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
) -> Result<(), BigWorldError> {
    let path = path.as_ref();
    let (size_x, size_y, size_z) = world_size(world);

    log::info!("[BigWorld] Compressing chunk data...");

    // Compress chunks in parallel
    let chunks: Vec<_> = world.chunks().collect();
    let compressed_chunks: Vec<CompressedChunk> = chunks
        .par_iter()
        .map(|(pos, chunk)| CompressedChunk::from_chunk(**pos, chunk))
        .collect();

    log::info!("[BigWorld] Converting meshes...");

    // Convert existing meshes (no regeneration!)
    let mesh_data: Vec<MeshData> = meshes
        .map(|(pos, vertices)| MeshData {
            base_chunk: [pos.x, pos.y, pos.z],
            vertices: vertices.iter().map(|&v| SerVertex::from(v)).collect(),
        })
        .collect();

    log::info!("[BigWorld] Writing file...");

    let data = BigWorldData {
        header: BigWorldHeader {
            world_size_x: size_x,
            world_size_y: size_y,
            world_size_z: size_z,
            mesh_group_size,
            mesh_count: mesh_data.len() as u32,
            spawn_position,
        },
        chunk_data: compressed_chunks,
        meshes: mesh_data,
    };

    // Serialize to bytes
    let serialized = bincode::serialize(&data)?;

    // Compress with LZ4
    let compressed = lz4_flex::compress_prepend_size(&serialized);

    // Write file
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&compressed)?;

    log::info!(
        "[BigWorld] Saved {} chunks, {} meshes, {:.1} MB compressed",
        data.chunk_data.len(),
        data.meshes.len(),
        compressed.len() as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

/// Loaded big world data
pub struct LoadedBigWorld {
    pub world: ChunkManager,
    pub meshes: Vec<(ChunkPosition, Vec<Vertex>)>,
    pub spawn_position: [f32; 3],
    pub mesh_group_size: i32,
}

/// Load a big world from file
pub fn load_big_world(
    path: impl AsRef<Path>,
    progress: Option<ProgressCallback>,
) -> Result<LoadedBigWorld, BigWorldError> {
    let path = path.as_ref();

    let report = |pct: f32, msg: &str| {
        if let Some(ref cb) = progress {
            cb(pct, msg);
        }
    };

    report(0.0, "Reading file...");

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read and verify magic
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(BigWorldError::InvalidMagic);
    }

    // Read version
    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != VERSION {
        return Err(BigWorldError::UnsupportedVersion(version));
    }

    report(0.1, "Decompressing...");

    // Read compressed data
    let mut compressed = Vec::new();
    reader.read_to_end(&mut compressed)?;

    // Decompress
    let decompressed = lz4_flex::decompress_size_prepended(&compressed)?;

    report(0.3, "Deserializing...");

    // Deserialize
    let data: BigWorldData = bincode::deserialize(&decompressed)?;

    report(0.5, "Loading chunks...");

    // Create world storage
    let mut world = ChunkManager::new();

    // Load chunks in parallel
    let chunks: Vec<_> = data.chunk_data.par_iter().map(|c| c.to_chunk()).collect();

    for (pos, chunk) in chunks {
        world.insert_chunk(pos, chunk);
    }

    report(0.7, "Converting meshes...");

    // Convert meshes
    let meshes: Vec<(ChunkPosition, Vec<Vertex>)> = data
        .meshes
        .into_par_iter()
        .map(|m| {
            let pos = ChunkPosition::new(m.base_chunk[0], m.base_chunk[1], m.base_chunk[2]);
            let vertices: Vec<Vertex> = m.vertices.into_iter().map(Vertex::from).collect();
            (pos, vertices)
        })
        .collect();

    report(1.0, "Done!");

    log::info!(
        "[BigWorld] Loaded {} chunks, {} meshes",
        world.chunk_count(),
        meshes.len()
    );

    Ok(LoadedBigWorld {
        world,
        meshes,
        spawn_position: data.header.spawn_position,
        mesh_group_size: data.header.mesh_group_size,
    })
}

// ============================================================================
// Fast save/load - chunks only, meshes generated on load
// ============================================================================

/// Fast header (no mesh data)
#[derive(Serialize, Deserialize)]
struct FastWorldHeader {
    world_size_x: i32,
    world_size_y: i32,
    world_size_z: i32,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
}

/// Fast world data (chunks only)
#[derive(Serialize, Deserialize)]
struct FastWorldData {
    header: FastWorldHeader,
    chunk_data: Vec<CompressedChunk>,
}

/// Save world fast - only chunk data, no meshes (very fast save)
pub fn save_big_world_fast(
    path: impl AsRef<Path>,
    world: &ChunkManager,
    mesh_group_size: i32,
    spawn_position: [f32; 3],
) -> Result<(), BigWorldError> {
    let path = path.as_ref();
    let (size_x, size_y, size_z) = world_size(world);

    log::info!("[BigWorld] Compressing {} chunks...", world.chunk_count());

    // Compress chunks in parallel
    let chunks: Vec<_> = world.chunks().collect();
    let compressed_chunks: Vec<CompressedChunk> = chunks
        .par_iter()
        .map(|(pos, chunk)| CompressedChunk::from_chunk(**pos, chunk))
        .collect();

    let data = FastWorldData {
        header: FastWorldHeader {
            world_size_x: size_x,
            world_size_y: size_y,
            world_size_z: size_z,
            mesh_group_size,
            spawn_position,
        },
        chunk_data: compressed_chunks,
    };

    // Serialize to bytes
    let serialized = bincode::serialize(&data)?;

    // Compress with LZ4
    let compressed = lz4_flex::compress_prepend_size(&serialized);

    // Write file
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(MAGIC_FAST)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&compressed)?;

    log::info!(
        "[BigWorld] Saved {} chunks, {:.2} MB",
        data.chunk_data.len(),
        compressed.len() as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

/// Load world fast - regenerates meshes on load
pub fn load_big_world_fast(path: impl AsRef<Path>) -> Result<LoadedBigWorld, BigWorldError> {
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
    if version != VERSION {
        return Err(BigWorldError::UnsupportedVersion(version));
    }

    // Read compressed data
    let mut compressed = Vec::new();
    reader.read_to_end(&mut compressed)?;

    log::info!("[BigWorld] Decompressing...");

    // Decompress
    let decompressed = lz4_flex::decompress_size_prepended(&compressed)?;

    // Deserialize
    let data: FastWorldData = bincode::deserialize(&decompressed)?;

    log::info!("[BigWorld] Loading {} chunks...", data.chunk_data.len());

    // Create world storage
    let mut world = ChunkManager::new();

    // Load chunks in parallel
    let chunks: Vec<_> = data.chunk_data.par_iter().map(|c| c.to_chunk()).collect();

    for (pos, chunk) in chunks {
        world.insert_chunk(pos, chunk);
    }

    log::info!("[BigWorld] Generating meshes...");

    // Generate meshes
    let mesh_group_size = data.header.mesh_group_size;
    let (size_x, _, size_z) = world_size(&world);
    let mesh_count_x = (size_x + mesh_group_size - 1) / mesh_group_size;
    let mesh_count_z = (size_z + mesh_group_size - 1) / mesh_group_size;

    let mesh_positions: Vec<ChunkPosition> = (0..mesh_count_z)
        .flat_map(|mz| {
            (0..mesh_count_x)
                .map(move |mx| ChunkPosition::new(mx * mesh_group_size, 0, mz * mesh_group_size))
        })
        .collect();

    let meshes: Vec<(ChunkPosition, Vec<Vertex>)> = mesh_positions
        .iter()
        .map(|&pos| {
            let vertices = generate_merged_mesh_simple(&world, pos, mesh_group_size);
            (pos, vertices)
        })
        .collect();

    log::info!(
        "[BigWorld] Loaded {} chunks, generated {} meshes",
        world.chunk_count(),
        meshes.len()
    );

    Ok(LoadedBigWorld {
        world,
        meshes,
        spawn_position: data.header.spawn_position,
        mesh_group_size: data.header.mesh_group_size,
    })
}

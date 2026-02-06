use super::Voxel;
use super::chunk::{AIR, CHUNK_SIZE, VOXEL_SCALE};
use crate::renderer::Vertex;
use crate::world::{ChunkManager, ChunkPosition};
use crate::world::lod::VoxelData;
use crate::world::position::LodNodeKey;

#[derive(Clone, Copy)]
struct Face {
    vertices: [[f32; 3]; 4],
    normal: [f32; 3],
    ao_neighbors: [[[i32; 3]; 3]; 4],
}

const FACE_TOP: Face = Face {
    vertices: [
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ],
    normal: [0.0, 1.0, 0.0],
    ao_neighbors: [
        [[-1, 1, 0], [0, 1, -1], [-1, 1, -1]],
        [[1, 1, 0], [0, 1, -1], [1, 1, -1]],
        [[1, 1, 0], [0, 1, 1], [1, 1, 1]],
        [[-1, 1, 0], [0, 1, 1], [-1, 1, 1]],
    ],
};

const FACE_BOTTOM: Face = Face {
    vertices: [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
    ],
    normal: [0.0, -1.0, 0.0],
    ao_neighbors: [
        [[-1, -1, 0], [0, -1, 1], [-1, -1, 1]],
        [[1, -1, 0], [0, -1, 1], [1, -1, 1]],
        [[1, -1, 0], [0, -1, -1], [1, -1, -1]],
        [[-1, -1, 0], [0, -1, -1], [-1, -1, -1]],
    ],
};

const FACE_FRONT: Face = Face {
    vertices: [
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 1.0],
    ],
    normal: [0.0, 0.0, 1.0],
    ao_neighbors: [
        [[-1, 0, 1], [0, -1, 1], [-1, -1, 1]],
        [[-1, 0, 1], [0, 1, 1], [-1, 1, 1]],
        [[1, 0, 1], [0, 1, 1], [1, 1, 1]],
        [[1, 0, 1], [0, -1, 1], [1, -1, 1]],
    ],
};

const FACE_BACK: Face = Face {
    vertices: [
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
    ],
    normal: [0.0, 0.0, -1.0],
    ao_neighbors: [
        [[1, 0, -1], [0, -1, -1], [1, -1, -1]],
        [[1, 0, -1], [0, 1, -1], [1, 1, -1]],
        [[-1, 0, -1], [0, 1, -1], [-1, 1, -1]],
        [[-1, 0, -1], [0, -1, -1], [-1, -1, -1]],
    ],
};

const FACE_RIGHT: Face = Face {
    vertices: [
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ],
    normal: [1.0, 0.0, 0.0],
    ao_neighbors: [
        [[1, 0, 1], [1, -1, 0], [1, -1, 1]],
        [[1, 0, 1], [1, 1, 0], [1, 1, 1]],
        [[1, 0, -1], [1, 1, 0], [1, 1, -1]],
        [[1, 0, -1], [1, -1, 0], [1, -1, -1]],
    ],
};

const FACE_LEFT: Face = Face {
    vertices: [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, 0.0, 1.0],
    ],
    normal: [-1.0, 0.0, 0.0],
    ao_neighbors: [
        [[-1, 0, -1], [-1, -1, 0], [-1, -1, -1]],
        [[-1, 0, -1], [-1, 1, 0], [-1, 1, -1]],
        [[-1, 0, 1], [-1, 1, 0], [-1, 1, 1]],
        [[-1, 0, 1], [-1, -1, 0], [-1, -1, 1]],
    ],
};

fn calc_ao(side1: bool, side2: bool, corner: bool) -> f32 {
    let ao = if side1 && side2 {
        0
    } else {
        3 - (side1 as u8 + side2 as u8 + corner as u8)
    };
    ao as f32 / 3.0
}

fn get_vertex_ao_world(
    world: &ChunkManager,
    wx: i32,
    wy: i32,
    wz: i32,
    neighbors: &[[i32; 3]; 3],
) -> f32 {
    let side1 = world.is_solid(
        wx + neighbors[0][0],
        wy + neighbors[0][1],
        wz + neighbors[0][2],
    );
    let side2 = world.is_solid(
        wx + neighbors[1][0],
        wy + neighbors[1][1],
        wz + neighbors[1][2],
    );
    let corner = world.is_solid(
        wx + neighbors[2][0],
        wy + neighbors[2][1],
        wz + neighbors[2][2],
    );
    calc_ao(side1, side2, corner)
}

fn add_face_world(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    world: &ChunkManager,
    wx: i32,
    wy: i32,
    wz: i32,
    color_index: Voxel,
) {
    let x = wx as f32 * VOXEL_SCALE;
    let y = wy as f32 * VOXEL_SCALE;
    let z = wz as f32 * VOXEL_SCALE;

    let ao = [
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[0]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[1]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[2]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[3]),
    ];

    let indices = if ao[0] + ao[2] > ao[1] + ao[3] {
        [0, 2, 1, 0, 3, 2]
    } else {
        [0, 3, 1, 1, 3, 2]
    };

    for &i in &indices {
        let v = face.vertices[i];
        vertices.push(Vertex::new(
            [
                v[0] * VOXEL_SCALE + x,
                v[1] * VOXEL_SCALE + y,
                v[2] * VOXEL_SCALE + z,
            ],
            face.normal,
            ao[i],
            color_index,
        ));
    }
}

/// Generate mesh for a single chunk using world coordinates for cross-boundary neighbor checks
pub fn generate_chunk_mesh(world: &ChunkManager, chunk_pos: ChunkPosition) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    let chunk = match world.get_chunk(chunk_pos) {
        Some(c) => c,
        None => return vertices,
    };

    let (origin_x, origin_y, origin_z) = chunk_pos.world_origin();

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let voxel = chunk.get(x, y, z);
                if voxel == AIR {
                    continue;
                }

                let wx = origin_x + x as i32;
                let wy = origin_y + y as i32;
                let wz = origin_z + z as i32;

                if !world.is_solid(wx, wy + 1, wz) {
                    add_face_world(&mut vertices, &FACE_TOP, world, wx, wy, wz, voxel);
                }
                if !world.is_solid(wx, wy - 1, wz) {
                    add_face_world(&mut vertices, &FACE_BOTTOM, world, wx, wy, wz, voxel);
                }
                if !world.is_solid(wx, wy, wz + 1) {
                    add_face_world(&mut vertices, &FACE_FRONT, world, wx, wy, wz, voxel);
                }
                if !world.is_solid(wx, wy, wz - 1) {
                    add_face_world(&mut vertices, &FACE_BACK, world, wx, wy, wz, voxel);
                }
                if !world.is_solid(wx + 1, wy, wz) {
                    add_face_world(&mut vertices, &FACE_RIGHT, world, wx, wy, wz, voxel);
                }
                if !world.is_solid(wx - 1, wy, wz) {
                    add_face_world(&mut vertices, &FACE_LEFT, world, wx, wy, wz, voxel);
                }
            }
        }
    }

    vertices
}

/// Generate LOD mesh for a single chunk with reduced detail
/// step: voxel sampling step (2 = every other voxel, 4 = every 4th voxel, etc.)
pub fn generate_lod_mesh(
    world: &ChunkManager,
    chunk_pos: ChunkPosition,
    _lod_scale: i32, // unused, kept for API compatibility
    step: usize,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    let chunk = match world.get_chunk(chunk_pos) {
        Some(c) => c,
        None => return vertices,
    };

    let (origin_x, origin_y, origin_z) = chunk_pos.world_origin();
    let voxel_scale = VOXEL_SCALE * step as f32;
    let step_i32 = step as i32;

    // Sample voxels with step size within this chunk
    for lx in (0..CHUNK_SIZE).step_by(step) {
        for ly in (0..CHUNK_SIZE).step_by(step) {
            for lz in (0..CHUNK_SIZE).step_by(step) {
                let voxel = chunk.get(lx, ly, lz);
                if voxel == AIR {
                    continue;
                }

                let wx = origin_x + lx as i32;
                let wy = origin_y + ly as i32;
                let wz = origin_z + lz as i32;

                // Check if face should be visible (neighbor is air)
                if !world.is_solid(wx, wy + step_i32, wz) {
                    add_face_lod(&mut vertices, &FACE_TOP, lx, ly, lz, voxel_scale, voxel);
                }
                if !world.is_solid(wx, wy - step_i32, wz) {
                    add_face_lod(&mut vertices, &FACE_BOTTOM, lx, ly, lz, voxel_scale, voxel);
                }
                if !world.is_solid(wx, wy, wz + step_i32) {
                    add_face_lod(&mut vertices, &FACE_FRONT, lx, ly, lz, voxel_scale, voxel);
                }
                if !world.is_solid(wx, wy, wz - step_i32) {
                    add_face_lod(&mut vertices, &FACE_BACK, lx, ly, lz, voxel_scale, voxel);
                }
                if !world.is_solid(wx + step_i32, wy, wz) {
                    add_face_lod(&mut vertices, &FACE_RIGHT, lx, ly, lz, voxel_scale, voxel);
                }
                if !world.is_solid(wx - step_i32, wy, wz) {
                    add_face_lod(&mut vertices, &FACE_LEFT, lx, ly, lz, voxel_scale, voxel);
                }
            }
        }
    }

    vertices
}

/// Generate merged mesh for a group of chunks (e.g., 8x8 chunks combined)
/// base_chunk: bottom-left corner of the group
/// group_size: number of chunks per axis (e.g., 8 for 8x8)
pub fn generate_merged_mesh(
    world: &ChunkManager,
    base_chunk: ChunkPosition,
    group_size: i32,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    // Iterate over all chunks in the group
    for dx in 0..group_size {
        for dz in 0..group_size {
            let chunk_pos = ChunkPosition::new(base_chunk.x + dx, base_chunk.y, base_chunk.z + dz);

            let chunk = match world.get_chunk(chunk_pos) {
                Some(c) => c,
                None => continue,
            };

            let (origin_x, origin_y, origin_z) = chunk_pos.world_origin();

            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        let voxel = chunk.get(x, y, z);
                        if voxel == AIR {
                            continue;
                        }

                        let wx = origin_x + x as i32;
                        let wy = origin_y + y as i32;
                        let wz = origin_z + z as i32;

                        // Check faces using world coordinates (handles chunk boundaries)
                        if !world.is_solid(wx, wy + 1, wz) {
                            add_face_world(&mut vertices, &FACE_TOP, world, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy - 1, wz) {
                            add_face_world(&mut vertices, &FACE_BOTTOM, world, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy, wz + 1) {
                            add_face_world(&mut vertices, &FACE_FRONT, world, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy, wz - 1) {
                            add_face_world(&mut vertices, &FACE_BACK, world, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx + 1, wy, wz) {
                            add_face_world(&mut vertices, &FACE_RIGHT, world, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx - 1, wy, wz) {
                            add_face_world(&mut vertices, &FACE_LEFT, world, wx, wy, wz, voxel);
                        }
                    }
                }
            }
        }
    }

    vertices
}

/// Generate merged mesh without AO (faster for big worlds)
pub fn generate_merged_mesh_simple(
    world: &ChunkManager,
    base_chunk: ChunkPosition,
    group_size: i32,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    // Iterate over all chunks in the group
    for dx in 0..group_size {
        for dz in 0..group_size {
            let chunk_pos = ChunkPosition::new(base_chunk.x + dx, base_chunk.y, base_chunk.z + dz);

            let chunk = match world.get_chunk(chunk_pos) {
                Some(c) => c,
                None => continue,
            };

            let (origin_x, origin_y, origin_z) = chunk_pos.world_origin();

            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        let voxel = chunk.get(x, y, z);
                        if voxel == AIR {
                            continue;
                        }

                        let wx = origin_x + x as i32;
                        let wy = origin_y + y as i32;
                        let wz = origin_z + z as i32;

                        // Check faces - simplified AO (no AO for merged meshes, faster)
                        if !world.is_solid(wx, wy + 1, wz) {
                            add_face_simple(&mut vertices, &FACE_TOP, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy - 1, wz) {
                            add_face_simple(&mut vertices, &FACE_BOTTOM, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy, wz + 1) {
                            add_face_simple(&mut vertices, &FACE_FRONT, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx, wy, wz - 1) {
                            add_face_simple(&mut vertices, &FACE_BACK, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx + 1, wy, wz) {
                            add_face_simple(&mut vertices, &FACE_RIGHT, wx, wy, wz, voxel);
                        }
                        if !world.is_solid(wx - 1, wy, wz) {
                            add_face_simple(&mut vertices, &FACE_LEFT, wx, wy, wz, voxel);
                        }
                    }
                }
            }
        }
    }

    vertices
}

/// Add face without AO calculation (faster for big worlds)
fn add_face_simple(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    wx: i32,
    wy: i32,
    wz: i32,
    color_index: Voxel,
) {
    let x = wx as f32 * VOXEL_SCALE;
    let y = wy as f32 * VOXEL_SCALE;
    let z = wz as f32 * VOXEL_SCALE;

    let indices = [0, 2, 1, 0, 3, 2];

    for &i in &indices {
        let v = face.vertices[i];
        vertices.push(Vertex::new(
            [
                v[0] * VOXEL_SCALE + x,
                v[1] * VOXEL_SCALE + y,
                v[2] * VOXEL_SCALE + z,
            ],
            face.normal,
            1.0, // No AO
            color_index,
        ));
    }
}

/// Generate mesh for an octree LOD node.
/// The data is a 32^3 VoxelData representing the downsampled content of multiple chunks.
/// Each LOD voxel is scaled according to the node's lod_level.
/// Self-contained neighbor checks (no cross-boundary lookups).
pub fn generate_octree_lod_mesh(data: &VoxelData, key: &LodNodeKey) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    let scale = key.voxel_scale();
    let (origin_x, origin_y, origin_z) = key.world_origin();
    let origin_wx = origin_x as f32 * VOXEL_SCALE;
    let origin_wy = origin_y as f32 * VOXEL_SCALE;
    let origin_wz = origin_z as f32 * VOXEL_SCALE;

    match data {
        VoxelData::Homogeneous(v) => {
            if *v == AIR {
                return vertices;
            }
            // Single box with 6 faces covering the entire node
            let size = CHUNK_SIZE as f32 * scale;
            add_face_scaled(&mut vertices, &FACE_TOP, origin_wx, origin_wy, origin_wz, size, *v);
            add_face_scaled(&mut vertices, &FACE_BOTTOM, origin_wx, origin_wy, origin_wz, size, *v);
            add_face_scaled(&mut vertices, &FACE_FRONT, origin_wx, origin_wy, origin_wz, size, *v);
            add_face_scaled(&mut vertices, &FACE_BACK, origin_wx, origin_wy, origin_wz, size, *v);
            add_face_scaled(&mut vertices, &FACE_RIGHT, origin_wx, origin_wy, origin_wz, size, *v);
            add_face_scaled(&mut vertices, &FACE_LEFT, origin_wx, origin_wy, origin_wz, size, *v);
        }
        _ => {
            let res = CHUNK_SIZE; // Always iterate 32^3
            for x in 0..res {
                for y in 0..res {
                    for z in 0..res {
                        let voxel = data.get(x, y, z);
                        if voxel == AIR {
                            continue;
                        }

                        let wx = origin_wx + x as f32 * scale;
                        let wy = origin_wy + y as f32 * scale;
                        let wz = origin_wz + z as f32 * scale;

                        // Self-contained neighbor checks within the 32^3 data
                        // At boundaries, treat as exposed (air)
                        if y + 1 >= res || data.get(x, y + 1, z) == AIR {
                            add_face_scaled(&mut vertices, &FACE_TOP, wx, wy, wz, scale, voxel);
                        }
                        if y == 0 || data.get(x, y - 1, z) == AIR {
                            add_face_scaled(&mut vertices, &FACE_BOTTOM, wx, wy, wz, scale, voxel);
                        }
                        if z + 1 >= res || data.get(x, y, z + 1) == AIR {
                            add_face_scaled(&mut vertices, &FACE_FRONT, wx, wy, wz, scale, voxel);
                        }
                        if z == 0 || data.get(x, y, z - 1) == AIR {
                            add_face_scaled(&mut vertices, &FACE_BACK, wx, wy, wz, scale, voxel);
                        }
                        if x + 1 >= res || data.get(x + 1, y, z) == AIR {
                            add_face_scaled(&mut vertices, &FACE_RIGHT, wx, wy, wz, scale, voxel);
                        }
                        if x == 0 || data.get(x - 1, y, z) == AIR {
                            add_face_scaled(&mut vertices, &FACE_LEFT, wx, wy, wz, scale, voxel);
                        }
                    }
                }
            }
        }
    }

    vertices
}

/// Add a face at world position with arbitrary scale (no AO).
fn add_face_scaled(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    wx: f32,
    wy: f32,
    wz: f32,
    scale: f32,
    color_index: Voxel,
) {
    let indices = [0, 2, 1, 0, 3, 2];

    for &i in &indices {
        let v = face.vertices[i];
        vertices.push(Vertex::new(
            [
                v[0] * scale + wx,
                v[1] * scale + wy,
                v[2] * scale + wz,
            ],
            face.normal,
            1.0, // No AO for LOD meshes
            color_index,
        ));
    }
}

/// Get representative voxel for a region (fast: just sample center)
#[inline]
fn get_dominant_voxel(world: &ChunkManager, wx: i32, wy: i32, wz: i32, size: i32) -> Voxel {
    // Fast path: just check center voxel
    let half = size / 2;
    world.get_voxel(wx + half, wy + half, wz + half)
}

/// Check if region is solid at LOD scale
fn is_solid_lod(world: &ChunkManager, wx: i32, wy: i32, wz: i32, size: i32) -> bool {
    // Check center point for speed
    world.is_solid(wx + size / 2, wy + size / 2, wz + size / 2)
}

/// Add face for LOD mesh (simplified, no AO)
fn add_face_lod(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    lx: usize,
    ly: usize,
    lz: usize,
    voxel_scale: f32,
    color_index: Voxel,
) {
    let x = lx as f32 * VOXEL_SCALE;
    let y = ly as f32 * VOXEL_SCALE;
    let z = lz as f32 * VOXEL_SCALE;

    // No AO for LOD meshes (faster)
    let ao = 1.0;

    let indices = [0, 2, 1, 0, 3, 2];

    for &i in &indices {
        let v = face.vertices[i];
        vertices.push(Vertex::new(
            [
                v[0] * voxel_scale + x,
                v[1] * voxel_scale + y,
                v[2] * voxel_scale + z,
            ],
            face.normal,
            ao,
            color_index,
        ));
    }
}

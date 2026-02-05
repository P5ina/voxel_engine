use super::block::BlockType;
use super::chunk::{Chunk, CHUNK_SIZE};
use crate::renderer::Vertex;
use crate::world::{ChunkManager, ChunkPosition};

const TILE_SIZE: f32 = 1.0 / 16.0;

#[derive(Clone, Copy)]
struct Face {
    vertices: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
    // For each vertex: [side1, side2, corner] offsets from block position
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
    uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    ao_neighbors: [
        [[-1, 1, 0], [0, 1, -1], [-1, 1, -1]], // v0: -X, -Z corner
        [[1, 1, 0], [0, 1, -1], [1, 1, -1]],   // v1: +X, -Z corner
        [[1, 1, 0], [0, 1, 1], [1, 1, 1]],     // v2: +X, +Z corner
        [[-1, 1, 0], [0, 1, 1], [-1, 1, 1]],   // v3: -X, +Z corner
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
    uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
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
    uvs: [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
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
    uvs: [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
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
    uvs: [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
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
    uvs: [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    ao_neighbors: [
        [[-1, 0, -1], [-1, -1, 0], [-1, -1, -1]],
        [[-1, 0, -1], [-1, 1, 0], [-1, 1, -1]],
        [[-1, 0, 1], [-1, 1, 0], [-1, 1, 1]],
        [[-1, 0, 1], [-1, -1, 0], [-1, -1, 1]],
    ],
};

#[derive(Clone, Copy)]
enum FaceDir {
    Top,
    Bottom,
    Side,
}

fn get_tile(block: BlockType, dir: FaceDir) -> (u32, u32) {
    match block {
        BlockType::Air => (0, 0),
        BlockType::Dirt => (0, 0),
        BlockType::Stone => (1, 0),
        BlockType::Grass => match dir {
            FaceDir::Top => (2, 0),
            FaceDir::Bottom => (0, 0),
            FaceDir::Side => (3, 0),
        },
        BlockType::Light => (1, 0), // Use stone texture for now (will glow via emission)
    }
}

fn calc_ao(side1: bool, side2: bool, corner: bool) -> f32 {
    let ao = if side1 && side2 {
        0 // Both sides block light completely
    } else {
        3 - (side1 as u8 + side2 as u8 + corner as u8)
    };
    // Convert to 0.0-1.0 range (0 = dark, 1 = bright)
    ao as f32 / 3.0
}

fn get_vertex_ao(chunk: &Chunk, bx: i32, by: i32, bz: i32, neighbors: &[[i32; 3]; 3]) -> f32 {
    let side1 = chunk
        .get_signed(bx + neighbors[0][0], by + neighbors[0][1], bz + neighbors[0][2])
        .is_solid();
    let side2 = chunk
        .get_signed(bx + neighbors[1][0], by + neighbors[1][1], bz + neighbors[1][2])
        .is_solid();
    let corner = chunk
        .get_signed(bx + neighbors[2][0], by + neighbors[2][1], bz + neighbors[2][2])
        .is_solid();
    calc_ao(side1, side2, corner)
}

fn get_vertex_ao_world(world: &ChunkManager, wx: i32, wy: i32, wz: i32, neighbors: &[[i32; 3]; 3]) -> f32 {
    let side1 = world.is_solid(wx + neighbors[0][0], wy + neighbors[0][1], wz + neighbors[0][2]);
    let side2 = world.is_solid(wx + neighbors[1][0], wy + neighbors[1][1], wz + neighbors[1][2]);
    let corner = world.is_solid(wx + neighbors[2][0], wy + neighbors[2][1], wz + neighbors[2][2]);
    calc_ao(side1, side2, corner)
}

fn add_face(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    chunk: &Chunk,
    bx: i32,
    by: i32,
    bz: i32,
    tile: (u32, u32),
    block: BlockType,
) {
    let u_base = tile.0 as f32 * TILE_SIZE;
    let v_base = tile.1 as f32 * TILE_SIZE;

    let x = bx as f32;
    let y = by as f32;
    let z = bz as f32;

    let material_id = block as u32;

    // Calculate AO for each vertex
    let ao = [
        get_vertex_ao(chunk, bx, by, bz, &face.ao_neighbors[0]),
        get_vertex_ao(chunk, bx, by, bz, &face.ao_neighbors[1]),
        get_vertex_ao(chunk, bx, by, bz, &face.ao_neighbors[2]),
        get_vertex_ao(chunk, bx, by, bz, &face.ao_neighbors[3]),
    ];

    // Fix anisotropy: flip quad diagonal if needed for smoother AO interpolation
    let indices = if ao[0] + ao[2] > ao[1] + ao[3] {
        [0, 2, 1, 0, 3, 2] // Normal diagonal
    } else {
        [0, 3, 1, 1, 3, 2] // Flipped diagonal (maintains CCW)
    };

    for &i in &indices {
        let v = face.vertices[i];
        let uv = face.uvs[i];
        vertices.push(Vertex::new(
            [v[0] + x, v[1] + y, v[2] + z],
            face.normal,
            [u_base + uv[0] * TILE_SIZE, v_base + uv[1] * TILE_SIZE],
            ao[i],
            material_id,
        ));
    }
}

pub fn generate_mesh(chunk: &Chunk) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z);
                if !block.is_solid() {
                    continue;
                }

                let bx = x as i32;
                let by = y as i32;
                let bz = z as i32;

                if !chunk.get_signed(bx, by + 1, bz).is_solid() {
                    let tile = get_tile(block, FaceDir::Top);
                    add_face(&mut vertices, &FACE_TOP, chunk, bx, by, bz, tile, block);
                }
                if !chunk.get_signed(bx, by - 1, bz).is_solid() {
                    let tile = get_tile(block, FaceDir::Bottom);
                    add_face(&mut vertices, &FACE_BOTTOM, chunk, bx, by, bz, tile, block);
                }
                if !chunk.get_signed(bx, by, bz + 1).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_FRONT, chunk, bx, by, bz, tile, block);
                }
                if !chunk.get_signed(bx, by, bz - 1).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_BACK, chunk, bx, by, bz, tile, block);
                }
                if !chunk.get_signed(bx + 1, by, bz).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_RIGHT, chunk, bx, by, bz, tile, block);
                }
                if !chunk.get_signed(bx - 1, by, bz).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_LEFT, chunk, bx, by, bz, tile, block);
                }
            }
        }
    }

    vertices
}

fn add_face_world(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    world: &ChunkManager,
    wx: i32,
    wy: i32,
    wz: i32,
    tile: (u32, u32),
    block: BlockType,
) {
    let u_base = tile.0 as f32 * TILE_SIZE;
    let v_base = tile.1 as f32 * TILE_SIZE;

    let x = wx as f32;
    let y = wy as f32;
    let z = wz as f32;

    let material_id = block as u32;

    // Calculate AO for each vertex
    let ao = [
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[0]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[1]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[2]),
        get_vertex_ao_world(world, wx, wy, wz, &face.ao_neighbors[3]),
    ];

    // Fix anisotropy: flip quad diagonal if needed for smoother AO interpolation
    let indices = if ao[0] + ao[2] > ao[1] + ao[3] {
        [0, 2, 1, 0, 3, 2] // Normal diagonal
    } else {
        [0, 3, 1, 1, 3, 2] // Flipped diagonal (maintains CCW)
    };

    for &i in &indices {
        let v = face.vertices[i];
        let uv = face.uvs[i];
        vertices.push(Vertex::new(
            [v[0] + x, v[1] + y, v[2] + z],
            face.normal,
            [u_base + uv[0] * TILE_SIZE, v_base + uv[1] * TILE_SIZE],
            ao[i],
            material_id,
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
                let block = chunk.get(x, y, z);
                if !block.is_solid() {
                    continue;
                }

                // World coordinates
                let wx = origin_x + x as i32;
                let wy = origin_y + y as i32;
                let wz = origin_z + z as i32;

                // Check neighbors using world coordinates (crosses chunk boundaries)
                if !world.is_solid(wx, wy + 1, wz) {
                    let tile = get_tile(block, FaceDir::Top);
                    add_face_world(&mut vertices, &FACE_TOP, world, wx, wy, wz, tile, block);
                }
                if !world.is_solid(wx, wy - 1, wz) {
                    let tile = get_tile(block, FaceDir::Bottom);
                    add_face_world(&mut vertices, &FACE_BOTTOM, world, wx, wy, wz, tile, block);
                }
                if !world.is_solid(wx, wy, wz + 1) {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face_world(&mut vertices, &FACE_FRONT, world, wx, wy, wz, tile, block);
                }
                if !world.is_solid(wx, wy, wz - 1) {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face_world(&mut vertices, &FACE_BACK, world, wx, wy, wz, tile, block);
                }
                if !world.is_solid(wx + 1, wy, wz) {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face_world(&mut vertices, &FACE_RIGHT, world, wx, wy, wz, tile, block);
                }
                if !world.is_solid(wx - 1, wy, wz) {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face_world(&mut vertices, &FACE_LEFT, world, wx, wy, wz, tile, block);
                }
            }
        }
    }

    vertices
}

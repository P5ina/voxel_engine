use super::block::BlockType;
use super::chunk::{CHUNK_SIZE, Chunk};
use crate::renderer::Vertex;

const TILE_SIZE: f32 = 1.0 / 16.0;

#[derive(Clone, Copy)]
struct Face {
    vertices: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
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
    }
}

fn add_face(vertices: &mut Vec<Vertex>, face: &Face, x: f32, y: f32, z: f32, tile: (u32, u32)) {
    let u_base = tile.0 as f32 * TILE_SIZE;
    let v_base = tile.1 as f32 * TILE_SIZE;

    let indices = [0, 2, 1, 0, 3, 2];
    for &i in &indices {
        let v = face.vertices[i];
        let uv = face.uvs[i];
        vertices.push(Vertex::new(
            [v[0] + x, v[1] + y, v[2] + z],
            face.normal,
            [u_base + uv[0] * TILE_SIZE, v_base + uv[1] * TILE_SIZE],
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

                let fx = x as f32;
                let fy = y as f32;
                let fz = z as f32;

                let xi = x as i32;
                let yi = y as i32;
                let zi = z as i32;

                if !chunk.get_signed(xi, yi + 1, zi).is_solid() {
                    let tile = get_tile(block, FaceDir::Top);
                    add_face(&mut vertices, &FACE_TOP, fx, fy, fz, tile);
                }
                if !chunk.get_signed(xi, yi - 1, zi).is_solid() {
                    let tile = get_tile(block, FaceDir::Bottom);
                    add_face(&mut vertices, &FACE_BOTTOM, fx, fy, fz, tile);
                }
                if !chunk.get_signed(xi, yi, zi + 1).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_FRONT, fx, fy, fz, tile);
                }
                if !chunk.get_signed(xi, yi, zi - 1).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_BACK, fx, fy, fz, tile);
                }
                if !chunk.get_signed(xi + 1, yi, zi).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_RIGHT, fx, fy, fz, tile);
                }
                if !chunk.get_signed(xi - 1, yi, zi).is_solid() {
                    let tile = get_tile(block, FaceDir::Side);
                    add_face(&mut vertices, &FACE_LEFT, fx, fy, fz, tile);
                }
            }
        }
    }

    vertices
}

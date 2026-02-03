use super::chunk::{Chunk, CHUNK_SIZE};
use crate::renderer::Vertex;

#[derive(Clone, Copy)]
struct Face {
    vertices: [[f32; 3]; 4],
    normal: [f32; 3],
}

const FACE_TOP: Face = Face {
    vertices: [
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ],
    normal: [0.0, 1.0, 0.0],
};

const FACE_BOTTOM: Face = Face {
    vertices: [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
    ],
    normal: [0.0, -1.0, 0.0],
};

const FACE_FRONT: Face = Face {
    vertices: [
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 1.0],
    ],
    normal: [0.0, 0.0, 1.0],
};

const FACE_BACK: Face = Face {
    vertices: [
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
    ],
    normal: [0.0, 0.0, -1.0],
};

const FACE_RIGHT: Face = Face {
    vertices: [
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ],
    normal: [1.0, 0.0, 0.0],
};

const FACE_LEFT: Face = Face {
    vertices: [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, 0.0, 1.0],
    ],
    normal: [-1.0, 0.0, 0.0],
};

fn add_face(vertices: &mut Vec<Vertex>, face: &Face, x: f32, y: f32, z: f32, color: [f32; 3]) {
    let indices = [0, 2, 1, 0, 3, 2];
    for &i in &indices {
        let v = face.vertices[i];
        vertices.push(Vertex::new(
            [v[0] + x, v[1] + y, v[2] + z],
            face.normal,
            color,
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

                let color = block.color();
                let fx = x as f32;
                let fy = y as f32;
                let fz = z as f32;

                let xi = x as i32;
                let yi = y as i32;
                let zi = z as i32;

                // Check each face
                if !chunk.get_signed(xi, yi + 1, zi).is_solid() {
                    add_face(&mut vertices, &FACE_TOP, fx, fy, fz, color);
                }
                if !chunk.get_signed(xi, yi - 1, zi).is_solid() {
                    add_face(&mut vertices, &FACE_BOTTOM, fx, fy, fz, color);
                }
                if !chunk.get_signed(xi, yi, zi + 1).is_solid() {
                    add_face(&mut vertices, &FACE_FRONT, fx, fy, fz, color);
                }
                if !chunk.get_signed(xi, yi, zi - 1).is_solid() {
                    add_face(&mut vertices, &FACE_BACK, fx, fy, fz, color);
                }
                if !chunk.get_signed(xi + 1, yi, zi).is_solid() {
                    add_face(&mut vertices, &FACE_RIGHT, fx, fy, fz, color);
                }
                if !chunk.get_signed(xi - 1, yi, zi).is_solid() {
                    add_face(&mut vertices, &FACE_LEFT, fx, fy, fz, color);
                }
            }
        }
    }

    vertices
}

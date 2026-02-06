use super::Voxel;
use super::chunk::{AIR, CHUNK_SIZE, VOXEL_SCALE};
use crate::renderer::Vertex;
use crate::world::lod::VoxelData;
use crate::world::position::LodNodeKey;
use crate::world::{ChunkManager, ChunkPosition};

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
            add_face_scaled(
                &mut vertices,
                &FACE_TOP,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
            add_face_scaled(
                &mut vertices,
                &FACE_BOTTOM,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
            add_face_scaled(
                &mut vertices,
                &FACE_FRONT,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
            add_face_scaled(
                &mut vertices,
                &FACE_BACK,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
            add_face_scaled(
                &mut vertices,
                &FACE_RIGHT,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
            add_face_scaled(
                &mut vertices,
                &FACE_LEFT,
                origin_wx,
                origin_wy,
                origin_wz,
                size,
                *v,
            );
        }
        _ => {
            // Downsample for coarser mesh at higher LOD levels to reduce triangle count.
            // LOD 1: keep 32³ (closest LOD, needs detail)
            // LOD 2: downsample to 16³ (8x fewer iterations)
            // LOD 3: downsample to 8³ (64x fewer)
            // LOD 4+: downsample to 4³ (512x fewer)
            let downsampled = match key.lod_level {
                0 | 1 => None,
                2 => Some(data.downsample(1)),
                3 => Some(data.downsample(2)),
                _ => Some(data.downsample(3)),
            };
            let mesh_data = downsampled.as_ref().unwrap_or(data);
            let res = mesh_data.resolution();
            let scale_multiplier = (CHUNK_SIZE / res) as f32;
            let cell_scale = scale * scale_multiplier;

            for x in 0..res {
                for y in 0..res {
                    for z in 0..res {
                        let voxel = mesh_data.get_native(x, y, z);
                        if voxel == AIR {
                            continue;
                        }

                        let wx = origin_wx + x as f32 * cell_scale;
                        let wy = origin_wy + y as f32 * cell_scale;
                        let wz = origin_wz + z as f32 * cell_scale;

                        if y + 1 >= res || mesh_data.get_native(x, y + 1, z) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_TOP,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
                        }
                        if y == 0 || mesh_data.get_native(x, y - 1, z) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_BOTTOM,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
                        }
                        if z + 1 >= res || mesh_data.get_native(x, y, z + 1) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_FRONT,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
                        }
                        if z == 0 || mesh_data.get_native(x, y, z - 1) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_BACK,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
                        }
                        if x + 1 >= res || mesh_data.get_native(x + 1, y, z) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_RIGHT,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
                        }
                        if x == 0 || mesh_data.get_native(x - 1, y, z) == AIR {
                            add_face_scaled(
                                &mut vertices,
                                &FACE_LEFT,
                                wx,
                                wy,
                                wz,
                                cell_scale,
                                voxel,
                            );
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
            [v[0] * scale + wx, v[1] * scale + wy, v[2] * scale + wz],
            face.normal,
            1.0, // No AO for LOD meshes
            color_index,
        ));
    }
}

#![allow(clippy::needless_range_loop)]

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

/// A greedy face direction with axis mapping.
/// For each face direction, we sweep slices along the normal axis.
/// Within each slice, we have a 2D grid of (u, v) coordinates.
/// `axis_map(slice, u, v)` returns (x, y, z) in local chunk coords.
struct GreedyFaceDir {
    face: Face,
    /// Maps (slice, u, v) -> (x, y, z) local chunk coords
    axis_map: fn(usize, usize, usize) -> (usize, usize, usize),
    /// Neighbor offset along normal axis for visibility check: +1 or -1
    normal_step: i32,
    /// Which axis is the normal (0=x, 1=y, 2=z)
    normal_axis: usize,
}

const GREEDY_DIRS: [GreedyFaceDir; 6] = [
    // Top (Y+): sweep Y, u=X, v=Z
    GreedyFaceDir {
        face: FACE_TOP,
        axis_map: |slice, u, v| (u, slice, v),
        normal_step: 1,
        normal_axis: 1,
    },
    // Bottom (Y-): sweep Y, u=X, v=Z
    GreedyFaceDir {
        face: FACE_BOTTOM,
        axis_map: |slice, u, v| (u, slice, v),
        normal_step: -1,
        normal_axis: 1,
    },
    // Front (Z+): sweep Z, u=X, v=Y
    GreedyFaceDir {
        face: FACE_FRONT,
        axis_map: |slice, u, v| (u, v, slice),
        normal_step: 1,
        normal_axis: 2,
    },
    // Back (Z-): sweep Z, u=X, v=Y
    GreedyFaceDir {
        face: FACE_BACK,
        axis_map: |slice, u, v| (u, v, slice),
        normal_step: -1,
        normal_axis: 2,
    },
    // Right (X+): sweep X, u=Z, v=Y
    GreedyFaceDir {
        face: FACE_RIGHT,
        axis_map: |slice, u, v| (slice, v, u),
        normal_step: 1,
        normal_axis: 0,
    },
    // Left (X-): sweep X, u=Z, v=Y
    GreedyFaceDir {
        face: FACE_LEFT,
        axis_map: |slice, u, v| (slice, v, u),
        normal_step: -1,
        normal_axis: 0,
    },
];

/// Emit a merged quad for greedy meshing with AO.
/// (u0, v0) is the start corner, (w, h) is the size in voxels.
/// The face direction determines how to map (slice, u, v) to world coords.
#[allow(clippy::too_many_arguments)]
fn emit_greedy_quad(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    dir: &GreedyFaceDir,
    world: &ChunkManager,
    origin_x: i32,
    origin_y: i32,
    origin_z: i32,
    slice: usize,
    u0: usize,
    v0: usize,
    w: usize,
    h: usize,
    color_index: Voxel,
) {
    // The 4 corners of the original face go:
    //   vertex[0] = (0,0) corner in face-local uv
    //   vertex[1] = (0,1) or (1,0) depending on face winding
    //   vertex[2] = (1,1) corner
    //   vertex[3] = opposite of vertex[1]
    //
    // For each face, the vertices define a unit quad. We need to figure out
    // which face-local vertex maps to which corner of our (u0,v0)-(u0+w,v0+h) rect.
    //
    // The face vertices are defined in terms of (x,y,z) where the normal-axis
    // component is constant. The other two axes map to our u,v.
    //
    // We compute the world-space position of each corner of the merged quad,
    // and compute AO at the voxel coordinate of each corner.

    // Determine the 4 corner positions in local chunk coordinates:
    // For each face vertex, we need to scale it from unit to our quad size.
    // The face vertices have the normal-axis component as 0 or 1 (constant for the face).
    // The other two components go from 0..1 in the unit quad.

    // Extract which components of the face vertex correspond to u and v.
    // For Top/Bottom (normal_axis=1): face vertex x->u, z->v
    // For Front/Back (normal_axis=2): face vertex x->u, y->v
    // For Right/Left (normal_axis=0): face vertex z->u, y->v
    let (u_comp, v_comp) = match dir.normal_axis {
        0 => (2, 1), // x-normal: u=z, v=y
        1 => (0, 2), // y-normal: u=x, v=z
        _ => (0, 1), // z-normal: u=x, v=y
    };

    let mut corner_pos = [[0.0f32; 3]; 4];
    let mut corner_ao = [1.0f32; 4];

    for i in 0..4 {
        let fv = face.vertices[i];
        // Map the face vertex (0 or 1 in u,v) to our merged quad coordinates
        let u_frac = fv[u_comp]; // 0.0 or 1.0
        let v_frac = fv[v_comp]; // 0.0 or 1.0

        let u_local = u0 as f32 + u_frac * w as f32;
        let v_local = v0 as f32 + v_frac * h as f32;

        // Reconstruct the 3D local position
        let (lx, ly, lz) = match dir.normal_axis {
            0 => {
                // x is fixed (slice + face normal component)
                let x = slice as f32 + fv[0]; // fv[0] is 0 or 1 on the normal axis
                (x, v_local, u_local)
            }
            1 => {
                let y = slice as f32 + fv[1];
                (u_local, y, v_local)
            }
            _ => {
                let z = slice as f32 + fv[2];
                (u_local, v_local, z)
            }
        };

        let wx = origin_x as f32 + lx;
        let wy = origin_y as f32 + ly;
        let wz = origin_z as f32 + lz;

        corner_pos[i] = [wx * VOXEL_SCALE, wy * VOXEL_SCALE, wz * VOXEL_SCALE];

        // AO: sample at the voxel coordinate of this corner.
        // The AO corner corresponds to the voxel at the "inner" corner of the quad.
        // For AO, we use the voxel that this corner is adjacent to.
        // The AO voxel position: take the face-local corner and map back to a voxel.
        // For a merged quad from (u0,v0) to (u0+w,v0+h), the AO voxel for corner
        // with (u_frac, v_frac) = (0,0) is at (u0, v0), and for (1,1) is at (u0+w-1, v0+h-1).
        let ao_u = if u_frac < 0.5 {
            u0 as i32
        } else {
            u0 as i32 + w as i32 - 1
        };
        let ao_v = if v_frac < 0.5 {
            v0 as i32
        } else {
            v0 as i32 + h as i32 - 1
        };

        let (ax, ay, az) = match dir.normal_axis {
            0 => (slice as i32, ao_v, ao_u),
            1 => (ao_u, slice as i32, ao_v),
            _ => (ao_u, ao_v, slice as i32),
        };

        let awx = origin_x + ax;
        let awy = origin_y + ay;
        let awz = origin_z + az;

        corner_ao[i] = get_vertex_ao_world(world, awx, awy, awz, &face.ao_neighbors[i]);
    }

    // Diagonal flip for AO
    let indices = if corner_ao[0] + corner_ao[2] > corner_ao[1] + corner_ao[3] {
        [0, 2, 1, 0, 3, 2]
    } else {
        [0, 3, 1, 1, 3, 2]
    };

    for &i in &indices {
        vertices.push(Vertex::new(
            corner_pos[i],
            face.normal,
            corner_ao[i],
            color_index,
        ));
    }
}

/// Generate mesh for a single chunk using greedy meshing with AO.
pub fn generate_chunk_mesh(world: &ChunkManager, chunk_pos: ChunkPosition) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    let chunk = match world.get_chunk(chunk_pos) {
        Some(c) => c,
        None => return vertices,
    };

    let (origin_x, origin_y, origin_z) = chunk_pos.world_origin();
    let cs = CHUNK_SIZE;

    for dir in &GREEDY_DIRS {
        // For each slice along the normal axis
        for slice in 0..cs {
            // Build mask: which cells have an exposed face in this direction?
            let mut mask = [[0u8; CHUNK_SIZE]; CHUNK_SIZE]; // [v][u], 0 = no face

            for v in 0..cs {
                for u in 0..cs {
                    let (x, y, z) = (dir.axis_map)(slice, u, v);
                    let voxel = chunk.get(x, y, z);
                    if voxel == AIR {
                        continue;
                    }

                    // Check neighbor along normal direction
                    let (nx, ny, nz) = match dir.normal_axis {
                        0 => (x as i32 + dir.normal_step, y as i32, z as i32),
                        1 => (x as i32, y as i32 + dir.normal_step, z as i32),
                        _ => (x as i32, y as i32, z as i32 + dir.normal_step),
                    };

                    let wx = origin_x + nx;
                    let wy = origin_y + ny;
                    let wz = origin_z + nz;

                    if !world.is_solid(wx, wy, wz) {
                        mask[v][u] = voxel;
                    }
                }
            }

            // Greedy merge the mask
            let mut visited = [[false; CHUNK_SIZE]; CHUNK_SIZE];

            for v0 in 0..cs {
                for u0 in 0..cs {
                    let mat = mask[v0][u0];
                    if mat == 0 || visited[v0][u0] {
                        continue;
                    }

                    // Expand right (u direction)
                    let mut w = 1;
                    while u0 + w < cs && mask[v0][u0 + w] == mat && !visited[v0][u0 + w] {
                        w += 1;
                    }

                    // Expand down (v direction)
                    let mut h = 1;
                    'outer: while v0 + h < cs {
                        for u in u0..u0 + w {
                            if mask[v0 + h][u] != mat || visited[v0 + h][u] {
                                break 'outer;
                            }
                        }
                        h += 1;
                    }

                    // Mark visited
                    for v in v0..v0 + h {
                        for u in u0..u0 + w {
                            visited[v][u] = true;
                        }
                    }

                    // Emit quad
                    emit_greedy_quad(
                        &mut vertices,
                        &dir.face,
                        dir,
                        world,
                        origin_x,
                        origin_y,
                        origin_z,
                        slice,
                        u0,
                        v0,
                        w,
                        h,
                        mat,
                    );
                }
            }
        }
    }

    vertices
}

/// Generate mesh for an octree LOD node using greedy meshing (no AO).
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

            greedy_mesh_lod(
                &mut vertices,
                mesh_data,
                res,
                cell_scale,
                origin_wx,
                origin_wy,
                origin_wz,
            );
        }
    }

    vertices
}

/// Greedy mesh a VoxelData grid for LOD (no AO).
fn greedy_mesh_lod(
    vertices: &mut Vec<Vertex>,
    data: &VoxelData,
    res: usize,
    cell_scale: f32,
    origin_wx: f32,
    origin_wy: f32,
    origin_wz: f32,
) {
    // For each face direction
    for dir_idx in 0..6 {
        let dir = &GREEDY_DIRS[dir_idx];

        for slice in 0..res {
            // Build mask
            let mut mask = vec![vec![0u8; res]; res]; // [v][u]

            for v in 0..res {
                for u in 0..res {
                    let (x, y, z) = (dir.axis_map)(slice, u, v);
                    let voxel = data.get_native(x, y, z);
                    if voxel == AIR {
                        continue;
                    }

                    // Check neighbor
                    let (nx, ny, nz) = match dir.normal_axis {
                        0 => (x as i32 + dir.normal_step, y as i32, z as i32),
                        1 => (x as i32, y as i32 + dir.normal_step, z as i32),
                        _ => (x as i32, y as i32, z as i32 + dir.normal_step),
                    };

                    let neighbor_air = if nx < 0
                        || ny < 0
                        || nz < 0
                        || nx >= res as i32
                        || ny >= res as i32
                        || nz >= res as i32
                    {
                        true
                    } else {
                        data.get_native(nx as usize, ny as usize, nz as usize) == AIR
                    };

                    if neighbor_air {
                        mask[v][u] = voxel;
                    }
                }
            }

            // Greedy merge
            let mut visited = vec![vec![false; res]; res];

            for v0 in 0..res {
                for u0 in 0..res {
                    let mat = mask[v0][u0];
                    if mat == 0 || visited[v0][u0] {
                        continue;
                    }

                    let mut w = 1;
                    while u0 + w < res && mask[v0][u0 + w] == mat && !visited[v0][u0 + w] {
                        w += 1;
                    }

                    let mut h = 1;
                    'outer: while v0 + h < res {
                        for u in u0..u0 + w {
                            if mask[v0 + h][u] != mat || visited[v0 + h][u] {
                                break 'outer;
                            }
                        }
                        h += 1;
                    }

                    for v in v0..v0 + h {
                        for u in u0..u0 + w {
                            visited[v][u] = true;
                        }
                    }

                    // Emit merged quad for LOD (no AO)
                    emit_lod_quad(
                        vertices, &dir.face, dir, cell_scale, origin_wx, origin_wy, origin_wz,
                        slice, u0, v0, w, h, mat,
                    );
                }
            }
        }
    }
}

/// Emit a merged quad for LOD meshing (no AO).
#[allow(clippy::too_many_arguments)]
fn emit_lod_quad(
    vertices: &mut Vec<Vertex>,
    face: &Face,
    dir: &GreedyFaceDir,
    cell_scale: f32,
    origin_wx: f32,
    origin_wy: f32,
    origin_wz: f32,
    slice: usize,
    u0: usize,
    v0: usize,
    w: usize,
    h: usize,
    color_index: Voxel,
) {
    let (u_comp, v_comp) = match dir.normal_axis {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };

    let mut corner_pos = [[0.0f32; 3]; 4];

    for i in 0..4 {
        let fv = face.vertices[i];
        let u_frac = fv[u_comp];
        let v_frac = fv[v_comp];

        let u_local = u0 as f32 + u_frac * w as f32;
        let v_local = v0 as f32 + v_frac * h as f32;

        let (lx, ly, lz) = match dir.normal_axis {
            0 => {
                let x = slice as f32 + fv[0];
                (x, v_local, u_local)
            }
            1 => {
                let y = slice as f32 + fv[1];
                (u_local, y, v_local)
            }
            _ => {
                let z = slice as f32 + fv[2];
                (u_local, v_local, z)
            }
        };

        corner_pos[i] = [
            origin_wx + lx * cell_scale,
            origin_wy + ly * cell_scale,
            origin_wz + lz * cell_scale,
        ];
    }

    let indices = [0, 2, 1, 0, 3, 2];
    for &i in &indices {
        vertices.push(Vertex::new(corner_pos[i], face.normal, 1.0, color_index));
    }
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
            1.0,
            color_index,
        ));
    }
}

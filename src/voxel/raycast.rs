use glam::Vec3;

use super::chunk::VOXEL_SCALE;

pub struct RaycastHit {
    pub block_pos: [i32; 3],
    pub normal: [i32; 3],
}

/// DDA raycasting algorithm to find the first solid voxel hit
/// Works in world coordinates, returns voxel coordinates
pub fn raycast<F>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    is_solid: F,
) -> Option<RaycastHit>
where
    F: Fn(i32, i32, i32) -> bool,
{
    let dir = direction.normalize();

    // Convert world origin to voxel coordinates
    let voxel_origin = origin / VOXEL_SCALE;

    // Current voxel position
    let mut x = voxel_origin.x.floor() as i32;
    let mut y = voxel_origin.y.floor() as i32;
    let mut z = voxel_origin.z.floor() as i32;

    // Direction signs
    let step_x = if dir.x >= 0.0 { 1 } else { -1 };
    let step_y = if dir.y >= 0.0 { 1 } else { -1 };
    let step_z = if dir.z >= 0.0 { 1 } else { -1 };

    // Distance to next voxel boundary for each axis (in world units)
    let t_delta_x = if dir.x != 0.0 {
        (VOXEL_SCALE / dir.x).abs()
    } else {
        f32::MAX
    };
    let t_delta_y = if dir.y != 0.0 {
        (VOXEL_SCALE / dir.y).abs()
    } else {
        f32::MAX
    };
    let t_delta_z = if dir.z != 0.0 {
        (VOXEL_SCALE / dir.z).abs()
    } else {
        f32::MAX
    };

    // Initial t values (distance to first voxel boundary in world units)
    let mut t_max_x = if dir.x != 0.0 {
        let boundary = if dir.x > 0.0 {
            (x + 1) as f32 * VOXEL_SCALE
        } else {
            x as f32 * VOXEL_SCALE
        };
        (boundary - origin.x) / dir.x
    } else {
        f32::MAX
    };

    let mut t_max_y = if dir.y != 0.0 {
        let boundary = if dir.y > 0.0 {
            (y + 1) as f32 * VOXEL_SCALE
        } else {
            y as f32 * VOXEL_SCALE
        };
        (boundary - origin.y) / dir.y
    } else {
        f32::MAX
    };

    let mut t_max_z = if dir.z != 0.0 {
        let boundary = if dir.z > 0.0 {
            (z + 1) as f32 * VOXEL_SCALE
        } else {
            z as f32 * VOXEL_SCALE
        };
        (boundary - origin.z) / dir.z
    } else {
        f32::MAX
    };

    let mut distance = 0.0;
    let mut normal = [0i32; 3];

    while distance < max_distance {
        // Check current block
        if is_solid(x, y, z) {
            return Some(RaycastHit {
                block_pos: [x, y, z],
                normal,
            });
        }

        // Step to next block
        if t_max_x < t_max_y && t_max_x < t_max_z {
            distance = t_max_x;
            x += step_x;
            t_max_x += t_delta_x;
            normal = [-step_x, 0, 0];
        } else if t_max_y < t_max_z {
            distance = t_max_y;
            y += step_y;
            t_max_y += t_delta_y;
            normal = [0, -step_y, 0];
        } else {
            distance = t_max_z;
            z += step_z;
            t_max_z += t_delta_z;
            normal = [0, 0, -step_z];
        }
    }

    None
}

use glam::Vec3;

pub struct RaycastHit {
    pub block_pos: [i32; 3],
    pub normal: [i32; 3],
    pub distance: f32,
}

/// DDA raycasting algorithm to find the first solid block hit
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

    // Current block position
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    // Direction signs
    let step_x = if dir.x >= 0.0 { 1 } else { -1 };
    let step_y = if dir.y >= 0.0 { 1 } else { -1 };
    let step_z = if dir.z >= 0.0 { 1 } else { -1 };

    // Distance to next block boundary for each axis
    let t_delta_x = if dir.x != 0.0 { (1.0 / dir.x).abs() } else { f32::MAX };
    let t_delta_y = if dir.y != 0.0 { (1.0 / dir.y).abs() } else { f32::MAX };
    let t_delta_z = if dir.z != 0.0 { (1.0 / dir.z).abs() } else { f32::MAX };

    // Initial t values
    let mut t_max_x = if dir.x != 0.0 {
        let boundary = if dir.x > 0.0 { (x + 1) as f32 } else { x as f32 };
        (boundary - origin.x) / dir.x
    } else {
        f32::MAX
    };

    let mut t_max_y = if dir.y != 0.0 {
        let boundary = if dir.y > 0.0 { (y + 1) as f32 } else { y as f32 };
        (boundary - origin.y) / dir.y
    } else {
        f32::MAX
    };

    let mut t_max_z = if dir.z != 0.0 {
        let boundary = if dir.z > 0.0 { (z + 1) as f32 } else { z as f32 };
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
                distance,
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

use glam::Vec3;

use crate::voxel::VOXEL_SCALE;
use crate::world::ChunkManager;

/// Trait for worlds that support collision checking
pub trait CollisionWorld {
    fn is_solid(&self, x: i32, y: i32, z: i32) -> bool;
}

impl CollisionWorld for ChunkManager {
    fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.is_solid(x, y, z)
    }
}

#[allow(dead_code)]
pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,

    // AABB size (half-extents)
    pub width: f32,  // X and Z half-size
    pub height: f32, // Y full height
}

#[allow(dead_code)]
impl Player {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            on_ground: false,
            width: 0.3,  // 0.6 total width
            height: 1.8, // 1.8 blocks tall
        }
    }

    pub fn eye_position(&self) -> Vec3 {
        eye_position_free(self.position, self.height)
    }

    pub fn update<W: CollisionWorld>(&mut self, world: &W, dt: f32) {
        physics_update_free(
            &mut self.position,
            &mut self.velocity,
            &mut self.on_ground,
            self.width,
            self.height,
            world,
            dt,
        );
    }

    pub fn jump(&mut self) {
        jump_free(&mut self.velocity, &mut self.on_ground);
    }

    pub fn apply_movement(&mut self, forward: Vec3, right: Vec3, input: Vec3, speed: f32) {
        apply_movement_free(&mut self.velocity, forward, right, input, speed);
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Get eye position from player position and height
pub fn eye_position_free(position: Vec3, height: f32) -> Vec3 {
    position + Vec3::new(0.0, height - 0.2, 0.0)
}

/// Apply camera-relative movement input to velocity
pub fn apply_movement_free(
    velocity: &mut Vec3,
    forward: Vec3,
    right: Vec3,
    input: Vec3,
    speed: f32,
) {
    let move_dir = (forward * input.z + right * input.x).normalize_or_zero();
    velocity.x += move_dir.x * speed;
    velocity.z += move_dir.z * speed;
}

/// Attempt to jump (only if on ground)
pub fn jump_free(velocity: &mut Vec3, on_ground: &mut bool) {
    if *on_ground {
        velocity.y = 9.0;
        *on_ground = false;
    }
}

/// Full physics update: gravity, movement, collision, friction
pub fn physics_update_free<W: CollisionWorld>(
    position: &mut Vec3,
    velocity: &mut Vec3,
    on_ground: &mut bool,
    width: f32,
    height: f32,
    world: &W,
    dt: f32,
) {
    const GRAVITY: f32 = 28.0;
    const TERMINAL_VELOCITY: f32 = 50.0;

    // Always apply gravity
    velocity.y -= GRAVITY * dt;
    velocity.y = velocity.y.max(-TERMINAL_VELOCITY);

    // Reset on_ground, will be set true if we hit ground
    *on_ground = false;

    // Move and collide each axis separately
    move_axis_free(position, velocity, on_ground, width, height, world, Vec3::X, velocity.x * dt);
    move_axis_free(position, velocity, on_ground, width, height, world, Vec3::Y, velocity.y * dt);
    move_axis_free(position, velocity, on_ground, width, height, world, Vec3::Z, velocity.z * dt);

    // Friction - frame-rate independent
    const FRICTION: f32 = 0.8;
    const TARGET_FPS: f32 = 60.0;
    let friction = FRICTION.powf(dt * TARGET_FPS);
    velocity.x *= friction;
    velocity.z *= friction;
}

#[allow(clippy::too_many_arguments)]
fn move_axis_free<W: CollisionWorld>(
    position: &mut Vec3,
    velocity: &mut Vec3,
    on_ground: &mut bool,
    width: f32,
    height: f32,
    world: &W,
    axis: Vec3,
    delta: f32,
) {
    if delta.abs() < 0.0001 {
        return;
    }

    // Auto-step small voxel stairs when moving horizontally.
    if axis != Vec3::Y && try_step_up_free(position, on_ground, width, height, world, axis, delta) {
        return;
    }

    let new_pos = *position + axis * delta;

    if check_collision_free(world, new_pos, width, height) {
        // Collision detected - sweep to find contact point
        let steps = (delta.abs().ceil() as i32).max(1) * 4;
        let step_delta = delta / steps as f32;

        for _ in 0..steps {
            let test_pos = *position + axis * step_delta;
            if check_collision_free(world, test_pos, width, height) {
                break;
            }
            *position = test_pos;
        }

        if axis == Vec3::Y {
            if delta < 0.0 {
                *on_ground = true;
            }
            velocity.y = 0.0;
        }
    } else {
        *position = new_pos;
    }
}

fn try_step_up_free<W: CollisionWorld>(
    position: &mut Vec3,
    on_ground: &mut bool,
    width: f32,
    height: f32,
    world: &W,
    axis: Vec3,
    delta: f32,
) -> bool {
    let step_height = VOXEL_SCALE + 0.06;
    let steps = (delta.abs().ceil() as i32).max(1) * 4;
    let step_delta = delta / steps as f32;

    let mut moved = false;

    for _ in 0..steps {
        let horiz_test = *position + axis * step_delta;
        if !check_collision_free(world, horiz_test, width, height) {
            *position = horiz_test;
            moved = true;
            continue;
        }

        let up_test = *position + Vec3::Y * step_height;
        if check_collision_free(world, up_test, width, height) {
            return moved;
        }

        let stepped_test = up_test + axis * step_delta;
        if check_collision_free(world, stepped_test, width, height) {
            return moved;
        }

        *position = stepped_test;
        moved = true;

        // Settle down onto the stair top so movement stays grounded.
        let drop_steps = 4;
        let drop_step = step_height / drop_steps as f32;
        for _ in 0..drop_steps {
            let down_test = *position - Vec3::Y * drop_step;
            if check_collision_free(world, down_test, width, height) {
                *on_ground = true;
                break;
            }
            *position = down_test;
        }
    }

    moved
}

fn check_collision_free<W: CollisionWorld>(world: &W, pos: Vec3, width: f32, height: f32) -> bool {
    let min_x = ((pos.x - width) / VOXEL_SCALE).floor() as i32;
    let max_x = ((pos.x + width) / VOXEL_SCALE).floor() as i32;
    let min_y = (pos.y / VOXEL_SCALE).floor() as i32;
    let max_y = ((pos.y + height) / VOXEL_SCALE).floor() as i32;
    let min_z = ((pos.z - width) / VOXEL_SCALE).floor() as i32;
    let max_z = ((pos.z + width) / VOXEL_SCALE).floor() as i32;

    for bx in min_x..=max_x {
        for by in min_y..=max_y {
            for bz in min_z..=max_z {
                if world.is_solid(bx, by, bz)
                    && aabb_intersects_block_free(pos, width, height, bx, by, bz)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn aabb_intersects_block_free(pos: Vec3, width: f32, height: f32, bx: i32, by: i32, bz: i32) -> bool {
    let player_min = Vec3::new(pos.x - width, pos.y, pos.z - width);
    let player_max = Vec3::new(pos.x + width, pos.y + height, pos.z + width);

    let block_min = Vec3::new(
        bx as f32 * VOXEL_SCALE,
        by as f32 * VOXEL_SCALE,
        bz as f32 * VOXEL_SCALE,
    );
    let block_max = block_min + Vec3::splat(VOXEL_SCALE);

    player_min.x < block_max.x
        && player_max.x > block_min.x
        && player_min.y < block_max.y
        && player_max.y > block_min.y
        && player_min.z < block_max.z
        && player_max.z > block_min.z
}

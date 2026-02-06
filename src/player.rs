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

pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,

    // AABB size (half-extents)
    pub width: f32,  // X and Z half-size
    pub height: f32, // Y full height
}

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
        self.position + Vec3::new(0.0, self.height - 0.2, 0.0)
    }

    pub fn update<W: CollisionWorld>(&mut self, world: &W, dt: f32) {
        const GRAVITY: f32 = 28.0;
        const TERMINAL_VELOCITY: f32 = 50.0;

        // Always apply gravity
        self.velocity.y -= GRAVITY * dt;
        self.velocity.y = self.velocity.y.max(-TERMINAL_VELOCITY);

        // Reset on_ground, will be set true if we hit ground
        self.on_ground = false;

        // Move and collide each axis separately
        self.move_axis(world, Vec3::X, self.velocity.x * dt);
        self.move_axis(world, Vec3::Y, self.velocity.y * dt);
        self.move_axis(world, Vec3::Z, self.velocity.z * dt);

        // Friction - frame-rate independent
        // Use exponential decay: friction^(dt * target_fps) keeps behavior consistent
        const FRICTION: f32 = 0.8;
        const TARGET_FPS: f32 = 60.0;
        let friction = FRICTION.powf(dt * TARGET_FPS);
        self.velocity.x *= friction;
        self.velocity.z *= friction;
    }

    fn move_axis<W: CollisionWorld>(&mut self, world: &W, axis: Vec3, delta: f32) {
        if delta.abs() < 0.0001 {
            return;
        }

        let new_pos = self.position + axis * delta;

        if self.check_collision(world, new_pos) {
            // Collision detected - sweep to find contact point
            let steps = (delta.abs().ceil() as i32).max(1) * 4;
            let step_delta = delta / steps as f32;

            for _ in 0..steps {
                let test_pos = self.position + axis * step_delta;
                if self.check_collision(world, test_pos) {
                    break;
                }
                self.position = test_pos;
            }

            if axis == Vec3::Y {
                if delta < 0.0 {
                    self.on_ground = true;
                }
                self.velocity.y = 0.0;
            }
        } else {
            self.position = new_pos;
        }
    }

    fn check_collision<W: CollisionWorld>(&self, world: &W, pos: Vec3) -> bool {
        // Convert world position to voxel coordinates
        // Check all voxels that the player AABB might intersect
        let min_x = ((pos.x - self.width) / VOXEL_SCALE).floor() as i32;
        let max_x = ((pos.x + self.width) / VOXEL_SCALE).floor() as i32;
        let min_y = (pos.y / VOXEL_SCALE).floor() as i32;
        let max_y = ((pos.y + self.height) / VOXEL_SCALE).floor() as i32;
        let min_z = ((pos.z - self.width) / VOXEL_SCALE).floor() as i32;
        let max_z = ((pos.z + self.width) / VOXEL_SCALE).floor() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    if world.is_solid(bx, by, bz) {
                        // AABB vs voxel AABB test
                        if self.aabb_intersects_block(pos, bx, by, bz) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn aabb_intersects_block(&self, pos: Vec3, bx: i32, by: i32, bz: i32) -> bool {
        let player_min = Vec3::new(pos.x - self.width, pos.y, pos.z - self.width);
        let player_max = Vec3::new(pos.x + self.width, pos.y + self.height, pos.z + self.width);

        // Voxel bounds in world coordinates
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

    pub fn jump(&mut self) {
        if self.on_ground {
            self.velocity.y = 9.0;
            self.on_ground = false;
        }
    }

    pub fn intersects_block(&self, bx: i32, by: i32, bz: i32) -> bool {
        self.aabb_intersects_block(self.position, bx, by, bz)
    }

    pub fn apply_movement(&mut self, forward: Vec3, right: Vec3, input: Vec3, speed: f32) {
        let move_dir = (forward * input.z + right * input.x).normalize_or_zero();
        self.velocity.x += move_dir.x * speed;
        self.velocity.z += move_dir.z * speed;
    }
}

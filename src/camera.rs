use glam::{Mat4, Quat, Vec3, Vec4};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,

    // Head bob
    bob_time: f32,
    bob_intensity: f32,

    // Animation shake (from first-person model)
    shake_offset: Vec3,
    shake_rotation: Quat,
}

impl Camera {
    pub fn new(position: Vec3, aspect: f32) -> Self {
        Self {
            position,
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
            aspect,
            fov: 70.0_f32.to_radians(),
            near: 0.1,
            far: 1000.0,
            bob_time: 0.0,
            bob_intensity: 0.0,
            shake_offset: Vec3::ZERO,
            shake_rotation: Quat::IDENTITY,
        }
    }

    /// Apply animation shake from first-person model
    pub fn apply_shake(&mut self, offset: Vec3, rotation: Quat) {
        self.shake_offset = offset;
        self.shake_rotation = rotation;
    }

    /// Get current head bob offset
    fn bob_offset(&self) -> Vec3 {
        if self.bob_intensity < 0.001 {
            return Vec3::ZERO;
        }

        // Vertical bob (up/down)
        const BOB_HEIGHT: f32 = 0.08;
        // Horizontal bob (side-to-side)
        const BOB_SIDE: f32 = 0.03;

        let vertical = self.bob_time.sin().abs() * BOB_HEIGHT * self.bob_intensity;
        let horizontal = (self.bob_time * 0.5).sin() * BOB_SIDE * self.bob_intensity;

        let right = self.right();
        Vec3::new(0.0, vertical, 0.0) + right * horizontal
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        let bob = self.bob_offset();
        let pos = self.position + bob + self.shake_offset;

        // Build base rotation from yaw/pitch
        // Subtract 90 degrees to yaw to match forward() convention (yaw=0 looks at +X)
        let yaw_quat = Quat::from_rotation_y(-self.yaw - std::f32::consts::FRAC_PI_2);
        let pitch_quat = Quat::from_rotation_x(self.pitch);
        let base_rotation = yaw_quat * pitch_quat;

        // Apply animation shake rotation
        let final_rotation = base_rotation * self.shake_rotation;

        // Compute forward and up from final rotation
        let forward = final_rotation * Vec3::NEG_Z;
        let up = final_rotation * Vec3::Y;

        let target = pos + forward;
        Mat4::look_at_rh(pos, target, up)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov, self.aspect, self.near, self.far)
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn process_mouse(&mut self, delta_x: f32, delta_y: f32, sensitivity: f32) {
        self.yaw += delta_x * sensitivity;
        self.pitch -= delta_y * sensitivity;

        let max_pitch = 89.0_f32.to_radians();
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if height > 0 {
            self.aspect = width as f32 / height as f32;
        }
    }

    /// Extract 6 frustum planes from the view-projection matrix.
    /// Each plane is (normal.x, normal.y, normal.z, distance) with inward-pointing normals.
    pub fn frustum_planes(&self) -> [Vec4; 6] {
        let vp = self.view_projection_matrix();
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);

        let mut planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r3 + r2, // near
            r3 - r2, // far
        ];

        // Normalize each plane
        for p in &mut planes {
            let len = Vec3::new(p.x, p.y, p.z).length();
            if len > 0.0 {
                *p /= len;
            }
        }

        planes
    }
}

/// Test if an AABB is at least partially inside the frustum.
/// Returns true if visible, false if fully outside.
pub fn aabb_in_frustum(min: Vec3, max: Vec3, planes: &[Vec4; 6]) -> bool {
    for plane in planes {
        let n = Vec3::new(plane.x, plane.y, plane.z);
        // Find the "positive vertex" — the corner most in the direction of the normal
        let pv = Vec3::new(
            if n.x >= 0.0 { max.x } else { min.x },
            if n.y >= 0.0 { max.y } else { min.y },
            if n.z >= 0.0 { max.z } else { min.z },
        );
        // If the positive vertex is behind the plane, the AABB is fully outside
        if n.dot(pv) + plane.w < 0.0 {
            return false;
        }
    }
    true
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    world_position: [f32; 3],
    _padding: f32,
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            world_position: [0.0; 3],
            _padding: 0.0,
        }
    }

    pub fn update(&mut self, camera: &Camera) {
        self.view_proj = camera.view_projection_matrix().to_cols_array_2d();
        self.world_position = camera.position.to_array();
    }
}

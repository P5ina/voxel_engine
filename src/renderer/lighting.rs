/// Lighting parameters for path tracing
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingParams {
    pub sun_direction: [f32; 3],
    pub sun_intensity: f32,
    pub sun_color: [f32; 3],
    pub _padding: f32,
}

impl LightingParams {
    pub fn new() -> Self {
        Self {
            sun_direction: [0.5, 1.0, 0.3],
            sun_intensity: 1.0,
            sun_color: [1.0, 0.98, 0.9],
            _padding: 0.0,
        }
    }

    pub fn update_time(&mut self, time: f32) {
        let time = time % 1.0;
        let angle = time * std::f32::consts::TAU;

        let sun_y = angle.cos();
        let sun_z = angle.sin();
        let sun_x = 0.3;

        let len = (sun_x * sun_x + sun_y * sun_y + sun_z * sun_z).sqrt();
        self.sun_direction = [sun_x / len, sun_y / len, sun_z / len];

        let sun_height = sun_y.max(0.0);
        self.sun_intensity = sun_height.powf(0.5);

        if sun_y > 0.0 {
            let dawn_dusk_factor = (1.0 - sun_height).powf(2.0) * sun_height.min(0.5) * 4.0;
            let r = 1.0 + dawn_dusk_factor * 0.3;
            let g = 0.95 - dawn_dusk_factor * 0.3;
            let b = 0.85 - dawn_dusk_factor * 0.5;
            self.sun_color = [r.min(1.0), g.max(0.5), b.max(0.3)];
        } else {
            self.sun_color = [0.4, 0.45, 0.6];
            self.sun_intensity = 0.15;
        }
    }
}

impl Default for LightingParams {
    fn default() -> Self {
        Self::new()
    }
}

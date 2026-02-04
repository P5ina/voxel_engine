use bytemuck::{Pod, Zeroable};

/// Material properties for path tracing
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Material {
    pub albedo: [f32; 3],
    pub roughness: f32,
    pub emission: [f32; 3],
    pub metallic: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            albedo: [0.5, 0.5, 0.5],
            roughness: 0.5,
            emission: [0.0, 0.0, 0.0],
            metallic: 0.0,
        }
    }
}

impl Material {
    pub const fn new(albedo: [f32; 3], roughness: f32, emission: [f32; 3], metallic: f32) -> Self {
        Self {
            albedo,
            roughness,
            emission,
            metallic,
        }
    }
}

/// Block materials indexed by BlockType
/// Index 0 = Air, 1 = Dirt, 2 = Stone, 3 = Grass
pub const BLOCK_MATERIALS: [Material; 4] = [
    // Air - transparent, no material properties
    Material {
        albedo: [0.0, 0.0, 0.0],
        roughness: 1.0,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    // Dirt - brown, rough
    Material {
        albedo: [0.55, 0.35, 0.2],
        roughness: 0.9,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    // Stone - gray, moderately rough
    Material {
        albedo: [0.5, 0.5, 0.5],
        roughness: 0.7,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    // Grass - green, rough
    Material {
        albedo: [0.3, 0.6, 0.2],
        roughness: 0.85,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
];

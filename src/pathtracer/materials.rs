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
            roughness: 0.8,
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

    pub const fn color(r: f32, g: f32, b: f32) -> Self {
        Self {
            albedo: [r, g, b],
            roughness: 0.8,
            emission: [0.0, 0.0, 0.0],
            metallic: 0.0,
        }
    }

    pub const fn emissive(r: f32, g: f32, b: f32, intensity: f32) -> Self {
        Self {
            albedo: [r, g, b],
            roughness: 1.0,
            emission: [r * intensity, g * intensity, b * intensity],
            metallic: 0.0,
        }
    }
}

/// 256-color palette for voxels.
/// Index 0 = Air (transparent), 1-254 = solid colors, 255 = emissive light
pub struct Palette {
    pub materials: [Material; 256],
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

impl Palette {
    pub fn new() -> Self {
        let mut materials = [Material::default(); 256];

        // Index 0: Air (transparent)
        materials[0] = Material::new([0.0, 0.0, 0.0], 1.0, [0.0, 0.0, 0.0], 0.0);

        // Grayscale (1-16)
        for i in 0..16 {
            let v = i as f32 / 15.0;
            materials[1 + i] = Material::color(v, v, v);
        }

        // Basic colors (17-32)
        materials[17] = Material::color(1.0, 0.0, 0.0); // Red
        materials[18] = Material::color(0.0, 1.0, 0.0); // Green
        materials[19] = Material::color(0.0, 0.0, 1.0); // Blue
        materials[20] = Material::color(1.0, 1.0, 0.0); // Yellow
        materials[21] = Material::color(1.0, 0.0, 1.0); // Magenta
        materials[22] = Material::color(0.0, 1.0, 1.0); // Cyan
        materials[23] = Material::color(1.0, 0.5, 0.0); // Orange
        materials[24] = Material::color(0.5, 0.0, 1.0); // Purple
        materials[25] = Material::color(1.0, 0.5, 0.5); // Light red
        materials[26] = Material::color(0.5, 1.0, 0.5); // Light green
        materials[27] = Material::color(0.5, 0.5, 1.0); // Light blue
        materials[28] = Material::color(0.5, 0.25, 0.0); // Brown
        materials[29] = Material::color(0.0, 0.5, 0.0); // Dark green
        materials[30] = Material::color(0.0, 0.0, 0.5); // Dark blue
        materials[31] = Material::color(0.5, 0.0, 0.0); // Dark red
        materials[32] = Material::color(0.3, 0.3, 0.3); // Dark gray

        // Earth tones (33-48)
        materials[33] = Material::color(0.55, 0.35, 0.2); // Dirt
        materials[34] = Material::color(0.6, 0.4, 0.25); // Light dirt
        materials[35] = Material::color(0.4, 0.25, 0.15); // Dark dirt
        materials[36] = Material::color(0.8, 0.7, 0.5); // Sand
        materials[37] = Material::color(0.9, 0.85, 0.7); // Light sand
        materials[38] = Material::color(0.6, 0.5, 0.35); // Dark sand
        materials[39] = Material::color(0.4, 0.35, 0.3); // Clay
        materials[40] = Material::color(0.7, 0.4, 0.3); // Terracotta
        materials[41] = Material::color(0.3, 0.25, 0.2); // Dark earth
        materials[42] = Material::color(0.5, 0.45, 0.4); // Stone gray
        materials[43] = Material::color(0.6, 0.55, 0.5); // Light stone
        materials[44] = Material::color(0.35, 0.3, 0.25); // Dark stone
        materials[45] = Material::color(0.45, 0.4, 0.35); // Cobblestone
        materials[46] = Material::color(0.7, 0.65, 0.6); // Limestone
        materials[47] = Material::color(0.25, 0.2, 0.18); // Basalt
        materials[48] = Material::color(0.8, 0.75, 0.7); // Marble

        // Greens / Nature (49-64)
        materials[49] = Material::color(0.3, 0.6, 0.2); // Grass green
        materials[50] = Material::color(0.4, 0.7, 0.3); // Light grass
        materials[51] = Material::color(0.2, 0.4, 0.15); // Dark grass
        materials[52] = Material::color(0.1, 0.3, 0.1); // Forest green
        materials[53] = Material::color(0.5, 0.8, 0.4); // Lime
        materials[54] = Material::color(0.6, 0.5, 0.3); // Dead grass
        materials[55] = Material::color(0.3, 0.5, 0.3); // Moss
        materials[56] = Material::color(0.2, 0.35, 0.2); // Dark moss
        materials[57] = Material::color(0.15, 0.25, 0.1); // Pine green
        materials[58] = Material::color(0.4, 0.55, 0.35); // Sage
        materials[59] = Material::color(0.55, 0.65, 0.45); // Olive
        materials[60] = Material::color(0.0, 0.4, 0.3); // Teal green
        materials[61] = Material::color(0.3, 0.45, 0.25); // Fern
        materials[62] = Material::color(0.5, 0.6, 0.3); // Yellow-green
        materials[63] = Material::color(0.25, 0.5, 0.25); // Medium green
        materials[64] = Material::color(0.35, 0.55, 0.4); // Sea green

        // Blues (65-80)
        materials[65] = Material::color(0.2, 0.4, 0.8); // Sky blue
        materials[66] = Material::color(0.1, 0.2, 0.5); // Deep blue
        materials[67] = Material::color(0.4, 0.6, 0.9); // Light sky
        materials[68] = Material::color(0.0, 0.3, 0.6); // Ocean blue
        materials[69] = Material::color(0.3, 0.5, 0.7); // Steel blue
        materials[70] = Material::color(0.1, 0.1, 0.3); // Navy
        materials[71] = Material::color(0.4, 0.7, 0.8); // Cyan-ish
        materials[72] = Material::color(0.2, 0.3, 0.5); // Slate blue
        materials[73] = Material::color(0.5, 0.7, 0.9); // Powder blue
        materials[74] = Material::color(0.15, 0.25, 0.4); // Dark slate
        materials[75] = Material::color(0.3, 0.4, 0.6); // Denim
        materials[76] = Material::color(0.0, 0.5, 0.8); // Azure
        materials[77] = Material::color(0.2, 0.5, 0.6); // Teal blue
        materials[78] = Material::color(0.4, 0.5, 0.6); // Blue gray
        materials[79] = Material::color(0.1, 0.4, 0.7); // Cobalt
        materials[80] = Material::color(0.6, 0.8, 1.0); // Ice blue

        // Reds / Warm (81-96)
        materials[81] = Material::color(0.8, 0.2, 0.2); // Brick red
        materials[82] = Material::color(0.6, 0.1, 0.1); // Dark red
        materials[83] = Material::color(1.0, 0.4, 0.4); // Salmon
        materials[84] = Material::color(0.7, 0.3, 0.2); // Rust
        materials[85] = Material::color(0.9, 0.5, 0.4); // Coral
        materials[86] = Material::color(0.5, 0.15, 0.15); // Maroon
        materials[87] = Material::color(0.8, 0.4, 0.3); // Terra cotta
        materials[88] = Material::color(1.0, 0.6, 0.5); // Peach
        materials[89] = Material::color(0.7, 0.2, 0.3); // Crimson
        materials[90] = Material::color(0.9, 0.3, 0.3); // Scarlet
        materials[91] = Material::color(0.6, 0.25, 0.2); // Auburn
        materials[92] = Material::color(0.8, 0.5, 0.4); // Rose
        materials[93] = Material::color(0.4, 0.1, 0.1); // Burgundy
        materials[94] = Material::color(1.0, 0.7, 0.6); // Light peach
        materials[95] = Material::color(0.65, 0.35, 0.3); // Copper
        materials[96] = Material::color(0.9, 0.6, 0.5); // Apricot

        // Purples / Pinks (97-112)
        materials[97] = Material::color(0.6, 0.3, 0.7); // Purple
        materials[98] = Material::color(0.4, 0.2, 0.5); // Deep purple
        materials[99] = Material::color(0.8, 0.5, 0.8); // Light purple
        materials[100] = Material::color(0.5, 0.0, 0.5); // Magenta dark
        materials[101] = Material::color(0.9, 0.6, 0.9); // Pink
        materials[102] = Material::color(0.7, 0.4, 0.6); // Mauve
        materials[103] = Material::color(0.3, 0.1, 0.4); // Indigo
        materials[104] = Material::color(0.8, 0.7, 0.9); // Lavender
        materials[105] = Material::color(0.6, 0.4, 0.8); // Violet
        materials[106] = Material::color(1.0, 0.7, 0.8); // Light pink
        materials[107] = Material::color(0.5, 0.3, 0.5); // Plum
        materials[108] = Material::color(0.9, 0.4, 0.6); // Hot pink
        materials[109] = Material::color(0.4, 0.3, 0.6); // Grape
        materials[110] = Material::color(0.7, 0.5, 0.7); // Orchid
        materials[111] = Material::color(0.3, 0.2, 0.3); // Dark plum
        materials[112] = Material::color(1.0, 0.8, 0.9); // Blush

        // Yellows / Oranges (113-128)
        materials[113] = Material::color(1.0, 0.9, 0.3); // Yellow
        materials[114] = Material::color(1.0, 0.8, 0.0); // Gold
        materials[115] = Material::color(1.0, 1.0, 0.6); // Light yellow
        materials[116] = Material::color(0.8, 0.7, 0.2); // Olive yellow
        materials[117] = Material::color(1.0, 0.6, 0.2); // Orange
        materials[118] = Material::color(0.9, 0.5, 0.1); // Dark orange
        materials[119] = Material::color(1.0, 0.7, 0.4); // Light orange
        materials[120] = Material::color(0.7, 0.5, 0.1); // Bronze
        materials[121] = Material::color(1.0, 0.85, 0.5); // Cream
        materials[122] = Material::color(0.9, 0.75, 0.3); // Mustard
        materials[123] = Material::color(1.0, 0.95, 0.8); // Ivory
        materials[124] = Material::color(0.8, 0.6, 0.2); // Amber
        materials[125] = Material::color(0.6, 0.45, 0.1); // Dark gold
        materials[126] = Material::color(1.0, 0.55, 0.0); // Tangerine
        materials[127] = Material::color(0.95, 0.9, 0.7); // Beige
        materials[128] = Material::color(0.85, 0.65, 0.35); // Caramel

        // Wood tones (129-144)
        materials[129] = Material::color(0.6, 0.4, 0.2); // Oak
        materials[130] = Material::color(0.4, 0.25, 0.1); // Walnut
        materials[131] = Material::color(0.8, 0.6, 0.4); // Pine
        materials[132] = Material::color(0.3, 0.15, 0.05); // Mahogany
        materials[133] = Material::color(0.7, 0.5, 0.3); // Birch
        materials[134] = Material::color(0.5, 0.35, 0.2); // Cherry
        materials[135] = Material::color(0.9, 0.75, 0.55); // Light wood
        materials[136] = Material::color(0.2, 0.1, 0.05); // Ebony
        materials[137] = Material::color(0.65, 0.45, 0.25); // Teak
        materials[138] = Material::color(0.75, 0.55, 0.35); // Cedar
        materials[139] = Material::color(0.55, 0.4, 0.25); // Chestnut
        materials[140] = Material::color(0.85, 0.7, 0.5); // Maple
        materials[141] = Material::color(0.45, 0.3, 0.15); // Dark oak
        materials[142] = Material::color(0.7, 0.55, 0.4); // Ash
        materials[143] = Material::color(0.35, 0.2, 0.1); // Rosewood
        materials[144] = Material::color(0.8, 0.65, 0.45); // Bamboo

        // Metals (145-160) - with metallic property
        materials[145] = Material::new([0.9, 0.9, 0.9], 0.1, [0.0, 0.0, 0.0], 1.0); // Silver
        materials[146] = Material::new([1.0, 0.85, 0.5], 0.2, [0.0, 0.0, 0.0], 1.0); // Gold
        materials[147] = Material::new([0.7, 0.45, 0.2], 0.3, [0.0, 0.0, 0.0], 1.0); // Copper
        materials[148] = Material::new([0.5, 0.5, 0.55], 0.2, [0.0, 0.0, 0.0], 1.0); // Iron
        materials[149] = Material::new([0.6, 0.6, 0.65], 0.15, [0.0, 0.0, 0.0], 1.0); // Steel
        materials[150] = Material::new([0.8, 0.8, 0.75], 0.1, [0.0, 0.0, 0.0], 1.0); // Platinum
        materials[151] = Material::new([0.6, 0.5, 0.4], 0.4, [0.0, 0.0, 0.0], 1.0); // Bronze
        materials[152] = Material::new([0.75, 0.7, 0.5], 0.25, [0.0, 0.0, 0.0], 1.0); // Brass
        materials[153] = Material::new([0.4, 0.4, 0.45], 0.3, [0.0, 0.0, 0.0], 1.0); // Lead
        materials[154] = Material::new([0.85, 0.85, 0.9], 0.05, [0.0, 0.0, 0.0], 1.0); // Chrome
        materials[155] = Material::new([0.5, 0.35, 0.25], 0.5, [0.0, 0.0, 0.0], 0.8); // Rust
        materials[156] = Material::new([0.3, 0.3, 0.35], 0.4, [0.0, 0.0, 0.0], 0.9); // Dark metal
        materials[157] = Material::new([0.7, 0.7, 0.75], 0.2, [0.0, 0.0, 0.0], 1.0); // Aluminum
        materials[158] = Material::new([0.85, 0.75, 0.6], 0.3, [0.0, 0.0, 0.0], 0.9); // Antique gold
        materials[159] = Material::new([0.55, 0.55, 0.6], 0.35, [0.0, 0.0, 0.0], 1.0); // Zinc
        materials[160] = Material::new([0.65, 0.6, 0.55], 0.45, [0.0, 0.0, 0.0], 0.7); // Weathered metal

        // Fill remaining slots (161-254) with a gradient spectrum
        for i in 161..255 {
            let t = (i - 161) as f32 / 93.0;
            // HSV-like hue rotation
            let (r, g, b) = hsv_to_rgb(t * 360.0, 0.7, 0.8);
            materials[i] = Material::color(r, g, b);
        }

        // Index 255: Light (emissive)
        materials[255] = Material::emissive(1.0, 0.95, 0.9, 10.0);

        Self { materials }
    }

    pub fn get(&self, index: u8) -> &Material {
        &self.materials[index as usize]
    }

    pub fn get_mut(&mut self, index: u8) -> &mut Material {
        &mut self.materials[index as usize]
    }

    pub fn as_slice(&self) -> &[Material] {
        &self.materials
    }
}

/// Convert HSV to RGB (h: 0-360, s: 0-1, v: 0-1)
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (r + m, g + m, b + m)
}

// Legacy constant for backward compatibility
pub const BLOCK_MATERIALS: [Material; 6] = [
    Material {
        albedo: [0.0, 0.0, 0.0],
        roughness: 1.0,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    Material {
        albedo: [0.55, 0.35, 0.2],
        roughness: 1.0,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    Material {
        albedo: [0.5, 0.5, 0.5],
        roughness: 0.0,
        emission: [0.0, 0.0, 0.0],
        metallic: 1.0,
    },
    Material {
        albedo: [0.3, 0.6, 0.2],
        roughness: 1.0,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
    Material {
        albedo: [1.0, 0.95, 0.8],
        roughness: 1.0,
        emission: [10.0, 9.0, 7.0],
        metallic: 0.0,
    },
    Material {
        albedo: [1.0, 0.5, 0.0],
        roughness: 0.8,
        emission: [0.0, 0.0, 0.0],
        metallic: 0.0,
    },
];

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BlockType {
    #[default]
    Air = 0,
    Dirt = 1,
    Stone = 2,
    Grass = 3,
}

impl BlockType {
    pub fn is_solid(&self) -> bool {
        !matches!(self, BlockType::Air)
    }

    pub fn color(&self) -> [f32; 3] {
        match self {
            BlockType::Air => [0.0, 0.0, 0.0],
            BlockType::Dirt => [0.55, 0.35, 0.2],
            BlockType::Stone => [0.5, 0.5, 0.5],
            BlockType::Grass => [0.3, 0.6, 0.2],
        }
    }
}

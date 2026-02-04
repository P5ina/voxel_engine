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
}

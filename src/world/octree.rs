use glam::IVec3;

use crate::voxel::{CHUNK_SIZE, Voxel};

use super::ChunkPosition;
use super::lod::{LodLevel, VoxelData, build_lod_from_children};
use super::position::LodNodeKey;

/// Node flags for octree nodes
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeFlags(u8);

impl NodeFlags {
    pub const IS_LEAF: u8 = 0b00000001;
    pub const IS_HOMOGENEOUS: u8 = 0b00000010;
    pub const MESH_DIRTY: u8 = 0b00000100;
    pub const LOD_DIRTY: u8 = 0b00001000;
    pub const LOADED: u8 = 0b00010000;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn is_leaf(&self) -> bool {
        self.0 & Self::IS_LEAF != 0
    }

    pub fn set_leaf(&mut self, value: bool) {
        if value {
            self.0 |= Self::IS_LEAF;
        } else {
            self.0 &= !Self::IS_LEAF;
        }
    }

    pub fn is_homogeneous(&self) -> bool {
        self.0 & Self::IS_HOMOGENEOUS != 0
    }

    pub fn set_homogeneous(&mut self, value: bool) {
        if value {
            self.0 |= Self::IS_HOMOGENEOUS;
        } else {
            self.0 &= !Self::IS_HOMOGENEOUS;
        }
    }

    pub fn is_mesh_dirty(&self) -> bool {
        self.0 & Self::MESH_DIRTY != 0
    }

    pub fn set_mesh_dirty(&mut self, value: bool) {
        if value {
            self.0 |= Self::MESH_DIRTY;
        } else {
            self.0 &= !Self::MESH_DIRTY;
        }
    }

    pub fn is_lod_dirty(&self) -> bool {
        self.0 & Self::LOD_DIRTY != 0
    }

    pub fn set_lod_dirty(&mut self, value: bool) {
        if value {
            self.0 |= Self::LOD_DIRTY;
        } else {
            self.0 &= !Self::LOD_DIRTY;
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.0 & Self::LOADED != 0
    }

    pub fn set_loaded(&mut self, value: bool) {
        if value {
            self.0 |= Self::LOADED;
        } else {
            self.0 &= !Self::LOADED;
        }
    }
}

/// Octree node
#[derive(Clone)]
pub struct OctreeNode {
    /// Bit flags for which children exist (8 bits for 8 octants)
    pub children_mask: u8,

    /// If leaf: index into voxel_data storage
    /// If internal: index into first child in nodes array
    pub data_index: u32,

    /// Index into voxel_data for this node's LOD data (u32::MAX = none)
    pub lod_data_index: u32,

    /// LOD level of this node (0 = chunk level, higher = coarser)
    pub lod_level: LodLevel,

    /// Various flags
    pub flags: NodeFlags,

    /// For homogeneous nodes: the single voxel value
    pub homogeneous_value: Voxel,
}

impl OctreeNode {
    pub fn new_leaf(data_index: u32, lod_level: LodLevel) -> Self {
        let mut flags = NodeFlags::new();
        flags.set_leaf(true);
        flags.set_mesh_dirty(true);

        Self {
            children_mask: 0,
            data_index,
            lod_data_index: u32::MAX,
            lod_level,
            flags,
            homogeneous_value: 0,
        }
    }

    pub fn new_internal(children_index: u32, lod_level: LodLevel) -> Self {
        Self {
            children_mask: 0xFF, // All 8 children exist by default
            data_index: children_index,
            lod_data_index: u32::MAX,
            lod_level,
            flags: NodeFlags::new(),
            homogeneous_value: 0,
        }
    }

    pub fn new_homogeneous(value: Voxel, lod_level: LodLevel) -> Self {
        let mut flags = NodeFlags::new();
        flags.set_leaf(true);
        flags.set_homogeneous(true);

        Self {
            children_mask: 0,
            data_index: 0,
            lod_data_index: u32::MAX,
            lod_level,
            flags,
            homogeneous_value: value,
        }
    }

    /// Check if child at given octant index exists
    pub fn has_child(&self, octant: u8) -> bool {
        self.children_mask & (1 << octant) != 0
    }

    /// Get child index in nodes array for given octant
    pub fn child_index(&self, octant: u8) -> Option<u32> {
        if !self.has_child(octant) {
            return None;
        }
        // Count how many children exist before this octant
        let before = (self.children_mask & ((1 << octant) - 1)).count_ones();
        Some(self.data_index + before)
    }
}

/// Main octree structure for storing voxel world
pub struct VoxelOctree {
    /// Root node index (always 0)
    root: u32,

    /// All nodes stored contiguously
    nodes: Vec<OctreeNode>,

    /// Voxel data storage (indexed by data_index for leaf nodes)
    voxel_data: Vec<VoxelData>,

    /// Free list for recycling data indices
    free_data_indices: Vec<u32>,

    /// World bounds in chunks
    pub world_min: IVec3,
    pub world_max: IVec3,

    /// Octree depth (log2 of world size in chunks per axis)
    depth: u8,
}

impl VoxelOctree {
    /// Create a new octree that can hold a world of given size (in chunks per axis)
    /// Size must be a power of 2. World is centered at origin (-half..half-1).
    pub fn new(size_chunks: u32) -> Self {
        assert!(size_chunks.is_power_of_two(), "Size must be power of 2");

        let depth = size_chunks.trailing_zeros() as u8;
        let half = (size_chunks / 2) as i32;

        // Create root node as homogeneous air leaf — ensure_path will expand it
        let root_node = OctreeNode::new_homogeneous(0, depth);

        Self {
            root: 0,
            nodes: vec![root_node],
            voxel_data: Vec::new(),
            free_data_indices: Vec::new(),
            world_min: IVec3::new(-half, -half, -half),
            world_max: IVec3::new(half - 1, half - 1, half - 1),
            depth,
        }
    }

    /// Create a new octree with a specific origin (world covers origin..origin+size-1)
    pub fn new_at_origin(size_chunks: u32, origin: IVec3) -> Self {
        assert!(size_chunks.is_power_of_two(), "Size must be power of 2");

        let depth = size_chunks.trailing_zeros() as u8;
        let size = size_chunks as i32;
        let root_node = OctreeNode::new_homogeneous(0, depth);

        Self {
            root: 0,
            nodes: vec![root_node],
            voxel_data: Vec::new(),
            free_data_indices: Vec::new(),
            world_min: origin,
            world_max: origin + IVec3::new(size - 1, size - 1, size - 1),
            depth,
        }
    }

    /// Create octree for a specific world size in meters, origin at (0,0,0)
    /// 1 meter = 16 voxels, 1 chunk = 32 voxels = 2 meters
    pub fn for_world_size_meters(meters: u32) -> Self {
        // Convert meters to chunks (32 voxels per chunk, 16 voxels per meter)
        // meters * 16 voxels/meter / 32 voxels/chunk = meters / 2 chunks
        let chunks = (meters / 2).next_power_of_two();
        Self::new_at_origin(chunks, IVec3::ZERO)
    }

    /// Get voxel at world coordinates
    pub fn get_voxel(&self, wx: i32, wy: i32, wz: i32) -> Voxel {
        let chunk_pos = ChunkPosition::from_world_coords(wx, wy, wz);
        let local_x = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_y = wy.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = wz.rem_euclid(CHUNK_SIZE as i32) as usize;

        if let Some(data) = self.get_chunk_data(chunk_pos) {
            data.get(local_x, local_y, local_z)
        } else {
            0 // Air
        }
    }

    /// Check if voxel is solid at world coordinates
    pub fn is_solid(&self, wx: i32, wy: i32, wz: i32) -> bool {
        self.get_voxel(wx, wy, wz) != 0
    }

    /// Get chunk data at given position
    pub fn get_chunk_data(&self, pos: ChunkPosition) -> Option<&VoxelData> {
        let node_idx = self.find_leaf_node(pos)?;
        let node = &self.nodes[node_idx as usize];

        if node.flags.is_homogeneous() {
            None // Homogeneous nodes don't have separate data
        } else {
            self.voxel_data.get(node.data_index as usize)
        }
    }

    /// Get chunk data at given position and LOD level
    pub fn get_lod_data(&self, pos: ChunkPosition, lod: LodLevel) -> Option<&VoxelData> {
        // For LOD > 0, we need to find the appropriate parent node
        let scaled_pos = ChunkPosition::new(pos.x >> lod, pos.y >> lod, pos.z >> lod);
        let node_idx = self.find_node_at_level(scaled_pos, lod)?;
        let node = &self.nodes[node_idx as usize];

        if node.flags.is_homogeneous() {
            None
        } else {
            self.voxel_data.get(node.data_index as usize)
        }
    }

    /// Insert or update chunk at given position
    pub fn insert_chunk(&mut self, pos: ChunkPosition, data: VoxelData) {
        // Ensure path to this chunk exists
        let node_idx = self.ensure_path(pos);

        let node = &mut self.nodes[node_idx as usize];

        // Check if chunk is homogeneous
        if let Some(value) = data.is_homogeneous() {
            node.flags.set_homogeneous(true);
            node.homogeneous_value = value;
            // Free old data if any
            if node.data_index != 0 {
                self.free_data_indices.push(node.data_index);
            }
            node.data_index = 0;
        } else {
            node.flags.set_homogeneous(false);
            // Allocate or reuse data index
            let data_idx = if let Some(idx) = self.free_data_indices.pop() {
                self.voxel_data[idx as usize] = data;
                idx
            } else {
                let idx = self.voxel_data.len() as u32;
                self.voxel_data.push(data);
                idx
            };
            node.data_index = data_idx;
        }

        node.flags.set_loaded(true);
        node.flags.set_mesh_dirty(true);

        // Mark parent LOD nodes as dirty
        self.mark_lod_dirty(pos);
    }

    /// Remove chunk at given position
    pub fn remove_chunk(&mut self, pos: ChunkPosition) {
        if let Some(node_idx) = self.find_leaf_node(pos) {
            let node = &mut self.nodes[node_idx as usize];

            if !node.flags.is_homogeneous() && node.data_index != 0 {
                self.free_data_indices.push(node.data_index);
            }

            // Convert to homogeneous air
            node.flags.set_homogeneous(true);
            node.homogeneous_value = 0;
            node.data_index = 0;
            node.flags.set_loaded(false);

            self.mark_lod_dirty(pos);
        }
    }

    /// Mark chunk and its LOD parents as dirty
    pub fn mark_dirty(&mut self, pos: ChunkPosition) {
        if let Some(node_idx) = self.find_leaf_node(pos) {
            self.nodes[node_idx as usize].flags.set_mesh_dirty(true);
        }
        self.mark_lod_dirty(pos);
    }

    /// Mark LOD parent nodes as dirty
    fn mark_lod_dirty(&mut self, pos: ChunkPosition) {
        let mut current_pos = pos;
        for lod in 1..=self.depth {
            current_pos =
                ChunkPosition::new(current_pos.x >> 1, current_pos.y >> 1, current_pos.z >> 1);
            if let Some(node_idx) = self.find_node_at_level(current_pos, lod) {
                self.nodes[node_idx as usize].flags.set_lod_dirty(true);
            }
        }
    }

    /// Find leaf node for given chunk position
    fn find_leaf_node(&self, pos: ChunkPosition) -> Option<u32> {
        self.find_node_at_level(pos, 0)
    }

    /// Find node at specific LOD level.
    /// `pos` is in LOD-level coordinates (chunk_pos >> target_lod).
    fn find_node_at_level(&self, pos: ChunkPosition, target_lod: LodLevel) -> Option<u32> {
        // Scale LOD-level position back to chunk-level for tree navigation
        let chunk_pos = ChunkPosition::new(
            pos.x << target_lod,
            pos.y << target_lod,
            pos.z << target_lod,
        );

        let mut node_idx = self.root;
        let mut current_lod = self.depth;

        // Navigate down to target level
        while current_lod > target_lod {
            let node = &self.nodes[node_idx as usize];

            if node.flags.is_leaf() {
                return None; // Path doesn't exist
            }

            // Calculate which octant contains this position at current level
            let shift = current_lod - 1;
            let octant = self.position_to_octant(chunk_pos, shift);

            node_idx = node.child_index(octant)?;
            current_lod -= 1;
        }

        Some(node_idx)
    }

    /// Ensure path exists to chunk position, creating nodes as needed
    fn ensure_path(&mut self, pos: ChunkPosition) -> u32 {
        let mut node_idx = self.root;
        let mut current_lod = self.depth;

        while current_lod > 0 {
            let shift = current_lod - 1;
            let octant = self.position_to_octant(pos, shift);

            // Check if we need to create children
            {
                let node = &self.nodes[node_idx as usize];
                if node.flags.is_leaf() {
                    // Convert leaf to internal node
                    let children_start = self.nodes.len() as u32;

                    // Create 8 child nodes (all homogeneous with parent's value)
                    let parent_value = node.homogeneous_value;
                    for _ in 0..8 {
                        self.nodes
                            .push(OctreeNode::new_homogeneous(parent_value, current_lod - 1));
                    }

                    let node = &mut self.nodes[node_idx as usize];
                    node.flags.set_leaf(false);
                    node.flags.set_homogeneous(false);
                    node.children_mask = 0xFF;
                    node.data_index = children_start;
                }
            }

            let node = &self.nodes[node_idx as usize];
            node_idx = node.child_index(octant).unwrap();
            current_lod -= 1;
        }

        node_idx
    }

    /// Convert chunk position to octant index at given level
    fn position_to_octant(&self, pos: ChunkPosition, shift: u8) -> u8 {
        // Normalize position relative to world origin
        let nx = pos.x - self.world_min.x;
        let ny = pos.y - self.world_min.y;
        let nz = pos.z - self.world_min.z;

        // Get bit at current level for each axis
        let bx = ((nx >> shift) & 1) as u8;
        let by = ((ny >> shift) & 1) as u8;
        let bz = ((nz >> shift) & 1) as u8;

        // Combine into octant index (0-7)
        bx | (by << 1) | (bz << 2)
    }

    /// Get all dirty chunks at LOD 0
    pub fn take_dirty_chunks(&mut self) -> Vec<ChunkPosition> {
        let mut dirty = Vec::new();
        self.collect_dirty_recursive(self.root, IVec3::ZERO, self.depth, &mut dirty);

        // Clear dirty flags
        for pos in &dirty {
            if let Some(idx) = self.find_leaf_node(*pos) {
                self.nodes[idx as usize].flags.set_mesh_dirty(false);
            }
        }

        dirty
    }

    fn collect_dirty_recursive(
        &self,
        node_idx: u32,
        offset: IVec3,
        level: u8,
        dirty: &mut Vec<ChunkPosition>,
    ) {
        let node = &self.nodes[node_idx as usize];

        if node.flags.is_leaf() {
            if level == 0 && node.flags.is_mesh_dirty() && node.flags.is_loaded() {
                dirty.push(ChunkPosition::new(
                    offset.x + self.world_min.x,
                    offset.y + self.world_min.y,
                    offset.z + self.world_min.z,
                ));
            }
            return;
        }

        let child_size = 1i32 << (level - 1);

        for octant in 0..8u8 {
            if node.has_child(octant) {
                let child_idx = node.child_index(octant).unwrap();
                let child_offset = IVec3::new(
                    offset.x + (octant & 1) as i32 * child_size,
                    offset.y + ((octant >> 1) & 1) as i32 * child_size,
                    offset.z + ((octant >> 2) & 1) as i32 * child_size,
                );
                self.collect_dirty_recursive(child_idx, child_offset, level - 1, dirty);
            }
        }
    }

    /// Iterator over all loaded chunks
    pub fn loaded_chunks(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        LoadedChunkIterator::new(self)
    }

    /// Get depth of the octree
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get number of stored voxel data blocks
    pub fn data_count(&self) -> usize {
        self.voxel_data.len()
    }

    /// Process all dirty LOD nodes bottom-up. Returns keys of regenerated nodes.
    pub fn process_dirty_lods(&mut self) -> Vec<LodNodeKey> {
        let mut regenerated = Vec::new();
        self.process_dirty_lods_recursive(
            self.root,
            IVec3::ZERO,
            self.depth,
            &mut regenerated,
        );
        regenerated
    }

    /// Recursive bottom-up LOD processing.
    fn process_dirty_lods_recursive(
        &mut self,
        node_idx: u32,
        offset: IVec3,
        level: u8,
        regenerated: &mut Vec<LodNodeKey>,
    ) {
        // Read node info into locals to avoid holding borrows during recursion
        let node = &self.nodes[node_idx as usize];
        if node.flags.is_leaf() {
            return;
        }
        let is_dirty = node.flags.is_lod_dirty();
        let children_mask = node.children_mask;
        let data_index = node.data_index;

        // Recurse into children first (bottom-up)
        let child_size = 1i32 << (level - 1);
        for octant in 0..8u8 {
            if children_mask & (1 << octant) != 0 {
                let before = (children_mask & ((1 << octant) - 1)).count_ones();
                let child_idx = data_index + before;
                let child_offset = IVec3::new(
                    offset.x + (octant & 1) as i32 * child_size,
                    offset.y + ((octant >> 1) & 1) as i32 * child_size,
                    offset.z + ((octant >> 2) & 1) as i32 * child_size,
                );
                self.process_dirty_lods_recursive(child_idx, child_offset, level - 1, regenerated);
            }
        }

        if !is_dirty {
            return;
        }

        // Collect children's data for LOD building (re-read node since tree may have been modified)
        let node = &self.nodes[node_idx as usize];
        let mut children_data: [Option<VoxelData>; 8] = Default::default();

        for octant in 0..8u8 {
            if node.has_child(octant) {
                let child_idx = node.child_index(octant).unwrap();
                let child = &self.nodes[child_idx as usize];

                if child.flags.is_leaf() {
                    if child.flags.is_homogeneous() {
                        children_data[octant as usize] = Some(VoxelData::Homogeneous(child.homogeneous_value));
                    } else if child.flags.is_loaded() {
                        children_data[octant as usize] = self.voxel_data.get(child.data_index as usize).cloned();
                    }
                } else {
                    // Internal node — use its LOD data if available
                    if child.lod_data_index != u32::MAX {
                        children_data[octant as usize] = self.voxel_data.get(child.lod_data_index as usize).cloned();
                    } else if child.flags.is_homogeneous() {
                        children_data[octant as usize] = Some(VoxelData::Homogeneous(child.homogeneous_value));
                    }
                }
            }
        }

        // Build LOD data from children
        let children_refs: [Option<&VoxelData>; 8] = [
            children_data[0].as_ref(),
            children_data[1].as_ref(),
            children_data[2].as_ref(),
            children_data[3].as_ref(),
            children_data[4].as_ref(),
            children_data[5].as_ref(),
            children_data[6].as_ref(),
            children_data[7].as_ref(),
        ];

        let lod_data = build_lod_from_children(&children_refs);

        // Store the LOD data
        let node = &mut self.nodes[node_idx as usize];
        if let Some(value) = lod_data.is_homogeneous() {
            node.flags.set_homogeneous(true);
            node.homogeneous_value = value;
            // Free old LOD data if any
            if node.lod_data_index != u32::MAX {
                self.free_data_indices.push(node.lod_data_index);
                node.lod_data_index = u32::MAX;
            }
        } else {
            node.flags.set_homogeneous(false);
            if node.lod_data_index != u32::MAX {
                self.voxel_data[node.lod_data_index as usize] = lod_data;
            } else if let Some(idx) = self.free_data_indices.pop() {
                self.voxel_data[idx as usize] = lod_data;
                node.lod_data_index = idx;
            } else {
                let idx = self.voxel_data.len() as u32;
                self.voxel_data.push(lod_data);
                node.lod_data_index = idx;
            }
        }

        node.flags.set_lod_dirty(false);

        // Build the LodNodeKey for this node
        // Convert offset (in [0, size) space) back to world chunk position, then to LOD-level coords
        let key = LodNodeKey::new(
            (offset.x + self.world_min.x) >> level,
            (offset.y + self.world_min.y) >> level,
            (offset.z + self.world_min.z) >> level,
            level,
        );
        regenerated.push(key);
    }

    /// Get LOD data for a specific LodNodeKey.
    pub fn get_node_lod_data(&self, key: &LodNodeKey) -> Option<&VoxelData> {
        let pos = ChunkPosition::new(key.x, key.y, key.z);
        let node_idx = self.find_node_at_level(pos, key.lod_level)?;
        let node = &self.nodes[node_idx as usize];
        if node.lod_data_index != u32::MAX {
            self.voxel_data.get(node.lod_data_index as usize)
        } else {
            None
        }
    }

    /// Check if a node is homogeneous, return the value if so.
    pub fn get_node_homogeneous(&self, key: &LodNodeKey) -> Option<Voxel> {
        let pos = ChunkPosition::new(key.x, key.y, key.z);
        let node_idx = self.find_node_at_level(pos, key.lod_level)?;
        let node = &self.nodes[node_idx as usize];
        if node.flags.is_homogeneous() {
            Some(node.homogeneous_value)
        } else {
            None
        }
    }
}

/// Iterator over loaded chunks in the octree
struct LoadedChunkIterator<'a> {
    octree: &'a VoxelOctree,
    stack: Vec<(u32, IVec3, u8)>, // (node_idx, offset, level)
}

impl<'a> LoadedChunkIterator<'a> {
    fn new(octree: &'a VoxelOctree) -> Self {
        let mut iter = Self {
            octree,
            stack: Vec::new(),
        };
        iter.stack.push((octree.root, IVec3::ZERO, octree.depth));
        iter
    }
}

impl<'a> Iterator for LoadedChunkIterator<'a> {
    type Item = ChunkPosition;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((node_idx, offset, level)) = self.stack.pop() {
            let node = &self.octree.nodes[node_idx as usize];

            if node.flags.is_leaf() {
                if level == 0 && node.flags.is_loaded() {
                    return Some(ChunkPosition::new(
                        offset.x + self.octree.world_min.x,
                        offset.y + self.octree.world_min.y,
                        offset.z + self.octree.world_min.z,
                    ));
                }
                continue;
            }

            let child_size = 1i32 << (level - 1);

            // Push children in reverse order so we process them in order
            for octant in (0..8u8).rev() {
                if node.has_child(octant) {
                    let child_idx = node.child_index(octant).unwrap();
                    let child_offset = IVec3::new(
                        offset.x + (octant & 1) as i32 * child_size,
                        offset.y + ((octant >> 1) & 1) as i32 * child_size,
                        offset.z + ((octant >> 2) & 1) as i32 * child_size,
                    );
                    self.stack.push((child_idx, child_offset, level - 1));
                }
            }
        }

        None
    }
}

impl Default for VoxelOctree {
    fn default() -> Self {
        Self::new(16) // 16x16x16 chunks = 32x32x32 meters default
    }
}

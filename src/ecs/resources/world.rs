//! World and chunk management resources

use std::collections::{HashSet, VecDeque};

use crate::world::{ChunkManager, ChunkPosition, ChunkStreamer, VoxelOctree};
use crate::MeshKey;

/// World data resource
/// Contains all voxel world data and streaming state
///
/// Note: RegionManager is stored separately since it contains non-Sync mpsc channels
/// Note: Debug not derived because underlying types don't implement Debug
pub struct WorldResource {
    /// Main chunk storage
    pub chunk_manager: ChunkManager,
    /// Voxel octree for LOD
    pub octree: Option<VoxelOctree>,
    /// Chunk streaming for large worlds
    pub streamer: Option<ChunkStreamer>,
    /// Whether streaming mode is enabled
    pub use_streaming: bool,
    /// Whether initial chunks are ready (prevents fall-through)
    pub chunks_ready: bool,
    /// Dirty chunks that need mesh rebuild
    pub dirty_chunks: HashSet<ChunkPosition>,
}

impl WorldResource {
    pub fn new() -> Self {
        Self {
            chunk_manager: ChunkManager::new(),
            octree: None,
            streamer: None,
            use_streaming: false,
            chunks_ready: true,
            dirty_chunks: HashSet::new(),
        }
    }

    /// Mark a chunk as dirty for mesh rebuild
    pub fn mark_dirty(&mut self, pos: ChunkPosition) {
        self.dirty_chunks.insert(pos);
    }

    /// Take all dirty chunks (clearing the set)
    pub fn take_dirty(&mut self) -> HashSet<ChunkPosition> {
        std::mem::take(&mut self.dirty_chunks)
    }

    /// Check if a voxel position is solid (for collision)
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.chunk_manager.is_solid(x, y, z)
    }
}

impl Default for WorldResource {
    fn default() -> Self {
        Self::new()
    }
}

/// Background mesh generation resource
/// Handles communication with rayon worker threads
///
/// Note: The mpsc channels are stored in a thread-local variable in the mesh system
/// since mpsc::Receiver is not Sync and cannot be a Specs resource.
#[derive(Debug, Default)]
pub struct MeshGenerationResource {
    /// Chunks currently being meshed in background
    pub inflight: HashSet<ChunkPosition>,
    /// Pending mesh queue
    pub pending_queue: VecDeque<ChunkPosition>,
    /// Pending set (for deduplication)
    pub pending_set: HashSet<ChunkPosition>,
}

impl MeshGenerationResource {
    /// Maximum number of chunks to rebuild per frame
    pub const CHUNKS_PER_FRAME: usize = 8;
    /// Maximum concurrent background mesh tasks
    pub const MAX_INFLIGHT: usize = 128;

    /// Add a chunk to the pending mesh queue
    pub fn request_mesh(&mut self, pos: ChunkPosition) {
        if !self.inflight.contains(&pos) && !self.pending_set.contains(&pos) {
            self.pending_queue.push_back(pos);
            self.pending_set.insert(pos);
        }
    }

    /// Mark a chunk as in-flight
    pub fn start_inflight(&mut self, pos: ChunkPosition) {
        self.inflight.insert(pos);
        self.pending_set.remove(&pos);
    }

    /// Mark a chunk as completed
    pub fn complete_inflight(&mut self, pos: ChunkPosition) {
        self.inflight.remove(&pos);
    }

    /// Check if we can dispatch more background tasks
    pub fn can_dispatch(&self) -> bool {
        self.inflight.len() < Self::MAX_INFLIGHT
    }
}

/// LOD mesh tracking resource
#[derive(Debug, Default)]
pub struct LodMeshResource {
    /// LOD mesh tasks to process
    pub tasks: Vec<MeshKey>,
}

/// Save/Load state resource
#[derive(Debug, Default)]
pub struct SaveLoadResource {
    /// Current save/load operation
    pub operation: SaveLoadState,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SaveLoadState {
    #[default]
    Idle,
    Saving,
    Loading,
    Generating,
}

/// World generation progress resource
#[derive(Debug, Clone, Default)]
pub struct WorldGenResource {
    /// Progress message
    pub message: String,
    /// Items completed
    pub completed: usize,
    /// Total items
    pub total: usize,
    /// Whether generation is complete
    pub is_complete: bool,
}

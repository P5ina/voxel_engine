//! Mesh rebuild system - processes dirty chunks and generates mesh data
//!
//! Note: Actual GPU mesh creation happens in the render thread since we need
//! access to wgpu::Device which is stored outside of ECS (in AppRenderResources).
//! This system generates vertex data and queues it for GPU upload.

use specs::{System, WriteExpect};

use crate::ecs::resources::{MeshGenerationResource, WorldGenResource, WorldResource};
use crate::voxel::{self, generate_chunk_mesh, generate_octree_lod_mesh};
use crate::world::lod::VoxelData;
use crate::MeshKey;

/// Mesh data ready for GPU upload
#[derive(Debug)]
pub struct MeshBuildResult {
    pub key: MeshKey,
    pub vertices: Vec<crate::renderer::Vertex>,
}

/// Resource holding pending mesh builds that need GPU upload
#[derive(Debug, Default)]
pub struct PendingMeshBuilds {
    pub builds: Vec<MeshBuildResult>,
}

pub struct MeshRebuildSystem;

impl MeshRebuildSystem {
    /// Generate LOD mesh data for a given key, using octree data or procedural fallback.
    fn generate_lod_mesh(world: &WorldResource, key: &crate::world::LodNodeKey) -> Vec<crate::renderer::Vertex> {
        if let Some(ref octree) = world.octree {
            if let Some(data) = octree.get_node_lod_data(key) {
                return generate_octree_lod_mesh(data, key);
            }
            if let Some(v) = octree.get_node_homogeneous(key) {
                if v != voxel::AIR {
                    let data = VoxelData::Homogeneous(v);
                    return generate_octree_lod_mesh(&data, key);
                }
            }
        }
        // Procedural fallback
        if let Some(data) = crate::terrain_gen::generate_lod_data_static(key) {
            return generate_octree_lod_mesh(&data, key);
        }
        Vec::new()
    }
}

impl<'a> System<'a> for MeshRebuildSystem {
    type SystemData = (
        WriteExpect<'a, WorldResource>,
        WriteExpect<'a, MeshGenerationResource>,
        WriteExpect<'a, PendingMeshBuilds>,
        WriteExpect<'a, WorldGenResource>,
    );

    fn run(&mut self, (mut world, _mesh_gen, mut pending, mut gen_res): Self::SystemData) {
        // 1. Process loading mesh tasks (high throughput during loading)
        if !gen_res.mesh_tasks.is_empty() {
            let batch_size = gen_res.mesh_tasks.len().min(256);
            let batch: Vec<MeshKey> = gen_res.mesh_tasks.drain(..batch_size).collect();

            for task in batch {
                let vertices = match &task {
                    MeshKey::Chunk(pos) => generate_chunk_mesh(&world.chunk_manager, *pos),
                    MeshKey::LodNode(key) => Self::generate_lod_mesh(&world, key),
                };
                pending.builds.push(MeshBuildResult {
                    key: task,
                    vertices,
                });
            }
            gen_res.completed = gen_res.mesh_tasks_total - gen_res.mesh_tasks.len();
            return; // During loading, focus on loading tasks only
        }

        // 2. Normal dirty chunk processing (up to limit per frame)
        let dirty_chunks: Vec<_> = world.take_dirty().into_iter().collect();
        let to_rebuild = dirty_chunks
            .len()
            .min(MeshGenerationResource::CHUNKS_PER_FRAME);

        for pos in dirty_chunks.iter().take(to_rebuild) {
            // Generate mesh data (vertices only, no GPU resources yet)
            let vertices = generate_chunk_mesh(&world.chunk_manager, *pos);

            // Queue for GPU upload (empty vertices signal mesh removal)
            pending.builds.push(MeshBuildResult {
                key: MeshKey::Chunk(*pos),
                vertices,
            });
        }

        // Re-queue any remaining dirty chunks
        for pos in dirty_chunks.iter().skip(to_rebuild) {
            world.mark_dirty(*pos);
        }
    }
}

/// Create the mesh rebuild system
pub fn mesh_rebuild_system() -> MeshRebuildSystem {
    MeshRebuildSystem
}

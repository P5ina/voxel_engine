//! Mesh rebuild system - processes dirty chunks and generates mesh data
//!
//! Note: Actual GPU mesh creation happens in the render thread since we need
//! access to wgpu::Device which is stored outside of ECS (in AppRenderResources).
//! This system generates vertex data and queues it for GPU upload.

use specs::{System, WriteExpect};

use crate::ecs::resources::{MeshGenerationResource, WorldResource};
use crate::voxel::generate_chunk_mesh;
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

impl<'a> System<'a> for MeshRebuildSystem {
    type SystemData = (
        WriteExpect<'a, WorldResource>,
        WriteExpect<'a, MeshGenerationResource>,
        WriteExpect<'a, PendingMeshBuilds>,
    );

    fn run(&mut self, (mut world, _mesh_gen, mut pending): Self::SystemData) {
        // Rebuild dirty meshes (up to limit per frame)
        let dirty_chunks: Vec<_> = world.take_dirty().into_iter().collect();
        let to_rebuild = dirty_chunks
            .len()
            .min(MeshGenerationResource::CHUNKS_PER_FRAME);

        for pos in dirty_chunks.iter().take(to_rebuild) {
            // Generate mesh data (vertices only, no GPU resources yet)
            let vertices = generate_chunk_mesh(&world.chunk_manager, *pos);

            // Queue for GPU upload
            if !vertices.is_empty() {
                pending.builds.push(MeshBuildResult {
                    key: MeshKey::Chunk(*pos),
                    vertices,
                });
            } else {
                // Queue removal of empty mesh
                pending.builds.push(MeshBuildResult {
                    key: MeshKey::Chunk(*pos),
                    vertices: Vec::new(),
                });
            }
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

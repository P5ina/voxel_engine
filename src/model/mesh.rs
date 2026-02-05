//! Mesh structures for polygonal models

use bytemuck::{Pod, Zeroable};

/// Texture format for loaded textures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8,
    Rgb8,
}

/// A loaded texture from a GLB file
#[derive(Debug, Clone)]
pub struct LoadedTexture {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

/// Vertex with skinning data for animated meshes
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joint_indices: [u32; 4],
    pub joint_weights: [f32; 4],
}

impl MeshVertex {
    pub const fn new(
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        joint_indices: [u32; 4],
        joint_weights: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            uv,
            joint_indices,
            joint_weights,
        }
    }

    /// Create a static vertex (no skinning)
    pub const fn static_vertex(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            joint_indices: [0, 0, 0, 0],
            joint_weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// A polygon mesh with vertices and optional material reference
#[derive(Debug, Clone)]
pub struct PolygonMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Option<Vec<u32>>,
    pub material_index: Option<usize>,
    pub texture_index: Option<usize>,
    pub name: String,
}

impl PolygonMesh {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            vertices: Vec::new(),
            indices: None,
            material_index: None,
            texture_index: None,
            name: name.into(),
        }
    }

    /// Get the number of triangles in this mesh
    pub fn triangle_count(&self) -> usize {
        if let Some(indices) = &self.indices {
            indices.len() / 3
        } else {
            self.vertices.len() / 3
        }
    }

    /// Iterate over triangles, yielding (v0, v1, v2) for each
    pub fn triangles(&self) -> impl Iterator<Item = (&MeshVertex, &MeshVertex, &MeshVertex)> {
        TriangleIterator {
            mesh: self,
            index: 0,
        }
    }
}

struct TriangleIterator<'a> {
    mesh: &'a PolygonMesh,
    index: usize,
}

impl<'a> Iterator for TriangleIterator<'a> {
    type Item = (&'a MeshVertex, &'a MeshVertex, &'a MeshVertex);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(indices) = &self.mesh.indices {
            if self.index + 2 < indices.len() {
                let i0 = indices[self.index] as usize;
                let i1 = indices[self.index + 1] as usize;
                let i2 = indices[self.index + 2] as usize;
                self.index += 3;
                Some((
                    &self.mesh.vertices[i0],
                    &self.mesh.vertices[i1],
                    &self.mesh.vertices[i2],
                ))
            } else {
                None
            }
        } else {
            if self.index + 2 < self.mesh.vertices.len() {
                let v0 = &self.mesh.vertices[self.index];
                let v1 = &self.mesh.vertices[self.index + 1];
                let v2 = &self.mesh.vertices[self.index + 2];
                self.index += 3;
                Some((v0, v1, v2))
            } else {
                None
            }
        }
    }
}

use crate::model::animation::AnimationClip;
use crate::model::skeleton::Skeleton;

/// A fully loaded model with meshes, skeleton, animations, and textures
#[derive(Debug)]
pub struct LoadedModel {
    pub meshes: Vec<PolygonMesh>,
    pub skeleton: Option<Skeleton>,
    pub animations: Vec<AnimationClip>,
    pub textures: Vec<LoadedTexture>,
    pub name: String,
}

impl LoadedModel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            meshes: Vec::new(),
            skeleton: None,
            animations: Vec::new(),
            textures: Vec::new(),
            name: name.into(),
        }
    }

    /// Get total triangle count across all meshes
    pub fn total_triangles(&self) -> usize {
        self.meshes.iter().map(|m| m.triangle_count()).sum()
    }

    /// Check if model has skeletal animation data
    pub fn is_skinned(&self) -> bool {
        self.skeleton.is_some()
    }

    /// Check if model has any animations
    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }
}

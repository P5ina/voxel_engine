//! GPU-friendly BVH structures for shader access

use bytemuck::{Pod, Zeroable};

/// GPU BVH node (32 bytes, cache-aligned)
///
/// Layout:
/// - bounds_min: [f32; 3] (12 bytes)
/// - left_or_first: u32 (4 bytes) - left child index OR first triangle index
/// - bounds_max: [f32; 3] (12 bytes)
/// - right_or_count: u32 (4 bytes) - right child index OR triangle count (with leaf flag)
///
/// If (right_or_count & 0x80000000) != 0, this is a leaf node:
///   - left_or_first = first triangle index
///   - right_or_count & 0x7FFFFFFF = triangle count
///     Otherwise, this is an interior node:
///   - left_or_first = left child index
///   - right_or_count = right child index
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuBvhNode {
    pub bounds_min: [f32; 3],
    pub left_or_first: u32,
    pub bounds_max: [f32; 3],
    pub right_or_count: u32,
}

impl GpuBvhNode {
    /// Leaf flag (highest bit of right_or_count)
    pub const LEAF_FLAG: u32 = 0x80000000;

    /// Create an interior node
    pub fn interior(
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        left_child: u32,
        right_child: u32,
    ) -> Self {
        Self {
            bounds_min,
            left_or_first: left_child,
            bounds_max,
            right_or_count: right_child,
        }
    }

    /// Create a leaf node
    pub fn leaf(
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        first_triangle: u32,
        triangle_count: u32,
    ) -> Self {
        Self {
            bounds_min,
            left_or_first: first_triangle,
            bounds_max,
            right_or_count: triangle_count | Self::LEAF_FLAG,
        }
    }
}

/// GPU triangle (48 bytes for efficiency, padded to 64 for alignment)
///
/// Uses edge representation for Moller-Trumbore intersection:
/// - v0: base vertex
/// - edge1: v1 - v0
/// - edge2: v2 - v0
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuTriangle {
    pub v0: [f32; 3],
    pub _pad0: f32,
    pub edge1: [f32; 3],
    pub _pad1: f32,
    pub edge2: [f32; 3],
    pub _pad2: f32,
    pub normal: [f32; 3],
    pub material_id: u32,
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    pub texture_id: u32,
    pub _pad3: u32,
}

impl GpuTriangle {
    /// Create a GPU triangle from vertices
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        v0: [f32; 3],
        v1: [f32; 3],
        v2: [f32; 3],
        normal: [f32; 3],
        uv0: [f32; 2],
        uv1: [f32; 2],
        uv2: [f32; 2],
        material_id: u32,
        texture_id: u32,
    ) -> Self {
        let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

        Self {
            v0,
            _pad0: 0.0,
            edge1,
            _pad1: 0.0,
            edge2,
            _pad2: 0.0,
            normal,
            material_id,
            uv0,
            uv1,
            uv2,
            texture_id,
            _pad3: 0,
        }
    }
}

/// Character parameters for the GPU
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct CharacterParams {
    /// Number of BVH nodes
    pub node_count: u32,
    /// Number of triangles
    pub triangle_count: u32,
    /// Whether characters are enabled
    pub enabled: u32,
    pub _padding: u32,
}

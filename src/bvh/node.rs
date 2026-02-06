//! CPU-side BVH node structures

use glam::Vec3;

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Create an empty (inverted) AABB
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    /// Create an AABB containing a triangle
    pub fn from_triangle(v0: Vec3, v1: Vec3, v2: Vec3) -> Self {
        Self {
            min: v0.min(v1).min(v2),
            max: v0.max(v1).max(v2),
        }
    }

    /// Expand AABB to include another AABB
    pub fn expand_aabb(&mut self, other: &Aabb) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    /// Get the center of the AABB
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Get the size (extents) of the AABB
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Get the surface area of the AABB (for SAH)
    pub fn surface_area(&self) -> f32 {
        let size = self.size();
        2.0 * (size.x * size.y + size.y * size.z + size.z * size.x)
    }

    /// Get the longest axis (0=x, 1=y, 2=z)
    pub fn longest_axis(&self) -> usize {
        let size = self.size();
        if size.x > size.y && size.x > size.z {
            0
        } else if size.y > size.z {
            1
        } else {
            2
        }
    }
}

impl Default for Aabb {
    fn default() -> Self {
        Self::empty()
    }
}

/// A triangle with precomputed data for intersection testing
#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub normal: Vec3,
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    pub material_id: u32,
    pub texture_id: u32,
}

impl Triangle {
    pub fn new(v0: Vec3, v1: Vec3, v2: Vec3) -> Self {
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let normal = edge1.cross(edge2).normalize_or_zero();

        Self {
            v0,
            v1,
            v2,
            normal,
            uv0: [0.0, 0.0],
            uv1: [1.0, 0.0],
            uv2: [0.0, 1.0],
            material_id: 0,
            texture_id: 0,
        }
    }

    /// Get the AABB of this triangle
    pub fn bounds(&self) -> Aabb {
        Aabb::from_triangle(self.v0, self.v1, self.v2)
    }

    /// Get the centroid of this triangle
    pub fn centroid(&self) -> Vec3 {
        (self.v0 + self.v1 + self.v2) / 3.0
    }
}

/// BVH node (CPU representation)
#[derive(Debug, Clone)]
pub enum BvhNode {
    /// Interior node with two children
    Interior {
        bounds: Aabb,
        left: Box<BvhNode>,
        right: Box<BvhNode>,
    },
    /// Leaf node containing triangle indices
    Leaf {
        bounds: Aabb,
        first_triangle: u32,
        triangle_count: u32,
    },
}

//! BVH construction algorithm

use glam::Vec3;

use crate::bvh::gpu_data::{GpuBvhNode, GpuTriangle};
use crate::bvh::node::{Aabb, BvhNode, Triangle};

/// Maximum triangles per leaf node
const MAX_LEAF_SIZE: usize = 4;

/// BVH builder using Surface Area Heuristic (SAH)
pub struct BvhBuilder {
    triangles: Vec<Triangle>,
    centroids: Vec<Vec3>,
}

impl BvhBuilder {
    pub fn new() -> Self {
        Self {
            triangles: Vec::new(),
            centroids: Vec::new(),
        }
    }

    /// Add a triangle to the BVH
    pub fn add_triangle(&mut self, triangle: Triangle) {
        self.centroids.push(triangle.centroid());
        self.triangles.push(triangle);
    }

    /// Add triangles from vertices (assumes triangle list)
    pub fn add_triangles_from_vertices(
        &mut self,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        uvs: &[[f32; 2]],
        material_id: u32,
        texture_id: u32,
    ) {
        for i in (0..vertices.len()).step_by(3) {
            if i + 2 < vertices.len() {
                let v0 = Vec3::from(vertices[i]);
                let v1 = Vec3::from(vertices[i + 1]);
                let v2 = Vec3::from(vertices[i + 2]);

                let mut tri = Triangle::new(v0, v1, v2);

                if i < normals.len() {
                    tri.normal = Vec3::from(normals[i]).normalize_or_zero();
                }

                if i < uvs.len() {
                    tri.uv0 = uvs[i];
                    tri.uv1 = uvs.get(i + 1).copied().unwrap_or([1.0, 0.0]);
                    tri.uv2 = uvs.get(i + 2).copied().unwrap_or([0.0, 1.0]);
                }

                tri.material_id = material_id;
                tri.texture_id = texture_id;

                self.add_triangle(tri);
            }
        }
    }

    /// Build the BVH tree
    pub fn build(&mut self) -> Option<BvhNode> {
        if self.triangles.is_empty() {
            return None;
        }

        // Create index array
        let mut indices: Vec<usize> = (0..self.triangles.len()).collect();

        Some(self.build_recursive(&mut indices, 0, self.triangles.len()))
    }

    fn build_recursive(&self, indices: &mut [usize], start: usize, end: usize) -> BvhNode {
        let count = end - start;

        // Calculate bounds for this subset
        let mut bounds = Aabb::empty();
        for &idx in &indices[start..end] {
            bounds.expand_aabb(&self.triangles[idx].bounds());
        }

        // Create leaf if few enough triangles
        if count <= MAX_LEAF_SIZE {
            return BvhNode::Leaf {
                bounds,
                first_triangle: start as u32,
                triangle_count: count as u32,
            };
        }

        // Find best split using SAH
        let (split_axis, split_pos) = self.find_best_split(indices, start, end, &bounds);

        // Partition triangles
        let mid = self.partition(indices, start, end, split_axis, split_pos);

        // Handle degenerate cases
        let mid = if mid == start || mid == end {
            start + count / 2
        } else {
            mid
        };

        // Recursively build children
        let left = Box::new(self.build_recursive(indices, start, mid));
        let right = Box::new(self.build_recursive(indices, mid, end));

        BvhNode::Interior {
            bounds,
            left,
            right,
        }
    }

    fn find_best_split(
        &self,
        indices: &[usize],
        start: usize,
        end: usize,
        bounds: &Aabb,
    ) -> (usize, f32) {
        let axis = bounds.longest_axis();
        let mut best_cost = f32::INFINITY;
        let mut best_split = bounds.center()[axis];

        // Try several split positions
        const NUM_BINS: usize = 8;
        let extent = bounds.size()[axis];

        if extent < 1e-6 {
            return (axis, best_split);
        }

        for i in 1..NUM_BINS {
            let t = i as f32 / NUM_BINS as f32;
            let split_pos = bounds.min[axis] + extent * t;

            // Count triangles on each side and compute bounds
            let mut left_bounds = Aabb::empty();
            let mut right_bounds = Aabb::empty();
            let mut left_count = 0;
            let mut right_count = 0;

            for &idx in &indices[start..end] {
                let centroid = self.centroids[idx][axis];
                let tri_bounds = self.triangles[idx].bounds();

                if centroid < split_pos {
                    left_bounds.expand_aabb(&tri_bounds);
                    left_count += 1;
                } else {
                    right_bounds.expand_aabb(&tri_bounds);
                    right_count += 1;
                }
            }

            // Skip if all triangles on one side
            if left_count == 0 || right_count == 0 {
                continue;
            }

            // SAH cost
            let cost = left_count as f32 * left_bounds.surface_area()
                + right_count as f32 * right_bounds.surface_area();

            if cost < best_cost {
                best_cost = cost;
                best_split = split_pos;
            }
        }

        (axis, best_split)
    }

    fn partition(
        &self,
        indices: &mut [usize],
        start: usize,
        end: usize,
        axis: usize,
        split_pos: f32,
    ) -> usize {
        let mut left = start;
        let mut right = end - 1;

        while left < right {
            while left < right && self.centroids[indices[left]][axis] < split_pos {
                left += 1;
            }
            while left < right && self.centroids[indices[right]][axis] >= split_pos {
                right -= 1;
            }
            if left < right {
                indices.swap(left, right);
            }
        }

        left
    }

    /// Convert to GPU-friendly format
    pub fn build_gpu(&mut self) -> (Vec<GpuBvhNode>, Vec<GpuTriangle>) {
        if self.triangles.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // Create index array - this gets reordered by build_recursive
        let mut indices: Vec<usize> = (0..self.triangles.len()).collect();

        let Some(root) = self.build_recursive_with_indices(&mut indices, 0, self.triangles.len()) else {
            return (Vec::new(), Vec::new());
        };

        // Reorder triangles according to BVH leaves
        let mut triangle_order: Vec<usize> = Vec::new();
        let mut gpu_nodes: Vec<GpuBvhNode> = Vec::new();

        // Pass indices to flatten so it can look up real triangle indices
        self.flatten_to_gpu_with_indices(&root, &mut gpu_nodes, &mut triangle_order, &indices);

        // Create GPU triangles in the new order
        let gpu_triangles: Vec<GpuTriangle> = triangle_order
            .iter()
            .map(|&idx| {
                let tri = &self.triangles[idx];
                GpuTriangle::new(
                    tri.v0.to_array(),
                    tri.v1.to_array(),
                    tri.v2.to_array(),
                    tri.normal.to_array(),
                    tri.uv0,
                    tri.uv1,
                    tri.uv2,
                    tri.material_id,
                    tri.texture_id,
                )
            })
            .collect();

        (gpu_nodes, gpu_triangles)
    }

    fn build_recursive_with_indices(&self, indices: &mut [usize], start: usize, end: usize) -> Option<BvhNode> {
        let count = end - start;
        if count == 0 {
            return None;
        }

        // Calculate bounds for this subset
        let mut bounds = Aabb::empty();
        for &idx in &indices[start..end] {
            bounds.expand_aabb(&self.triangles[idx].bounds());
        }

        // Create leaf if few enough triangles
        if count <= MAX_LEAF_SIZE {
            return Some(BvhNode::Leaf {
                bounds,
                first_triangle: start as u32,  // Index into indices array
                triangle_count: count as u32,
            });
        }

        // Find best split using SAH
        let (split_axis, split_pos) = self.find_best_split(indices, start, end, &bounds);

        // Partition triangles
        let mid = self.partition(indices, start, end, split_axis, split_pos);

        // Handle degenerate cases
        let mid = if mid == start || mid == end {
            start + count / 2
        } else {
            mid
        };

        // Recursively build children
        let left = Box::new(self.build_recursive_with_indices(indices, start, mid)?);
        let right = Box::new(self.build_recursive_with_indices(indices, mid, end)?);

        Some(BvhNode::Interior {
            bounds,
            left,
            right,
        })
    }

    fn flatten_to_gpu(
        &self,
        node: &BvhNode,
        gpu_nodes: &mut Vec<GpuBvhNode>,
        triangle_order: &mut Vec<usize>,
    ) -> u32 {
        let node_index = gpu_nodes.len() as u32;

        match node {
            BvhNode::Leaf {
                bounds,
                first_triangle,
                triangle_count,
            } => {
                let first_gpu_tri = triangle_order.len() as u32;

                // Add triangles to the order
                for i in 0..*triangle_count {
                    triangle_order.push((*first_triangle + i) as usize);
                }

                gpu_nodes.push(GpuBvhNode::leaf(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    first_gpu_tri,
                    *triangle_count,
                ));
            }
            BvhNode::Interior {
                bounds,
                left,
                right,
            } => {
                // Reserve space for this node
                gpu_nodes.push(GpuBvhNode::interior(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    0,
                    0,
                ));

                // Build children
                let left_index = self.flatten_to_gpu(left, gpu_nodes, triangle_order);
                let right_index = self.flatten_to_gpu(right, gpu_nodes, triangle_order);

                // Update this node with child indices
                gpu_nodes[node_index as usize] = GpuBvhNode::interior(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    left_index,
                    right_index,
                );
            }
        }

        node_index
    }

    fn flatten_to_gpu_with_indices(
        &self,
        node: &BvhNode,
        gpu_nodes: &mut Vec<GpuBvhNode>,
        triangle_order: &mut Vec<usize>,
        indices: &[usize],  // The reordered indices array from build
    ) -> u32 {
        let node_index = gpu_nodes.len() as u32;

        match node {
            BvhNode::Leaf {
                bounds,
                first_triangle,
                triangle_count,
            } => {
                let first_gpu_tri = triangle_order.len() as u32;

                // Add REAL triangle indices from the reordered indices array
                let start = *first_triangle as usize;
                for i in 0..*triangle_count as usize {
                    // indices[start + i] gives us the real triangle index
                    triangle_order.push(indices[start + i]);
                }

                gpu_nodes.push(GpuBvhNode::leaf(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    first_gpu_tri,
                    *triangle_count,
                ));
            }
            BvhNode::Interior {
                bounds,
                left,
                right,
            } => {
                // Reserve space for this node
                gpu_nodes.push(GpuBvhNode::interior(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    0,
                    0,
                ));

                // Build children
                let left_index = self.flatten_to_gpu_with_indices(left, gpu_nodes, triangle_order, indices);
                let right_index = self.flatten_to_gpu_with_indices(right, gpu_nodes, triangle_order, indices);

                // Update this node with child indices
                gpu_nodes[node_index as usize] = GpuBvhNode::interior(
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    left_index,
                    right_index,
                );
            }
        }

        node_index
    }

    /// Get the number of triangles
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Clear all triangles
    pub fn clear(&mut self) {
        self.triangles.clear();
        self.centroids.clear();
    }
}

impl Default for BvhBuilder {
    fn default() -> Self {
        Self::new()
    }
}

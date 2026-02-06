//! Bounding Volume Hierarchy (BVH) for ray-triangle intersection
//!
//! Provides GPU-friendly BVH structures for path tracing polygonal models.

pub mod builder;
pub mod gpu_data;
pub mod node;

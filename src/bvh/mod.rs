//! Bounding Volume Hierarchy (BVH) for ray-triangle intersection
//!
//! Provides GPU-friendly BVH structures for path tracing polygonal models.

pub mod builder;
pub mod gpu_data;
pub mod node;

pub use builder::BvhBuilder;
pub use gpu_data::{GpuBvhNode, GpuTriangle};
pub use node::{Aabb, BvhNode};

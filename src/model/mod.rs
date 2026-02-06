//! Model loading module for GLTF/GLB files
//!
//! Handles loading of polygonal models with support for:
//! - Meshes with vertices, normals, UVs
//! - Skeletal animation (skinning)
//! - Textures embedded in GLB files

pub mod animation;
pub mod gltf_loader;
pub mod mesh;
pub mod skeleton;

pub use animation::AnimationPlayer;
pub use gltf_loader::load_glb;
pub use mesh::{LoadedModel, PolygonMesh};
pub use skeleton::SkeletonState;

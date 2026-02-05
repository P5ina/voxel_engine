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

pub use animation::{AnimationClip, AnimationPlayer, Keyframe};
pub use gltf_loader::{load_glb, load_gltf, LoadError};
pub use mesh::{LoadedModel, LoadedTexture, MeshVertex, PolygonMesh, TextureFormat};
pub use skeleton::{Joint, Skeleton, SkeletonState};

//! Character system for polygonal models with path tracing
//!
//! Manages local player, remote players, and GPU resources for BVH-based ray tracing.

pub mod first_person;
pub mod local_player;
pub mod manager;
pub mod remote_player;

pub use manager::CharacterManager;

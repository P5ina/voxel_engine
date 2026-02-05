//! Character system for polygonal models with path tracing
//!
//! Manages local player, remote players, and GPU resources for BVH-based ray tracing.

pub mod first_person;
pub mod local_player;
pub mod manager;
pub mod remote_player;

pub use first_person::FirstPersonView;
pub use local_player::LocalPlayer;
pub use manager::CharacterManager;
pub use remote_player::RemotePlayer;

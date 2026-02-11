//! ECS Resources
//!
//! Resources are global data accessible by all systems.
//! Unlike components, they are not attached to entities.

pub mod game;
pub mod render;
pub mod world;

pub use game::*;
pub use render::*;
pub use world::*;

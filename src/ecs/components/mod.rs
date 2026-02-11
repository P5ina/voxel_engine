//! ECS Components
//!
//! Components are data-only structs that can be attached to entities.
//! Use Specs' Component derive macro for storage management.

pub mod character;
pub mod input;
pub mod network;
pub mod physics;
pub mod player;
pub mod spatial;
pub mod ui;

pub use character::*;
pub use input::*;
pub use network::*;
pub use physics::*;
pub use player::*;
pub use spatial::*;
pub use ui::*;

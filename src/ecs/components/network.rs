//! Networking components for multiplayer

use std::time::Instant;

use glam::{Quat, Vec3};
use specs::{Component, NullStorage, VecStorage};

/// Marker component for entities that sync over network
#[derive(Component, Debug, Default, Clone, Copy)]
#[storage(NullStorage)]
pub struct Networked;

/// Component indicating ownership of an entity
/// Used to determine which client can control/modify an entity
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct Owner {
    /// Client/player ID that owns this entity
    pub player_id: u64,
    /// Whether this is the local owner
    pub is_local: bool,
}

impl Owner {
    pub fn new(player_id: u64, is_local: bool) -> Self {
        Self {
            player_id,
            is_local,
        }
    }

    pub fn local(player_id: u64) -> Self {
        Self::new(player_id, true)
    }

    pub fn remote(player_id: u64) -> Self {
        Self::new(player_id, false)
    }
}

/// Network transform with interpolation data
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct NetworkTransform {
    /// Last known server position
    pub server_position: Vec3,
    /// Last known server rotation
    pub server_rotation: Quat,
    /// Last known server velocity
    pub server_velocity: Vec3,
    /// When this data was received
    pub last_received: Instant,
    /// Interpolation delay (how far behind present to render)
    pub interpolation_delay: f32,
}

impl Default for NetworkTransform {
    fn default() -> Self {
        Self {
            server_position: Vec3::ZERO,
            server_rotation: Quat::IDENTITY,
            server_velocity: Vec3::ZERO,
            last_received: Instant::now(),
            interpolation_delay: 0.1, // 100ms interpolation delay
        }
    }
}

/// Entity ID for network synchronization
/// Maps local entity to network entity ID
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[storage(VecStorage)]
pub struct NetEntityId {
    pub id: u64,
}

impl NetEntityId {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

/// Last sent state tracking for network delta compression
#[derive(Component, Debug, Clone, Copy)]
#[storage(VecStorage)]
pub struct LastSentState {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub time: Instant,
    pub sequence: u32,
}

impl Default for LastSentState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            time: Instant::now(),
            sequence: 0,
        }
    }
}

/// Network priority for entity updates
/// Higher priority entities get sent more frequently
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[storage(VecStorage)]
pub enum NetworkPriority {
    Critical, // Every frame (local player)
    High,     // 30Hz (nearby players, projectiles)
    Medium,   // 10Hz (distant players)
    Low,      // 5Hz (environmental objects)
    Static,   // Never (static world geometry)
}

impl Default for NetworkPriority {
    fn default() -> Self {
        NetworkPriority::Medium
    }
}

/// Network statistics for debugging
#[derive(Component, Debug, Clone, Copy, Default)]
#[storage(VecStorage)]
pub struct NetworkStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub last_rtt: f32,
    pub packet_loss: f32,
}

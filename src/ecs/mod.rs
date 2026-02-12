//! Entity Component System (ECS) module using Specs
//!
//! This module contains all ECS components, resources, and systems.
//! The migration from monolithic AppState to ECS enables:
//! - Parallel system execution for better performance
//! - Cleaner separation of concerns
//! - Foundation for multiplayer networking
//!
//! Most items are unused until later migration phases.
#![allow(dead_code, unused_imports)]

pub mod components;
pub mod entities;
pub mod resources;
pub mod systems;

pub use components::*;
pub use entities::*;
pub use resources::*;
pub use systems::*;

use specs::{Dispatcher, DispatcherBuilder, World, WorldExt};

/// Build the active dispatcher for the current migration phase.
/// Phase 2: timing, lighting, input, camera, free cam movement.
pub fn build_active_dispatcher() -> Dispatcher<'static, 'static> {
    DispatcherBuilder::new()
        .with(systems::time_system(), "time", &[])
        .with(systems::lighting_system(), "lighting", &[])
        .with(systems::input_system(), "input", &["time"])
        .with(systems::free_camera_system(), "free_cam", &["input"])
        .with(
            systems::camera_update_system(),
            "camera",
            &["input", "free_cam"],
        )
        .with(systems::mesh_rebuild_system(), "mesh_rebuild", &[])
        .build()
}

/// Build the ECS dispatcher with all systems in the correct order
pub fn build_dispatcher() -> Dispatcher<'static, 'static> {
    DispatcherBuilder::new()
        // Phase 1: Input & Time
        .with(systems::time_system(), "time", &[])
        .with(systems::input_system(), "input", &["time"])
        // Phase 2: Physics & Movement (parallel)
        .with(systems::player_physics_system(), "physics", &["input"])
        .with(systems::free_camera_system(), "free_cam", &["input"])
        // Phase 3: Camera & Transforms
        .with(
            systems::camera_update_system(),
            "camera",
            &["physics", "free_cam"],
        )
        .with(
            systems::character_animation_system(),
            "animation",
            &["camera"],
        )
        // Phase 4: World Updates
        .with(systems::voxel_edit_system(), "voxel_edit", &["input"])
        .with(systems::streaming_system(), "streaming", &["camera"])
        .with_barrier()
        // Phase 5: Mesh Generation
        .with(
            systems::mesh_rebuild_system(),
            "mesh_rebuild",
            &["streaming", "voxel_edit"],
        )
        // Phase 6: GPU Updates
        .with(systems::character_bvh_system(), "bvh", &["animation"])
        .with(systems::lighting_system(), "lighting", &["time"])
        // Phase 7: UI
        .with(systems::ui_system(), "ui", &[])
        .build()
}

/// Setup function to register all components and insert initial resources
pub fn setup_world(world: &mut World) {
    // Register spatial components
    world.register::<components::Position>();
    world.register::<components::Rotation>();
    world.register::<components::Velocity>();
    world.register::<components::Scale>();

    // Register player components
    world.register::<components::Player>();
    world.register::<components::LocalPlayer>();
    world.register::<components::RemotePlayer>();
    world.register::<components::Camera>();
    world.register::<components::FreeCam>();

    // Register input components
    world.register::<components::InputState>();
    world.register::<components::MouseInput>();
    world.register::<components::MovementSpeed>();
    world.register::<components::InputControlled>();

    // Register physics components
    world.register::<components::Physics>();
    world.register::<components::Grounded>();
    world.register::<components::StepHeight>();
    world.register::<components::TerminalVelocity>();
    world.register::<components::Aabb>();
    world.register::<components::Static>();
    world.register::<components::Kinematic>();

    // Register character components
    world.register::<components::FirstPersonView>();
    world.register::<components::AnimationState>();
    world.register::<components::WalkingState>();
    world.register::<components::BvhDirty>();
    world.register::<components::LocalCharacter>();
    world.register::<components::RemoteCharacter>();

    // Register network components
    world.register::<components::Networked>();
    world.register::<components::Owner>();
    world.register::<components::NetworkTransform>();
    world.register::<components::NetEntityId>();
    world.register::<components::LastSentState>();
    world.register::<components::NetworkPriority>();
    world.register::<components::NetworkStats>();

    // Register UI components
    world.register::<components::UiScreen>();
    world.register::<components::FpsDisplay>();
    world.register::<components::DebugDisplay>();
    world.register::<components::LoadingState>();
    world.register::<components::EditorMode>();
    world.register::<components::MapSelectState>();
    world.register::<components::MouseGrabbed>();

    // Insert default resources
    world.insert(systems::mesh::PendingMeshBuilds::default());
}

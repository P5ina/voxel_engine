//! Entity builders for creating game entities

use specs::prelude::*;

use crate::ecs::components::*;
use crate::ecs::resources::EntityLookup;

/// Builder for creating the local player entity
pub struct LocalPlayerBuilder {
    position: glam::Vec3,
    height: f32,
    width: f32,
    speed_walk: f32,
    speed_sprint: f32,
    fov: f32,
    aspect: f32,
}

impl Default for LocalPlayerBuilder {
    fn default() -> Self {
        Self {
            position: glam::Vec3::new(64.0, 12.0, 64.0),
            height: 1.8,
            width: 0.3,
            speed_walk: 45.0,
            speed_sprint: 90.0,
            fov: 70.0_f32.to_radians(),
            aspect: 1.0,
        }
    }
}

impl LocalPlayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn position(mut self, pos: glam::Vec3) -> Self {
        self.position = pos;
        self
    }

    pub fn aspect_ratio(mut self, aspect: f32) -> Self {
        self.aspect = aspect;
        self
    }

    /// Build and insert the local player entity into the world
    pub fn build(self, world: &mut World) -> Entity {
        let entity = world
            .create_entity()
            .with(Position(self.position))
            .with(Rotation::default())
            .with(Velocity::default())
            .with(Scale::uniform(1.0))
            .with(Player::new(self.height, self.width))
            .with(LocalPlayer)
            .with(InputState::default())
            .with(MouseInput::default())
            .with(MovementSpeed {
                walk: self.speed_walk,
                sprint: self.speed_sprint,
                freecam: 60.0,
                freecam_fast: 200.0,
                current: self.speed_walk,
            })
            .with(Camera::new(self.aspect))
            .with(FirstPersonView::new())
            .with(WalkingState::default())
            .with(Physics::default())
            .with(Aabb::from_player_size(self.width, self.height))
            .with(StepHeight::default())
            .with(TerminalVelocity::default())
            .with(Grounded::default())
            .with(InputControlled)
            .with(NetworkPriority::Critical)
            .with(Owner::local(0)) // Player ID 0 for local
            .build();

        // Register in entity lookup
        let mut lookup = world.write_resource::<EntityLookup>();
        lookup.local_player = Some(entity);
        lookup.active_camera = Some(entity);

        log::info!("[ECS] Created local player entity: {:?}", entity);

        entity
    }
}

/// Builder for creating a remote player entity (for multiplayer)
pub struct RemotePlayerBuilder {
    player_id: u64,
    position: glam::Vec3,
    height: f32,
    width: f32,
}

impl RemotePlayerBuilder {
    pub fn new(player_id: u64) -> Self {
        Self {
            player_id,
            position: glam::Vec3::ZERO,
            height: 1.8,
            width: 0.3,
        }
    }

    pub fn position(mut self, pos: glam::Vec3) -> Self {
        self.position = pos;
        self
    }

    /// Build and insert the remote player entity
    pub fn build(self, world: &mut World) -> Entity {
        let entity = world
            .create_entity()
            .with(Position(self.position))
            .with(Rotation::default())
            .with(Velocity::default())
            .with(Scale::uniform(1.0))
            .with(Player::new(self.height, self.width))
            .with(RemotePlayer::new(self.player_id))
            .with(Physics::default())
            .with(Aabb::from_player_size(self.width, self.height))
            .with(Networked)
            .with(NetworkPriority::High)
            .with(Owner::remote(self.player_id))
            .with(NetworkTransform::default())
            .with(RemoteCharacter::default())
            .build();

        // Register in entity lookup
        let mut lookup = world.write_resource::<EntityLookup>();
        lookup.remote_players.insert(self.player_id, entity);

        log::info!(
            "[ECS] Created remote player entity: {:?} (ID: {})",
            entity,
            self.player_id
        );

        entity
    }
}

/// Builder for creating a free camera entity (for editor/dev mode)
pub struct FreeCameraBuilder {
    position: glam::Vec3,
    yaw: f32,
    pitch: f32,
    fov: f32,
    aspect: f32,
}

impl Default for FreeCameraBuilder {
    fn default() -> Self {
        Self {
            position: glam::Vec3::new(64.0, 20.0, 64.0),
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
            fov: 70.0_f32.to_radians(),
            aspect: 1.0,
        }
    }
}

impl FreeCameraBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn position(mut self, pos: glam::Vec3) -> Self {
        self.position = pos;
        self
    }

    pub fn rotation(mut self, yaw: f32, pitch: f32) -> Self {
        self.yaw = yaw;
        self.pitch = pitch;
        self
    }

    pub fn aspect_ratio(mut self, aspect: f32) -> Self {
        self.aspect = aspect;
        self
    }

    /// Build and insert the free camera entity
    pub fn build(self, world: &mut World) -> Entity {
        let mut camera = Camera::new(self.aspect);
        camera.yaw = self.yaw;
        camera.pitch = self.pitch;
        camera.fov = self.fov;

        let entity = world
            .create_entity()
            .with(Position(self.position))
            .with(Rotation::from_yaw_pitch(self.yaw, self.pitch))
            .with(Velocity::default())
            .with(camera)
            .with(FreeCam)
            .with(InputState::default())
            .with(MouseInput::default())
            .with(MovementSpeed {
                walk: 45.0,
                sprint: 90.0,
                freecam: 60.0,
                freecam_fast: 200.0,
                current: 60.0,
            })
            .with(InputControlled)
            .build();

        // Set as active camera in entity lookup
        let mut lookup = world.write_resource::<EntityLookup>();
        lookup.active_camera = Some(entity);

        log::info!("[ECS] Created free camera entity: {:?}", entity);

        entity
    }
}

/// Convenience function to create a local player entity
pub fn create_local_player(world: &mut World, aspect_ratio: f32) -> Entity {
    LocalPlayerBuilder::new()
        .aspect_ratio(aspect_ratio)
        .build(world)
}

/// Convenience function to create a remote player entity
pub fn create_remote_player(world: &mut World, player_id: u64, position: glam::Vec3) -> Entity {
    RemotePlayerBuilder::new(player_id)
        .position(position)
        .build(world)
}

/// Convenience function to create a free camera entity
pub fn create_free_camera(world: &mut World, aspect_ratio: f32) -> Entity {
    FreeCameraBuilder::new()
        .aspect_ratio(aspect_ratio)
        .build(world)
}

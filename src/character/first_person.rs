//! First-person view handling (arms/weapon rendering)

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::model::{AnimationPlayer, LoadedModel, SkeletonState};

/// Currently held item type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeldItem {
    #[default]
    None,
}

impl HeldItem {
    /// Get mesh names to hide for this item (all other items)
    pub fn hidden_meshes(&self) -> &'static [&'static str] {
        match self {
            HeldItem::None => &[
                "item_pickaxe",
                "item_pistol",
                "item_sword",
                "pick",
                "pistol",
                "sword",
            ],
        }
    }

    /// Get idle animation name for this item
    pub fn idle_animation(&self) -> &'static str {
        match self {
            HeldItem::None => "idle",
        }
    }

    /// Get run animation name for this item
    pub fn run_animation(&self) -> &'static str {
        match self {
            HeldItem::None => "run",
        }
    }

    /// Get attack/use animation name for this item
    pub fn attack_animation(&self) -> &'static str {
        match self {
            HeldItem::None => "hands_punch",
        }
    }
}

/// First-person view model (hands/weapon)
pub struct FirstPersonView {
    /// Model for first-person arms/weapon
    pub model: Option<Arc<LoadedModel>>,
    /// Skeleton state for animations
    pub skeleton_state: Option<SkeletonState>,
    /// Animation player
    pub animation_player: AnimationPlayer,
    /// Currently held item
    pub held_item: HeldItem,
    /// Hidden mesh names (for item switching)
    pub hidden_meshes: Vec<String>,
    /// Position offset from camera
    pub offset: Vec3,
    /// Rotation offset
    pub rotation_offset: Quat,
    /// Scale
    pub scale: f32,
    /// Camera shake from animation (position)
    camera_shake_pos: Vec3,
    /// Camera shake from animation (rotation)
    camera_shake_rot: Quat,
    /// Is currently attacking
    is_attacking: bool,
    /// Is walking
    is_walking: bool,
    /// Is sprinting
    is_sprinting: bool,
    /// Debug: detach from camera to view in third person
    pub debug_detached: bool,
    /// Debug: saved position when detached
    debug_position: Vec3,
    /// Debug: saved rotation when detached
    debug_rotation: Quat,
}

impl FirstPersonView {
    pub fn new() -> Self {
        Self {
            model: None,
            skeleton_state: None,
            animation_player: AnimationPlayer::new(),
            held_item: HeldItem::None,
            hidden_meshes: Vec::new(),
            // Offset: none (model is already centered at camera)
            offset: Vec3::ZERO,
            // No rotation needed - model is already correctly oriented
            rotation_offset: Quat::IDENTITY,
            // Blockbench exports are pre-divided by 16
            scale: 1.0,
            camera_shake_pos: Vec3::ZERO,
            camera_shake_rot: Quat::IDENTITY,
            is_attacking: false,
            is_walking: false,
            is_sprinting: false,
            debug_detached: false,
            debug_position: Vec3::ZERO,
            debug_rotation: Quat::IDENTITY,
        }
    }

    /// Set the first-person model (arms/weapon)
    pub fn set_model(&mut self, model: Arc<LoadedModel>) {
        self.skeleton_state = model.skeleton.as_ref().map(SkeletonState::from_skeleton);

        self.animation_player = AnimationPlayer::new();
        for clip in &model.animations {
            self.animation_player.add_clip(clip.clone());
        }

        self.model = Some(model);

        // Apply current item visibility
        self.update_item_visibility();

        // Try to play idle animation
        self.play_idle();
    }

    /// Update which meshes are hidden based on held item
    fn update_item_visibility(&mut self) {
        self.hidden_meshes = self
            .held_item
            .hidden_meshes()
            .iter()
            .map(|s| s.to_string())
            .collect();
    }

    /// Play idle animation for current item
    pub fn play_idle(&mut self) {
        // Try item-specific idle first, fallback to generic
        if !self
            .animation_player
            .play_by_name(self.held_item.idle_animation())
        {
            self.animation_player.play_by_name("idle");
        }
        self.is_attacking = false;
    }

    /// Play run animation for current item
    pub fn play_run(&mut self) {
        if self.is_attacking {
            return; // Don't interrupt attack
        }

        if !self
            .animation_player
            .play_by_name(self.held_item.run_animation())
        {
            self.animation_player.play_by_name("run");
        }
    }

    /// Play attack animation for current item (restarts on each call)
    pub fn play_attack(&mut self) {
        if !self
            .animation_player
            .play_by_name_restart(self.held_item.attack_animation())
        {
            // Fallback animation
            self.animation_player.play_by_name_restart("punch");
        }
        self.is_attacking = true;
    }

    /// Update animations and effects
    pub fn update(&mut self, dt: f32, is_walking: bool, is_sprinting: bool, _mouse_delta: Vec3) {
        let was_walking = self.is_walking;
        let was_sprinting = self.is_sprinting;
        self.is_walking = is_walking;
        self.is_sprinting = is_sprinting;

        // Check if attack animation finished
        if self.is_attacking && !self.animation_player.playing {
            self.is_attacking = false;
            if is_walking && is_sprinting {
                self.play_run();
            } else {
                self.play_idle();
            }
        }

        // Switch between idle/run if not attacking
        // idle when standing still or walking, run when sprinting
        if !self.is_attacking {
            let state_changed = is_walking != was_walking || is_sprinting != was_sprinting;
            if state_changed {
                if is_walking && is_sprinting {
                    self.play_run();
                } else {
                    self.play_idle();
                }
            }
        }

        // Update animation
        if let (Some(model), Some(state)) = (&self.model, &mut self.skeleton_state)
            && let Some(skeleton) = &model.skeleton
        {
            self.animation_player.update(dt, skeleton, state);

            // Extract camera shake from "camera_target" bone
            if let Some(idx) = skeleton.find_joint("camera_target") {
                let transform = &state.local_transforms[idx];
                self.camera_shake_pos = transform.translation;
                self.camera_shake_rot = transform.rotation;
            }
        }
    }

    /// Get camera shake offset from animation
    pub fn get_camera_shake(&self) -> (Vec3, Quat) {
        (self.camera_shake_pos * self.scale, self.camera_shake_rot)
    }

    /// Get the transformation matrix relative to camera
    pub fn view_transform(&self, camera_position: Vec3, camera_rotation: Quat) -> Mat4 {
        // Use saved position/rotation if detached (debug mode)
        let (pos, rot) = if self.debug_detached {
            (self.debug_position, self.debug_rotation)
        } else {
            (camera_position, camera_rotation)
        };

        // Transform offset to world space (offset is in camera local space: x=right, y=up, z=forward)
        let world_offset = rot * self.offset;

        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            rot * self.rotation_offset,
            pos + world_offset,
        )
    }
}

impl Default for FirstPersonView {
    fn default() -> Self {
        Self::new()
    }
}

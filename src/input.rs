use std::collections::HashSet;

use specs::WorldExt;
use winit::{event::*, event_loop::ActiveEventLoop, keyboard::KeyCode, window::CursorGrabMode};

use crate::AppState;
use crate::ui::{BrushShape, UiScreen};
use crate::voxel::{AIR, raycast};
use crate::world::{ChunkPosition, RegionCoord};

impl AppState {
    pub(crate) fn grab_mouse(&mut self, grab: bool) {
        self.mouse_grabbed = grab;
        if grab {
            // Try Locked first (locks cursor in place), fall back to Confined
            let _ = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined));
            self.window.set_cursor_visible(false);
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        }
    }

    pub(crate) fn handle_key(
        &mut self,
        _event_loop: &ActiveEventLoop,
        code: KeyCode,
        is_pressed: bool,
    ) {
        // Handle key releases — write to InputResource
        if !is_pressed {
            let mut input = self
                .ecs_world
                .write_resource::<crate::ecs::resources::InputResource>();
            match code {
                KeyCode::KeyW => input.forward = false,
                KeyCode::KeyS => input.backward = false,
                KeyCode::KeyA => input.left = false,
                KeyCode::KeyD => input.right = false,
                KeyCode::Space => input.jump = false,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    input.down = false;
                    input.sprint = false;
                }
                KeyCode::KeyQ => input.up = false,
                KeyCode::KeyE => input.down = false,
                KeyCode::ControlLeft | KeyCode::ControlRight => input.ctrl = false,
                _ => {}
            }
            return;
        }

        // Handle Escape for all screens
        if code == KeyCode::Escape {
            if self.ui_screen == UiScreen::InGame {
                self.ui_screen = UiScreen::PauseMenu;
                self.grab_mouse(false);
            } else if self.ui_screen == UiScreen::PauseMenu {
                self.ui_screen = UiScreen::InGame;
                self.grab_mouse(true);
            } else if self.ui_screen == UiScreen::Matchmaking {
                self.ui_screen = UiScreen::MainMenu;
            } else if self.ui_screen == UiScreen::MapSelect {
                self.ui_screen = UiScreen::MainMenu;
            } else if self.ui_screen == UiScreen::Settings {
                self.ui_screen = self.prev_screen;
            } else if self.ui_screen != UiScreen::MainMenu {
                self.handle_dev_escape();
            }
            return;
        }

        // Handle Ctrl modifier
        if matches!(code, KeyCode::ControlLeft | KeyCode::ControlRight) {
            self.ecs_world
                .write_resource::<crate::ecs::resources::InputResource>()
                .ctrl = true;
            return;
        }

        // Handle Ctrl+S for save in editor
        if self.handle_dev_key(code) {
            return;
        }

        // Movement keys for InGame and Editor
        let can_move = self.ui_screen == UiScreen::InGame || self.can_move_editor();
        if can_move {
            // Movement input → InputResource
            {
                let mut input = self
                    .ecs_world
                    .write_resource::<crate::ecs::resources::InputResource>();
                match code {
                    KeyCode::KeyW => input.forward = true,
                    KeyCode::KeyS => input.backward = true,
                    KeyCode::KeyA => input.left = true,
                    KeyCode::KeyD => input.right = true,
                    KeyCode::Space => input.jump = true,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                        input.down = true;
                        input.sprint = true;
                    }
                    KeyCode::KeyQ => input.up = true,
                    KeyCode::KeyE => input.down = true,
                    _ => {}
                }
            }
            // Special keys (not input)
            match code {
                KeyCode::F3 => {
                    self.game_settings.show_debug = !self.game_settings.show_debug;
                }
                KeyCode::F5 => {
                    let entering_free_cam = !self.is_free_cam();
                    if entering_free_cam {
                        // Enter free cam: player Camera component retains current yaw/pitch
                        // (frozen for frustum culling vis)

                        // Create free cam entity at current camera position
                        crate::ecs::FreeCameraBuilder::new()
                            .position(self.camera.position)
                            .rotation(self.camera.yaw, self.camera.pitch)
                            .aspect_ratio(self.camera.aspect)
                            .build(&mut self.ecs_world);
                        log::info!(
                            "[FreeCam] Enabled — frustum culls from player perspective"
                        );
                    } else {
                        // Destroy free cam entity and restore player camera
                        let lookup =
                            self.ecs_world.read_resource::<crate::ecs::resources::EntityLookup>();
                        let cam_entity = lookup.active_camera;
                        let local_player = lookup.local_player;
                        drop(lookup);

                        // Read frozen yaw/pitch from player's ECS Camera component
                        let (frozen_yaw, frozen_pitch) = self.player_camera_yaw_pitch();

                        // Delete the free cam entity (if it's not the player)
                        if let Some(cam) = cam_entity {
                            if Some(cam) != local_player {
                                self.ecs_world.delete_entity(cam).ok();
                            }
                        }

                        // Restore active camera to local player
                        let mut lookup = self
                            .ecs_world
                            .write_resource::<crate::ecs::resources::EntityLookup>();
                        lookup.active_camera = local_player;
                        drop(lookup);

                        // Restore render camera from frozen player orientation
                        self.camera.position = self.player_eye_position();
                        self.camera.yaw = frozen_yaw;
                        self.camera.pitch = frozen_pitch;

                        log::info!("[FreeCam] Disabled — camera follows player");
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        let allow_look = self.ui_screen == UiScreen::InGame || self.can_move_editor();

        if self.mouse_grabbed && allow_look {
            // Accumulate mouse deltas in InputResource; processed by CameraUpdateSystem
            let mut input = self
                .ecs_world
                .write_resource::<crate::ecs::resources::InputResource>();
            input.mouse_delta_x += delta.0 as f32;
            input.mouse_delta_y += delta.1 as f32;
        }
    }

    pub(crate) fn handle_mouse_button(&mut self, button: MouseButton, is_pressed: bool) {
        if !is_pressed {
            return;
        }

        if !self.mouse_grabbed {
            if button == MouseButton::Left {
                self.grab_mouse(true);
            }
            return;
        }

        let can_interact = self.ui_screen == UiScreen::InGame || self.can_move_editor();

        // Only handle block interaction in active gameplay modes
        if !can_interact {
            return;
        }

        const MAX_REACH: f32 = 8.0;
        let origin = self.camera.position;
        let direction = self.camera.forward();

        let hit = raycast(origin, direction, MAX_REACH, |x, y, z| {
            self.world.is_solid(x, y, z)
        });

        let is_editor = self.editor_active();
        let color_to_place = if is_editor {
            self.editor_state.selected_color
        } else {
            33 // Default dirt color
        };

        // Get brush settings (editor only)
        let (brush_shape, brush_size) = (
            self.editor_state.brush_shape,
            self.editor_state.brush_size as i32,
        );

        match button {
            MouseButton::Left => {
                // Play attack animation
                self.character_manager.first_person.play_attack();

                if let Some(hit) = hit {
                    let [cx, cy, cz] = hit.block_pos;

                    // In game mode, punch destroys an exact 2x2x2 voxel block.
                    let positions = if is_editor {
                        Self::get_brush_positions(brush_shape, brush_size, cx, cy, cz)
                    } else {
                        Self::get_punch_positions_2x2(cx, cy, cz)
                    };
                    let batch: Vec<_> = positions
                        .into_iter()
                        .filter(|&(x, y, z)| self.world.is_solid(x, y, z))
                        .map(|(x, y, z)| (x, y, z, AIR))
                        .collect();
                    self.mark_regions_dirty_for_edits(&batch);
                    self.world.set_voxels_batch(&batch);
                    self.update_octree_for_edits(&batch);
                    self.path_tracer.notify_voxel_edits(
                        &self.render_ctx.queue,
                        &self.world,
                        &batch,
                    );
                }
            }
            MouseButton::Right => {
                // Only allow placement in editor mode
                if !is_editor {
                    return;
                }

                if let Some(hit) = hit {
                    let [x, y, z] = hit.block_pos;
                    let [nx, ny, nz] = hit.normal;
                    let place_x = x + nx;
                    let place_y = y + ny;
                    let place_z = z + nz;

                    // Apply brush pattern for placement using batch operation
                    let positions = Self::get_brush_positions(
                        brush_shape,
                        brush_size,
                        place_x,
                        place_y,
                        place_z,
                    );
                    let batch: Vec<_> = positions
                        .into_iter()
                        .filter(|&(x, y, z)| !self.world.is_solid(x, y, z))
                        .map(|(x, y, z)| (x, y, z, color_to_place))
                        .collect();
                    self.mark_regions_dirty_for_edits(&batch);
                    self.world.set_voxels_batch(&batch);
                    self.update_octree_for_edits(&batch);
                    self.path_tracer.notify_voxel_edits(
                        &self.render_ctx.queue,
                        &self.world,
                        &batch,
                    );
                }
            }
            _ => {}
        }
    }

    /// Update octree LOD data for chunks affected by voxel edits.
    /// This triggers LOD dirty propagation so LOD meshes rebuild.
    pub(crate) fn update_octree_for_edits(&mut self, batch: &[(i32, i32, i32, u8)]) {
        let Some(ref mut octree) = self.octree else {
            return;
        };
        let mut seen = HashSet::new();
        for &(wx, wy, wz, _) in batch {
            let (chunk_pos, _, _, _) = ChunkPosition::world_to_local(wx, wy, wz);
            if !seen.insert(chunk_pos) {
                continue;
            }
            if let Some(chunk) = self.world.get_chunk(chunk_pos) {
                let data = crate::world::lod::VoxelData::from_full(*chunk.data());
                octree.insert_chunk(chunk_pos, data);
            }
        }
    }

    /// Mark regions dirty for a batch of voxel edits.
    pub(crate) fn mark_regions_dirty_for_edits(&mut self, batch: &[(i32, i32, i32, u8)]) {
        if let Some(ref mut region_mgr) = self.region_manager {
            let mut seen = HashSet::new();
            for &(wx, _wy, wz, _) in batch {
                let chunk_pos = ChunkPosition::from_world_coords(wx, 0, wz);
                let coord = RegionCoord::from_chunk_pos(chunk_pos);
                if seen.insert(coord) {
                    region_mgr.mark_dirty(coord);
                }
            }
        }
    }

    /// Generate voxel positions for the given brush shape and size
    pub(crate) fn get_brush_positions(
        shape: BrushShape,
        size: i32,
        cx: i32,
        cy: i32,
        cz: i32,
    ) -> Vec<(i32, i32, i32)> {
        match shape {
            BrushShape::Single => {
                vec![(cx, cy, cz)]
            }
            BrushShape::Sphere => {
                let mut positions = Vec::new();
                let radius = size;
                let radius_sq = (radius * radius) as f32;

                for dx in -radius..=radius {
                    for dy in -radius..=radius {
                        for dz in -radius..=radius {
                            let dist_sq = (dx * dx + dy * dy + dz * dz) as f32;
                            if dist_sq <= radius_sq {
                                positions.push((cx + dx, cy + dy, cz + dz));
                            }
                        }
                    }
                }
                positions
            }
            BrushShape::Cube => {
                let mut positions = Vec::new();
                let half = size;

                for dx in -half..=half {
                    for dy in -half..=half {
                        for dz in -half..=half {
                            positions.push((cx + dx, cy + dy, cz + dz));
                        }
                    }
                }
                positions
            }
        }
    }

    /// Generate a 2x2x2 voxel block around the hit voxel, aligned to even voxel coordinates.
    pub(crate) fn get_punch_positions_2x2(cx: i32, cy: i32, cz: i32) -> Vec<(i32, i32, i32)> {
        let start_x = cx & !1;
        let start_y = cy & !1;
        let start_z = cz & !1;
        let mut positions = Vec::with_capacity(8);
        for dx in 0..=1 {
            for dy in 0..=1 {
                for dz in 0..=1 {
                    positions.push((start_x + dx, start_y + dy, start_z + dz));
                }
            }
        }
        positions
    }
}

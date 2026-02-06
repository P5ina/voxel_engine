use std::collections::HashSet;

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
        // Handle key releases
        if !is_pressed {
            match code {
                KeyCode::KeyW => self.input.forward = false,
                KeyCode::KeyS => self.input.backward = false,
                KeyCode::KeyA => self.input.left = false,
                KeyCode::KeyD => self.input.right = false,
                KeyCode::Space => self.input.jump = false,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    self.input.down = false;
                    self.input.sprint = false;
                }
                KeyCode::KeyQ => self.input.up = false,
                KeyCode::KeyE => self.input.down = false,
                KeyCode::ControlLeft | KeyCode::ControlRight => self.input.ctrl = false,
                _ => {}
            }
            return;
        }

        // Handle Escape for all screens
        if code == KeyCode::Escape {
            match self.ui_screen {
                UiScreen::InGame => {
                    self.ui_screen = UiScreen::PauseMenu;
                    self.grab_mouse(false);
                }
                UiScreen::PauseMenu => {
                    self.ui_screen = UiScreen::InGame;
                    self.grab_mouse(true);
                }
                UiScreen::Editor => {
                    self.prev_screen = UiScreen::Editor;
                    self.ui_screen = UiScreen::EditorPause;
                    self.grab_mouse(false);
                }
                UiScreen::EditorPause => {
                    self.ui_screen = UiScreen::Editor;
                    self.grab_mouse(true);
                }
                UiScreen::Settings => {
                    self.ui_screen = self.prev_screen;
                }
                UiScreen::WorldSelect => {
                    self.ui_screen = UiScreen::MainMenu;
                }
                UiScreen::MainMenu => {}
                UiScreen::Loading => {
                    // Can't escape during loading
                }
            }
            return;
        }

        // Handle Ctrl modifier
        if matches!(code, KeyCode::ControlLeft | KeyCode::ControlRight) {
            self.input.ctrl = true;
            return;
        }

        // Handle Ctrl+S for save in editor
        if self.ui_screen == UiScreen::Editor && self.input.ctrl && code == KeyCode::KeyS {
            self.save_big_world_to_file();
            return;
        }

        // Movement keys for InGame and Editor
        if self.ui_screen == UiScreen::InGame || self.ui_screen == UiScreen::Editor {
            match code {
                KeyCode::KeyW => self.input.forward = true,
                KeyCode::KeyS => self.input.backward = true,
                KeyCode::KeyA => self.input.left = true,
                KeyCode::KeyD => self.input.right = true,
                KeyCode::Space => self.input.jump = true,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    self.input.down = true;
                    self.input.sprint = true;
                }
                KeyCode::KeyQ => self.input.up = true,
                KeyCode::KeyE => self.input.down = true,
                KeyCode::F3 => {
                    self.game_settings.show_debug = !self.game_settings.show_debug;
                }
                KeyCode::F5 => {
                    self.free_cam = !self.free_cam;
                    if self.free_cam {
                        // Enter free cam: camera starts at current player eye position
                        log::info!(
                            "[FreeCam] Enabled — LOD loads around player, camera flies free"
                        );
                    } else {
                        // Exit free cam: snap camera back to player
                        self.camera.position = self.player.eye_position();
                        log::info!("[FreeCam] Disabled — camera follows player");
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        if self.mouse_grabbed
            && (self.ui_screen == UiScreen::InGame || self.ui_screen == UiScreen::Editor)
        {
            let sensitivity = self.game_settings.sensitivity;
            self.camera
                .process_mouse(delta.0 as f32, delta.1 as f32, sensitivity);
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

        // Only handle block interaction in game or editor mode
        if self.ui_screen != UiScreen::InGame && self.ui_screen != UiScreen::Editor {
            return;
        }

        const MAX_REACH: f32 = 8.0;
        let origin = self.camera.position;
        let direction = self.camera.forward();

        let hit = raycast(origin, direction, MAX_REACH, |x, y, z| {
            self.world.is_solid(x, y, z)
        });

        let is_editor = self.ui_screen == UiScreen::Editor;
        // Get color to place from editor or default
        let color_to_place = if is_editor {
            self.editor_state.selected_color
        } else {
            33 // Default dirt color
        };

        // Get brush settings
        let (brush_shape, brush_size) = if is_editor {
            (
                self.editor_state.brush_shape,
                self.editor_state.brush_size as i32,
            )
        } else {
            // In game mode: sphere destruction with radius 8
            (BrushShape::Sphere, 8)
        };

        match button {
            MouseButton::Left => {
                // Play attack animation
                self.character_manager.first_person.play_attack();

                if let Some(hit) = hit {
                    let [cx, cy, cz] = hit.block_pos;

                    // Apply brush pattern for destruction using batch operation
                    let positions = Self::get_brush_positions(brush_shape, brush_size, cx, cy, cz);
                    let batch: Vec<_> = positions
                        .into_iter()
                        .filter(|&(x, y, z)| self.world.is_solid(x, y, z))
                        .map(|(x, y, z)| (x, y, z, AIR))
                        .collect();
                    self.mark_regions_dirty_for_edits(&batch);
                    self.world.set_voxels_batch(&batch);
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
                }
            }
            _ => {}
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
}

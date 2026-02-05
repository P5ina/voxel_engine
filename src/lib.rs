use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window},
};

mod bvh;
mod camera;
mod character;
mod model;
mod pathtracer;
mod player;
mod renderer;
mod ui;
mod voxel;
mod world;

use camera::Camera;
use model::load_glb;
use pathtracer::PathTracer;
use player::Player;
use renderer::{
    CameraResources, DepthBuffer, LightingParams, MeshResources, RenderContext, TextureResources,
};
use ui::{EditorState, EguiRenderer, GameSettings, UiMessage, UiScreen};
use voxel::{BlockType, Chunk, generate_chunk_mesh, raycast};
use world::{ChunkManager, ChunkPosition, SaveFormat};

#[derive(Default)]
struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    jump: bool,
    up: bool,
    down: bool,
    ctrl: bool,
}

pub struct AppState {
    window: Arc<Window>,

    // Rendering
    render_ctx: RenderContext,
    depth_buffer: DepthBuffer,
    camera_resources: CameraResources,
    texture_resources: TextureResources,
    chunk_meshes: HashMap<ChunkPosition, MeshResources>,
    lighting: LightingParams,
    path_tracer: PathTracer,
    character_manager: character::CharacterManager,

    // Game state
    camera: Camera,
    player: Player,
    world: ChunkManager,

    // Input
    input: InputState,
    mouse_grabbed: bool,

    // UI
    egui: EguiRenderer,
    ui_screen: UiScreen,
    game_settings: GameSettings,
    prev_screen: UiScreen,

    // Editor
    editor_state: EditorState,
    available_levels: Vec<String>,

    // Timing
    last_frame: std::time::Instant,
    fps: f32,
    frame_time_accum: f32,
    frame_count: u32,
}

impl AppState {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();

        // Initialize render context
        let render_ctx = RenderContext::new(window.clone()).await?;

        // Player and camera setup
        let player = Player::new(Vec3::new(16.0, 12.0, 16.0));
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(player.eye_position(), aspect);

        // Create resources
        let camera_resources = CameraResources::new(&render_ctx.device, &camera);
        let texture_resources = TextureResources::new(&render_ctx.device, &render_ctx.queue);
        let depth_buffer = DepthBuffer::new(&render_ctx.device, size.width, size.height);

        // Create world with initial chunk
        let mut world = ChunkManager::with_metadata("Default World", [16.0, 20.0, 16.0]);
        let mut chunk = Chunk::new();
        chunk.fill_ground(8);
        let default_pos = ChunkPosition::new(0, 0, 0);
        world.insert_chunk(default_pos, chunk);

        // Generate meshes for all chunks
        let mut chunk_meshes = HashMap::new();
        for chunk_pos in world.chunk_positions().cloned().collect::<Vec<_>>() {
            let vertices = generate_chunk_mesh(&world, chunk_pos);
            if !vertices.is_empty() {
                chunk_meshes.insert(chunk_pos, MeshResources::new(&render_ctx.device, &vertices));
            }
        }
        world.take_dirty_chunks(); // Clear dirty flags after initial mesh build

        // Lighting
        let mut lighting = LightingParams::new();
        lighting.update_time(0.1);

        // UI
        let egui = EguiRenderer::new(&render_ctx.device, render_ctx.format(), &window);

        // Character Manager (must be created before PathTracer)
        let mut character_manager = character::CharacterManager::new(&render_ctx.device);

        // Load first-person arms model
        match load_glb("assets/models/player/fps_arms.glb") {
            Ok(fps_model) => {
                eprintln!(
                    "[FPS] Loaded: {} meshes, {} animations",
                    fps_model.meshes.len(),
                    fps_model.animations.len()
                );
                // Print skeleton info
                if let Some(skeleton) = &fps_model.skeleton {
                    eprintln!("[FPS] Skeleton: {} joints, roots: {:?}",
                        skeleton.joints.len(), skeleton.root_joints);
                    for (i, joint) in skeleton.joints.iter().enumerate() {
                        eprintln!("[FPS]   Joint {}: '{}' children={:?}",
                            i, joint.name, joint.children);
                    }
                } else {
                    eprintln!("[FPS] No skeleton!");
                }
                // Print vertex joint info for first mesh
                if let Some(mesh) = fps_model.meshes.first() {
                    if let Some(v) = mesh.vertices.first() {
                        eprintln!("[FPS] First vertex joints: {:?}, weights: {:?}",
                            v.joint_indices, v.joint_weights);
                    }
                }
                character_manager.first_person.set_model(Arc::new(fps_model));
            }
            Err(e) => {
                eprintln!("[FPS] Failed to load: {}", e);
            }
        }

        // Path Tracer
        let mut path_tracer = PathTracer::new(
            &render_ctx.device,
            &render_ctx.queue,
            size.width,
            size.height,
            render_ctx.format(),
            &camera_resources.bind_group_layout,
            &texture_resources.bind_group_layout,
            character_manager.bind_group_layout(),
        );
        path_tracer.update_world_voxels(&render_ctx.device, &render_ctx.queue, &world);

        // Scan for available levels
        let available_levels = Self::scan_levels();

        Ok(Self {
            window,
            render_ctx,
            depth_buffer,
            camera_resources,
            texture_resources,
            chunk_meshes,
            lighting,
            path_tracer,
            character_manager,
            camera,
            player,
            world,
            input: InputState::default(),
            mouse_grabbed: false,
            egui,
            ui_screen: UiScreen::default(),
            game_settings: GameSettings::default(),
            prev_screen: UiScreen::MainMenu,
            editor_state: EditorState::default(),
            available_levels,
            last_frame: std::time::Instant::now(),
            fps: 0.0,
            frame_time_accum: 0.0,
            frame_count: 0,
        })
    }

    fn scan_levels() -> Vec<String> {
        let maps_dir = std::path::Path::new("maps");
        if !maps_dir.exists() {
            return Vec::new();
        }

        std::fs::read_dir(maps_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.render_ctx.resize(width, height);
            self.depth_buffer
                .resize(&self.render_ctx.device, width, height);
            self.camera.resize(width, height);
            self.path_tracer
                .resize(&self.render_ctx.device, width, height);
        }
    }

    fn rebuild_dirty_meshes(&mut self) {
        let dirty = self.world.take_dirty_chunks();

        for chunk_pos in dirty {
            if self.world.get_chunk(chunk_pos).is_some() {
                let vertices = generate_chunk_mesh(&self.world, chunk_pos);
                if vertices.is_empty() {
                    self.chunk_meshes.remove(&chunk_pos);
                } else if let Some(mesh) = self.chunk_meshes.get_mut(&chunk_pos) {
                    mesh.update(&self.render_ctx.device, &vertices);
                } else {
                    self.chunk_meshes.insert(
                        chunk_pos,
                        MeshResources::new(&self.render_ctx.device, &vertices),
                    );
                }
            } else {
                self.chunk_meshes.remove(&chunk_pos);
            }
        }

        self.path_tracer.update_world_voxels(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &self.world,
        );
        self.path_tracer.reset_accumulation();
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        // FPS calculation (update every 0.5 seconds)
        self.frame_time_accum += dt;
        self.frame_count += 1;
        if self.frame_time_accum >= 0.5 {
            self.fps = self.frame_count as f32 / self.frame_time_accum;
            self.frame_time_accum = 0.0;
            self.frame_count = 0;
        }

        // Player movement input
        const MOVE_SPEED: f32 = 45.0;
        let input = Vec3::new(
            (self.input.right as i32 - self.input.left as i32) as f32,
            0.0,
            (self.input.forward as i32 - self.input.backward as i32) as f32,
        );

        let forward = self.camera.forward().with_y(0.0).normalize_or_zero();
        let right = self.camera.right();
        self.player
            .apply_movement(forward, right, input, MOVE_SPEED * dt);

        if self.input.jump {
            self.player.jump();
        }

        self.player.update(&self.world, dt);
        self.camera.position = self.player.eye_position();
        self.camera.fov = self.game_settings.fov.to_radians();

        // Head bob when walking
        let is_walking = input.length_squared() > 0.0;
        self.camera
            .update_bob(is_walking, self.player.on_ground, dt);

        self.camera_resources
            .update(&self.render_ctx.queue, &self.camera);
        self.lighting.update_time(0.1);

        // Build camera rotation for first-person view (yaw + pitch)
        let yaw_quat = glam::Quat::from_rotation_y(-self.camera.yaw - std::f32::consts::FRAC_PI_2);
        let pitch_quat = glam::Quat::from_rotation_x(self.camera.pitch);
        let camera_rotation = yaw_quat * pitch_quat;

        // Update character manager (builds BVH for path tracing)
        self.character_manager.update(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            dt,
            is_walking,
            self.camera.position,
            camera_rotation,
        );

        // Apply camera shake from first-person animation
        let (shake_pos, _shake_rot) = self.character_manager.first_person.get_camera_shake();
        self.camera.position += camera_rotation * shake_pos;

        if !self.world.dirty_chunks().is_empty() {
            self.rebuild_dirty_meshes();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        match self.ui_screen {
            UiScreen::InGame => self.update(),
            UiScreen::Editor => self.update_editor(),
            _ => {
                self.last_frame = std::time::Instant::now();
            }
        }
        self.window.request_redraw();

        if !self.render_ctx.is_configured {
            return Ok(());
        }

        // Begin egui frame
        self.egui.begin_frame(&self.window);

        // Build UI
        let msg = match self.ui_screen {
            UiScreen::MainMenu => ui::main_menu(&self.egui.ctx),
            UiScreen::Settings => ui::settings(&self.egui.ctx, &mut self.game_settings),
            UiScreen::PauseMenu => ui::pause_menu(&self.egui.ctx),
            UiScreen::InGame => {
                ui::hud(&self.egui.ctx, self.fps);
                None
            }
            UiScreen::LevelSelect => ui::level_select(&self.egui.ctx, &self.available_levels),
            UiScreen::Editor => ui::editor_hud(&self.egui.ctx, &mut self.editor_state, self.fps),
            UiScreen::EditorPause => ui::editor_pause(&self.egui.ctx),
            UiScreen::SaveDialog => {
                ui::save_dialog(&self.egui.ctx, &mut self.editor_state.level_name)
            }
        };

        if let Some(msg) = msg {
            self.handle_ui_message(msg);
        }

        let output = self.render_ctx.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.render_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        // Render 3D scene
        let should_render_3d = matches!(
            self.ui_screen,
            UiScreen::InGame
                | UiScreen::PauseMenu
                | UiScreen::Editor
                | UiScreen::EditorPause
                | UiScreen::SaveDialog
        );

        if should_render_3d {
            self.camera.fov = self.game_settings.fov.to_radians();
            self.camera_resources
                .update(&self.render_ctx.queue, &self.camera);

            self.path_tracer.update_params(
                &self.render_ctx.queue,
                &self.camera,
                self.lighting.sun_direction,
                self.lighting.sun_intensity,
                self.lighting.sun_color,
            );

            let meshes = self
                .chunk_meshes
                .values()
                .map(|m| (&m.vertex_buffer, m.num_vertices));

            self.path_tracer.render(
                &mut encoder,
                &view,
                &self.depth_buffer.view,
                &self.camera_resources.bind_group,
                &self.texture_resources.bind_group,
                self.character_manager.bind_group(),
                meshes,
                self.game_settings.lighting_mode,
            );
        } else {
            // Menu background
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Menu Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        // Render egui
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.render_ctx.config.width, self.render_ctx.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        self.egui.end_frame(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &mut encoder,
            &self.window,
            &view,
            screen_descriptor,
        );

        self.render_ctx
            .queue
            .submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn handle_ui_message(&mut self, msg: UiMessage) {
        match msg {
            UiMessage::Play => {
                self.ui_screen = UiScreen::InGame;
                self.grab_mouse(true);
            }
            UiMessage::Settings => {
                self.prev_screen = self.ui_screen;
                self.ui_screen = UiScreen::Settings;
            }
            UiMessage::Exit => {
                std::process::exit(0);
            }
            UiMessage::Back => {
                self.ui_screen = self.prev_screen;
            }
            UiMessage::Resume => {
                // Resume to appropriate mode
                if self.prev_screen == UiScreen::Editor
                    || self.ui_screen == UiScreen::EditorPause
                    || self.ui_screen == UiScreen::SaveDialog
                {
                    self.ui_screen = UiScreen::Editor;
                } else {
                    self.ui_screen = UiScreen::InGame;
                }
                self.grab_mouse(true);
            }
            UiMessage::QuitToMenu => {
                self.ui_screen = UiScreen::MainMenu;
                self.grab_mouse(false);
            }
            // Level select
            UiMessage::OpenLevelSelect => {
                self.available_levels = Self::scan_levels();
                self.prev_screen = UiScreen::MainMenu;
                self.ui_screen = UiScreen::LevelSelect;
            }
            UiMessage::LoadLevel(name) => {
                if self.load_level(&name) {
                    self.ui_screen = UiScreen::InGame;
                    self.grab_mouse(true);
                }
            }
            UiMessage::NewLevel => {
                self.create_new_level();
                self.ui_screen = UiScreen::InGame;
                self.grab_mouse(true);
            }
            // Editor
            UiMessage::OpenEditor => {
                self.create_new_level();
                self.ui_screen = UiScreen::Editor;
                self.grab_mouse(true);
            }
            UiMessage::SaveLevel(name) => {
                if name.is_empty() {
                    // Open save dialog
                    self.prev_screen = UiScreen::EditorPause;
                    self.ui_screen = UiScreen::SaveDialog;
                } else {
                    // Actually save
                    self.save_level(&name);
                    self.ui_screen = UiScreen::Editor;
                    self.grab_mouse(true);
                }
            }
            UiMessage::EditorQuitToMenu => {
                self.ui_screen = UiScreen::MainMenu;
                self.grab_mouse(false);
            }
        }
    }

    fn create_new_level(&mut self) {
        // Create empty world with single empty chunk
        self.world = ChunkManager::with_metadata("New Level", [16.0, 20.0, 16.0]);
        let mut chunk = Chunk::new();
        chunk.fill_ground(1); // Just a floor
        self.world.insert_chunk(ChunkPosition::new(0, 0, 0), chunk);

        // Reset player position
        self.player.position = Vec3::new(16.0, 5.0, 16.0);
        self.player.velocity = Vec3::ZERO;
        self.camera.position = self.player.eye_position();

        // Rebuild meshes
        self.chunk_meshes.clear();
        for chunk_pos in self.world.chunk_positions().cloned().collect::<Vec<_>>() {
            let vertices = generate_chunk_mesh(&self.world, chunk_pos);
            if !vertices.is_empty() {
                self.chunk_meshes.insert(
                    chunk_pos,
                    MeshResources::new(&self.render_ctx.device, &vertices),
                );
            }
        }
        self.world.take_dirty_chunks();

        // Update path tracer
        self.path_tracer.update_world_voxels(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &self.world,
        );
        self.path_tracer.reset_accumulation();

        self.editor_state.level_name = "untitled".to_string();
    }

    fn load_level(&mut self, name: &str) -> bool {
        let path = format!("maps/{}/world.bin", name);
        match ChunkManager::load(&path) {
            Ok(world) => {
                let spawn = world.spawn_position();
                self.world = world;

                // Reset player position
                self.player.position = Vec3::from_array(spawn);
                self.player.velocity = Vec3::ZERO;
                self.camera.position = self.player.eye_position();

                // Rebuild meshes
                self.chunk_meshes.clear();
                for chunk_pos in self.world.chunk_positions().cloned().collect::<Vec<_>>() {
                    let vertices = generate_chunk_mesh(&self.world, chunk_pos);
                    if !vertices.is_empty() {
                        self.chunk_meshes.insert(
                            chunk_pos,
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                }
                self.world.take_dirty_chunks();

                // Update path tracer
                self.path_tracer.update_world_voxels(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    &self.world,
                );
                self.path_tracer.reset_accumulation();

                self.editor_state.level_name = name.to_string();
                true
            }
            Err(e) => {
                log::error!("Failed to load level '{}': {}", name, e);
                false
            }
        }
    }

    fn save_level(&mut self, name: &str) {
        let dir = format!("maps/{}", name);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("Failed to create directory '{}': {}", dir, e);
            return;
        }

        // Save spawn position
        self.world.set_spawn(self.player.position.to_array());

        let path = format!("{}/world.bin", dir);
        if let Err(e) = self.world.save(&path, SaveFormat::Binary) {
            log::error!("Failed to save level '{}': {}", name, e);
        } else {
            log::info!("Level '{}' saved successfully", name);
            self.editor_state.level_name = name.to_string();
            self.available_levels = Self::scan_levels();
        }
    }

    fn grab_mouse(&mut self, grab: bool) {
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

    fn handle_key(&mut self, _event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        // Handle key releases
        if !is_pressed {
            match code {
                KeyCode::KeyW => self.input.forward = false,
                KeyCode::KeyS => self.input.backward = false,
                KeyCode::KeyA => self.input.left = false,
                KeyCode::KeyD => self.input.right = false,
                KeyCode::Space => self.input.jump = false,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.down = false,
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
                UiScreen::SaveDialog => {
                    self.ui_screen = UiScreen::EditorPause;
                }
                UiScreen::Settings => {
                    self.ui_screen = self.prev_screen;
                }
                UiScreen::LevelSelect => {
                    self.ui_screen = UiScreen::MainMenu;
                }
                UiScreen::MainMenu => {}
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
            self.prev_screen = UiScreen::Editor;
            self.ui_screen = UiScreen::SaveDialog;
            self.grab_mouse(false);
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
                KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.down = true,
                KeyCode::KeyQ => self.input.up = true,
                KeyCode::KeyE => self.input.down = true,
                KeyCode::F5 => {
                    // Toggle debug detach mode for first-person view
                    let yaw_quat = glam::Quat::from_rotation_y(-self.camera.yaw - std::f32::consts::FRAC_PI_2);
                    let pitch_quat = glam::Quat::from_rotation_x(self.camera.pitch);
                    let camera_rotation = yaw_quat * pitch_quat;
                    self.character_manager.first_person.toggle_detach(self.camera.position, camera_rotation);
                }
                _ => {}
            }
        }
    }

    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        if self.mouse_grabbed
            && (self.ui_screen == UiScreen::InGame || self.ui_screen == UiScreen::Editor)
        {
            let sensitivity = self.game_settings.sensitivity;
            self.camera
                .process_mouse(delta.0 as f32, delta.1 as f32, sensitivity);
        }
    }

    fn handle_mouse_button(&mut self, button: MouseButton, is_pressed: bool) {
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
        let block_to_place = if is_editor {
            self.editor_state.selected_block
        } else {
            BlockType::Stone
        };

        match button {
            MouseButton::Left => {
                if let Some(hit) = hit {
                    let [x, y, z] = hit.block_pos;
                    self.world.set_block(x, y, z, BlockType::Air);
                }
            }
            MouseButton::Right => {
                if let Some(hit) = hit {
                    let [x, y, z] = hit.block_pos;
                    let [nx, ny, nz] = hit.normal;
                    let place_x = x + nx;
                    let place_y = y + ny;
                    let place_z = z + nz;

                    // In editor mode, no collision check needed (free cam)
                    let can_place =
                        is_editor || !self.player.intersects_block(place_x, place_y, place_z);

                    if can_place {
                        self.world
                            .set_block(place_x, place_y, place_z, block_to_place);
                    }
                }
            }
            _ => {}
        }
    }

    fn update_editor(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        // FPS calculation
        self.frame_time_accum += dt;
        self.frame_count += 1;
        if self.frame_time_accum >= 0.5 {
            self.fps = self.frame_count as f32 / self.frame_time_accum;
            self.frame_time_accum = 0.0;
            self.frame_count = 0;
        }

        // Free camera movement
        let speed = self.editor_state.fly_speed * dt;
        let forward = self.camera.forward();
        let right = self.camera.right();
        let up = Vec3::Y;

        let mut move_dir = Vec3::ZERO;

        if self.input.forward {
            move_dir += forward;
        }
        if self.input.backward {
            move_dir -= forward;
        }
        if self.input.right {
            move_dir += right;
        }
        if self.input.left {
            move_dir -= right;
        }
        if self.input.jump || self.input.up {
            move_dir += up;
        }
        if self.input.down {
            move_dir -= up;
        }

        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize() * speed;
            self.camera.position += move_dir;
            // Keep player position synced for spawn point
            self.player.position =
                self.camera.position - Vec3::new(0.0, self.player.height - 0.2, 0.0);
        }

        self.camera.fov = self.game_settings.fov.to_radians();
        self.camera_resources
            .update(&self.render_ctx.queue, &self.camera);
        self.lighting.update_time(0.1);

        if !self.world.dirty_chunks().is_empty() {
            self.rebuild_dirty_meshes();
        }
    }
}

pub struct App {
    state: Option<AppState>,
}

impl App {
    pub fn new() -> Self {
        Self { state: None }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler<AppState> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("THE DROP - Voxel Engine");
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        self.state = Some(pollster::block_on(AppState::new(window)).unwrap());
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppState) {
        self.state = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };

        let egui_response = state.egui.handle_window_event(&state.window, &event);

        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                state.resize(size.width, size.height);
                return;
            }
            WindowEvent::RedrawRequested => {
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.window.inner_size();
                        state.resize(size.width, size.height);
                    }
                    Err(e) => {
                        log::error!("Unable to render {}", e);
                    }
                }
                return;
            }
            _ => {}
        }

        // Handle Tab for editor mouse toggle BEFORE egui consumes it
        if let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Tab),
                    state: key_state,
                    ..
                },
            ..
        } = &event
        {
            if key_state.is_pressed() && state.ui_screen == UiScreen::Editor {
                state.grab_mouse(!state.mouse_grabbed);
                return;
            }
        }

        if egui_response.consumed {
            return;
        }

        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => state.handle_key(event_loop, code, key_state.is_pressed()),
            WindowEvent::MouseInput {
                button,
                state: button_state,
                ..
            } => state.handle_mouse_button(button, button_state.is_pressed()),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(state) = &mut self.state {
            if let DeviceEvent::MouseMotion { delta } = event {
                state.handle_mouse_motion(delta);
            }
        }
    }
}

pub fn run() -> Result<(), winit::error::EventLoopError> {
    env_logger::init();
    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc;

use rayon::prelude::*;

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
    CameraResources, DepthBuffer, LightingParams, MeshResources, PaletteResources, RenderContext,
    TextureResources,
};
use ui::{BrushShape, DebugInfo, EditorState, EguiRenderer, GameSettings, LoadingState, UiMessage, UiScreen};
use voxel::{
    AIR, Chunk, generate_chunk_mesh, generate_octree_lod_mesh, raycast,
};
use world::{
    ChunkManager, ChunkPosition, ChunkStreamer, LodNodeKey, SaveFormat, StreamingConfig,
    VoxelOctree, load_big_world_fast, save_big_world_fast,
};

/// Unified mesh key for both chunk meshes and LOD node meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MeshKey {
    Chunk(ChunkPosition),
    LodNode(LodNodeKey),
}

/// Result from background world generation thread
struct BigWorldGenResult {
    world: ChunkManager,
    octree: VoxelOctree,
    mesh_tasks: Vec<MeshKey>,
    spawn_pos: Vec3,
    lod0_count: usize,
}

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
    sprint: bool,
}

pub struct AppState {
    window: Arc<Window>,

    // Rendering
    render_ctx: RenderContext,
    depth_buffer: DepthBuffer,
    camera_resources: CameraResources,
    palette_resources: PaletteResources,
    texture_resources: TextureResources,
    chunk_meshes: HashMap<MeshKey, MeshResources>,
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
    is_walking: bool,
    free_cam: bool,

    // UI
    egui: EguiRenderer,
    ui_screen: UiScreen,
    game_settings: GameSettings,
    prev_screen: UiScreen,

    // Editor
    editor_state: EditorState,
    available_levels: Vec<String>,

    // Chunk loading queue (legacy)
    pending_chunks: VecDeque<ChunkPosition>,
    pending_set: HashSet<ChunkPosition>,

    // Streaming system for large worlds
    chunk_streamer: Option<ChunkStreamer>,
    octree: Option<VoxelOctree>,
    use_streaming: bool,

    // Loading screen
    loading_state: LoadingState,
    big_world_lod_tasks: Option<Vec<MeshKey>>,
    big_world_gen_receiver: Option<mpsc::Receiver<BigWorldGenResult>>,

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
        let player = Player::new(Vec3::new(64.0, 12.0, 64.0));
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let camera = Camera::new(player.eye_position(), aspect);

        // Create resources
        let camera_resources = CameraResources::new(&render_ctx.device, &camera);
        let palette_resources = PaletteResources::new(&render_ctx.device);
        let texture_resources = TextureResources::new(&render_ctx.device, &render_ctx.queue);
        let depth_buffer = DepthBuffer::new(&render_ctx.device, size.width, size.height);

        // Create world with 8x8 chunks platform (256x256 voxels)
        // Spawn at center: 128 voxels * VOXEL_SCALE = 8.0 world units
        let mut world = ChunkManager::with_metadata("Default World", [8.0, 1.25, 8.0]);
        for cx in 0..8 {
            for cz in 0..8 {
                let mut chunk = Chunk::new();
                chunk.fill_ground(1, 42); // Stone gray color
                world.insert_chunk(ChunkPosition::new(cx, 0, cz), chunk);
            }
        }

        // Generate meshes for all chunks
        let mut chunk_meshes = HashMap::new();
        for chunk_pos in world.chunk_positions().cloned().collect::<Vec<_>>() {
            let vertices = generate_chunk_mesh(&world, chunk_pos);
            if !vertices.is_empty() {
                chunk_meshes.insert(
                    MeshKey::Chunk(chunk_pos),
                    MeshResources::new(&render_ctx.device, &vertices),
                );
            }
        }
        log::info!("[World] Generated {} chunk meshes", chunk_meshes.len());
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
                log::info!(
                    "[FPS] Loaded: {} meshes, {} animations, {} textures",
                    fps_model.meshes.len(),
                    fps_model.animations.len(),
                    fps_model.textures.len()
                );

                // Debug mesh info
                for mesh in &fps_model.meshes {
                    log::info!(
                        "[FPS] Mesh '{}': texture_index={:?}, vertices={}",
                        mesh.name,
                        mesh.texture_index,
                        mesh.vertices.len()
                    );
                    if let Some(v) = mesh.vertices.first() {
                        log::info!("[FPS]   First vertex UV: {:?}", v.uv);
                    }
                }

                // Upload textures to GPU
                for (i, texture) in fps_model.textures.iter().enumerate() {
                    // Convert to RGBA if needed
                    let rgba_data = texture.to_rgba();
                    character_manager.upload_texture(
                        &render_ctx.device,
                        &render_ctx.queue,
                        i as u32,
                        &rgba_data,
                        texture.width,
                        texture.height,
                    );
                    log::info!(
                        "[FPS] Uploaded texture {}: {}x{}",
                        i,
                        texture.width,
                        texture.height
                    );
                }

                character_manager
                    .first_person
                    .set_model(Arc::new(fps_model));
            }
            Err(e) => {
                log::info!("[FPS] Failed to load: {}", e);
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
            &palette_resources.bind_group_layout,
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
            palette_resources,
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
            is_walking: false,
            free_cam: false,
            egui,
            ui_screen: UiScreen::default(),
            game_settings: GameSettings::default(),
            prev_screen: UiScreen::MainMenu,
            editor_state: EditorState::default(),
            available_levels,
            pending_chunks: VecDeque::new(),
            pending_set: HashSet::new(),
            chunk_streamer: None,
            octree: None,
            use_streaming: false,
            loading_state: LoadingState::default(),
            big_world_lod_tasks: None,
            big_world_gen_receiver: None,
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

    /// Maximum number of chunks to rebuild per frame for smooth loading
    const CHUNKS_PER_FRAME: usize = 4;

    fn rebuild_dirty_meshes(&mut self) {
        // Add new dirty chunks to the pending queue
        let dirty = self.world.take_dirty_chunks();
        for chunk_pos in dirty {
            if !self.pending_set.contains(&chunk_pos) {
                self.pending_set.insert(chunk_pos);
                self.pending_chunks.push_back(chunk_pos);
            }
        }

        if self.pending_chunks.is_empty() {
            return;
        }

        // Sort pending chunks by distance from player (closer = higher priority)
        let player_chunk = ChunkPosition::from_world_pos(
            self.player.position.x,
            self.player.position.y,
            self.player.position.z,
        );

        // Convert to vec, sort by distance, convert back to deque
        let mut chunks_vec: Vec<_> = self.pending_chunks.drain(..).collect();
        chunks_vec.sort_by_key(|pos| {
            let dx = pos.x - player_chunk.x;
            let dy = pos.y - player_chunk.y;
            let dz = pos.z - player_chunk.z;
            dx * dx + dy * dy + dz * dz
        });
        self.pending_chunks = chunks_vec.into();

        // Process limited number of chunks this frame
        let num_to_process = self.pending_chunks.len().min(Self::CHUNKS_PER_FRAME);
        let mut processed = HashSet::new();

        for _ in 0..num_to_process {
            if let Some(chunk_pos) = self.pending_chunks.pop_front() {
                self.pending_set.remove(&chunk_pos);

                let key = MeshKey::Chunk(chunk_pos);
                if self.world.get_chunk(chunk_pos).is_some() {
                    let vertices = generate_chunk_mesh(&self.world, chunk_pos);
                    if vertices.is_empty() {
                        self.chunk_meshes.remove(&key);
                    } else if let Some(mesh) = self.chunk_meshes.get_mut(&key) {
                        mesh.update(&self.render_ctx.device, &vertices);
                    } else {
                        self.chunk_meshes.insert(
                            key,
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                } else {
                    self.chunk_meshes.remove(&key);
                }
                processed.insert(chunk_pos);
            }
        }

        // Update path tracer only for processed chunks
        if !processed.is_empty() {
            if self.path_tracer.needs_resize(&self.world) {
                self.path_tracer.update_world_voxels(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    &self.world,
                );
            } else {
                self.path_tracer
                    .update_chunks(&self.render_ctx.queue, &self.world, &processed);
            }
            self.path_tracer.reset_accumulation();
        }
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

        // Movement input
        const WALK_SPEED: f32 = 45.0;
        const SPRINT_SPEED: f32 = 90.0;
        const FREECAM_SPEED: f32 = 60.0;
        const FREECAM_FAST_SPEED: f32 = 200.0;

        let move_speed = if self.input.sprint {
            if self.free_cam { FREECAM_FAST_SPEED } else { SPRINT_SPEED }
        } else {
            if self.free_cam { FREECAM_SPEED } else { WALK_SPEED }
        };

        if self.free_cam {
            // Free cam: fly camera freely, player stays put
            let forward = self.camera.forward();
            let right = self.camera.right();
            let mut move_dir = Vec3::ZERO;
            if self.input.forward { move_dir += forward; }
            if self.input.backward { move_dir -= forward; }
            if self.input.right { move_dir += right; }
            if self.input.left { move_dir -= right; }
            if self.input.jump { move_dir += Vec3::Y; }
            if self.input.down { move_dir -= Vec3::Y; }

            if move_dir.length_squared() > 0.0 {
                move_dir = move_dir.normalize();
            }
            self.camera.position += move_dir * move_speed * dt;
            self.is_walking = false;
        } else {
            // Normal mode: player movement with physics
            let input = Vec3::new(
                (self.input.right as i32 - self.input.left as i32) as f32,
                0.0,
                (self.input.forward as i32 - self.input.backward as i32) as f32,
            );

            let forward = self.camera.forward().with_y(0.0).normalize_or_zero();
            let right = self.camera.right();
            self.player
                .apply_movement(forward, right, input, move_speed * dt);

            if self.input.jump {
                self.player.jump();
            }

            self.player.update(&self.world, dt);
            self.camera.position = self.player.eye_position();
            self.is_walking = input.length_squared() > 0.0;
        }

        self.camera.fov = self.game_settings.fov.to_radians();

        self.lighting.update_time(0.1);

        // Build camera rotation for first-person view (yaw + pitch)
        let yaw_quat = glam::Quat::from_rotation_y(-self.camera.yaw - std::f32::consts::FRAC_PI_2);
        let pitch_quat = glam::Quat::from_rotation_x(self.camera.pitch);
        let camera_rotation = yaw_quat * pitch_quat;

        // Update character manager (builds BVH for path tracing) — skip in free cam
        if !self.free_cam {
            let is_sprinting = self.input.sprint && self.is_walking;
            self.character_manager.update(
                &self.render_ctx.device,
                &self.render_ctx.queue,
                dt,
                self.is_walking,
                is_sprinting,
                self.camera.position,
                camera_rotation,
            );

            // Apply camera shake from first-person animation
            let (shake_pos, shake_rot) = self.character_manager.first_person.get_camera_shake();
            self.camera
                .apply_shake(camera_rotation * shake_pos, shake_rot);
        } else {
            self.camera.apply_shake(Vec3::ZERO, glam::Quat::IDENTITY);
        }

        // Update camera uniform AFTER applying shake
        self.camera_resources
            .update(&self.render_ctx.queue, &self.camera);

        // Update streaming system if enabled
        if self.use_streaming {
            self.update_streaming();
        }

        // Rebuild dirty meshes from voxel edits (works in both streaming and non-streaming modes)
        if !self.world.dirty_chunks().is_empty() {
            self.rebuild_dirty_meshes();
        }
    }

    /// Update streaming system for large worlds
    fn update_streaming(&mut self) {
        let Some(streamer) = &mut self.chunk_streamer else {
            return;
        };

        // Get streaming update
        let update = streamer.update(self.player.position);

        let has_mesh_requests = !update.mesh_requests.is_empty();
        let has_unload_requests = !update.unload_requests.is_empty();


        // Collect positions for path tracer update before consuming vectors
        let dirty_positions: HashSet<_> = update.mesh_requests.iter().map(|(p, _)| *p).collect();

        // Process mesh requests - generate chunks on demand and build meshes
        for (pos, _lod) in update.mesh_requests {
            // Generate chunk data if not in world
            let was_missing = self.world.get_chunk(pos).is_none();
            if was_missing {
                let chunk = self.generate_chunk_data(pos);
                self.world.insert_chunk(pos, chunk);

                // Only insert into octree for freshly generated chunks
                // (pre-existing chunks were already inserted during world generation)
                if let Some(ref mut octree) = self.octree {
                    octree.insert_chunk(pos, world::lod::VoxelData::from_full(
                        *self.world.get_chunk(pos).unwrap().data(),
                    ));
                }
            }

            // Generate mesh
            let vertices = generate_chunk_mesh(&self.world, pos);
            let key = MeshKey::Chunk(pos);
            if !vertices.is_empty() {
                if let Some(mesh) = self.chunk_meshes.get_mut(&key) {
                    mesh.update(&self.render_ctx.device, &vertices);
                } else {
                    self.chunk_meshes
                        .insert(key, MeshResources::new(&self.render_ctx.device, &vertices));
                }
            }

            // Mark mesh as built
            if let Some(s) = &mut self.chunk_streamer {
                s.mark_mesh_built(pos);
            }
        }

        // Process unload requests — only remove mesh, keep world data for collision
        for pos in update.unload_requests {
            self.chunk_meshes.remove(&MeshKey::Chunk(pos));
        }

        // Process dirty LOD nodes — rebuild octree LOD data, but only generate
        // meshes for nodes the streamer is actively tracking (LOD 1-3).
        // Without this filter, process_dirty_lods regenerates ALL parent nodes
        // up to the root (LOD 4-7), creating huge untracked meshes that overlap
        // everything and never get unloaded.
        if let Some(ref mut octree) = self.octree {
            let regenerated = octree.process_dirty_lods();
            for key in regenerated {
                // Only regenerate meshes for LOD nodes the streamer tracks
                let is_tracked = self.chunk_streamer.as_ref()
                    .map_or(false, |s| s.is_lod_loaded(&key));
                if !is_tracked {
                    continue;
                }

                if let Some(data) = octree.get_node_lod_data(&key) {
                    let vertices = generate_octree_lod_mesh(data, &key);
                    let mesh_key = MeshKey::LodNode(key);
                    if !vertices.is_empty() {
                        if let Some(mesh) = self.chunk_meshes.get_mut(&mesh_key) {
                            mesh.update(&self.render_ctx.device, &vertices);
                        } else {
                            self.chunk_meshes.insert(
                                mesh_key,
                                MeshResources::new(&self.render_ctx.device, &vertices),
                            );
                        }
                    } else {
                        self.chunk_meshes.remove(&mesh_key);
                    }
                } else if let Some(v) = octree.get_node_homogeneous(&key) {
                    let mesh_key = MeshKey::LodNode(key);
                    if v != AIR {
                        let data = world::lod::VoxelData::Homogeneous(v);
                        let vertices = generate_octree_lod_mesh(&data, &key);
                        if !vertices.is_empty() {
                            self.chunk_meshes.insert(
                                mesh_key,
                                MeshResources::new(&self.render_ctx.device, &vertices),
                            );
                        }
                    } else {
                        self.chunk_meshes.remove(&mesh_key);
                    }
                }
            }
        }

        // Process LOD node mesh requests from streamer
        if let Some(ref octree) = self.octree {
            for key in update.lod_mesh_requests {
                let vertices = if let Some(data) = octree.get_node_lod_data(&key) {
                    generate_octree_lod_mesh(data, &key)
                } else if let Some(v) = octree.get_node_homogeneous(&key) {
                    if v != voxel::AIR {
                        let data = world::lod::VoxelData::Homogeneous(v);
                        generate_octree_lod_mesh(&data, &key)
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                if !vertices.is_empty() {
                    self.chunk_meshes.insert(
                        MeshKey::LodNode(key),
                        MeshResources::new(&self.render_ctx.device, &vertices),
                    );
                }
                if let Some(s) = &mut self.chunk_streamer {
                    s.mark_lod_mesh_built(&key);
                }
            }
        }

        // Process LOD node unload requests
        for key in update.lod_unload_requests {
            self.chunk_meshes.remove(&MeshKey::LodNode(key));
        }

        // Update path tracer if chunks changed
        if has_mesh_requests || has_unload_requests {
            if !dirty_positions.is_empty() {
                if self.path_tracer.needs_resize(&self.world) {
                    self.path_tracer.update_world_voxels(
                        &self.render_ctx.device,
                        &self.render_ctx.queue,
                        &self.world,
                    );
                } else {
                    self.path_tracer.update_chunks(
                        &self.render_ctx.queue,
                        &self.world,
                        &dirty_positions,
                    );
                }
                self.path_tracer.reset_accumulation();
            }
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        match self.ui_screen {
            UiScreen::InGame => self.update(),
            UiScreen::Editor => self.update_editor(),
            UiScreen::Loading => self.update_loading(),
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
            UiScreen::PauseMenu => {
                let is_big_world = self.editor_state.level_name == "big_world"
                    || self.editor_state.level_name == "bigworld";
                ui::pause_menu(&self.egui.ctx, is_big_world)
            }
            UiScreen::InGame => {
                let debug = if self.game_settings.show_debug {
                    Some(self.build_debug_info())
                } else {
                    None
                };
                ui::hud(&self.egui.ctx, self.fps, debug.as_ref());
                None
            }
            UiScreen::LevelSelect => ui::level_select(&self.egui.ctx, &self.available_levels),
            UiScreen::Editor => ui::editor_hud(&self.egui.ctx, &mut self.editor_state, self.fps),
            UiScreen::EditorPause => ui::editor_pause(&self.egui.ctx),
            UiScreen::SaveDialog => {
                ui::save_dialog(&self.egui.ctx, &mut self.editor_state.level_name)
            }
            UiScreen::Loading => {
                ui::loading_screen(&self.egui.ctx, &self.loading_state);
                None
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
                self.is_walking,
            );

            let show_debug = self.game_settings.show_debug;
            let meshes = self
                .chunk_meshes
                .iter()
                .map(move |(key, m)| {
                    let lod = if show_debug {
                        match key {
                            MeshKey::Chunk(_) => 0u32,
                            MeshKey::LodNode(k) => k.lod_level as u32,
                        }
                    } else {
                        0u32
                    };
                    (&m.vertex_buffer, m.num_vertices, lod)
                });

            self.path_tracer.render(
                &mut encoder,
                &view,
                &self.depth_buffer.view,
                &self.camera_resources.bind_group,
                &self.palette_resources.bind_group,
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
            UiMessage::GenerateBigWorld => {
                self.start_big_world_generation();
            }
            UiMessage::SaveBigWorld => {
                self.save_big_world_to_file();
            }
            UiMessage::LoadBigWorld => {
                self.load_big_world_from_file();
            }
        }
    }

    fn create_new_level(&mut self) {
        // Clear pending chunks queue
        self.pending_chunks.clear();
        self.pending_set.clear();

        // Disable streaming for normal levels
        self.use_streaming = false;
        self.chunk_streamer = None;
        self.octree = None;

        // Create world with 8x8 chunks platform (256x256 voxels)
        // Spawn at center: 128 voxels * VOXEL_SCALE = 8.0 world units
        self.world = ChunkManager::with_metadata("New Level", [8.0, 1.25, 8.0]);
        for cx in 0..8 {
            for cz in 0..8 {
                let mut chunk = Chunk::new();
                chunk.fill_ground(1, 42); // Stone gray color
                self.world
                    .insert_chunk(ChunkPosition::new(cx, 0, cz), chunk);
            }
        }

        // Reset player position to center of platform
        self.player.position = Vec3::new(8.0, 0.3, 8.0);
        self.player.velocity = Vec3::ZERO;
        self.camera.position = self.player.eye_position();

        // Rebuild meshes
        self.chunk_meshes.clear();
        for chunk_pos in self.world.chunk_positions().cloned().collect::<Vec<_>>() {
            let vertices = generate_chunk_mesh(&self.world, chunk_pos);
            if !vertices.is_empty() {
                self.chunk_meshes.insert(
                    MeshKey::Chunk(chunk_pos),
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

    /// Start generating a big world with loading screen
    fn start_big_world_generation(&mut self) {
        // Clear pending chunks queue
        self.pending_chunks.clear();
        self.pending_set.clear();

        // Disable streaming and octree (will be set up after loading)
        self.octree = None;
        self.chunk_streamer = None;
        self.use_streaming = false;

        // Clear existing meshes and world
        self.chunk_meshes.clear();
        self.world = ChunkManager::new();

        self.editor_state.level_name = "big_world".to_string();

        // Setup loading state and switch to loading screen immediately
        self.loading_state = LoadingState::new("Generating world...", 100);
        self.ui_screen = UiScreen::Loading;
        self.grab_mouse(false);

        // Spawn background thread for heavy generation work
        let (tx, rx) = mpsc::channel();
        self.big_world_gen_receiver = Some(rx);

        std::thread::spawn(move || {
            Self::generate_big_world_background(tx);
        });

        log::info!("[BigWorld] Background generation thread started");
    }

    /// Heavy world generation work that runs on a background thread
    fn generate_big_world_background(tx: mpsc::Sender<BigWorldGenResult>) {
        const WORLD_SIZE_CHUNKS: i32 = 128;
        const WORLD_SIZE_METERS: u32 = 256;

        // Spawn at center, compute terrain height for spawn Y
        let center = (WORLD_SIZE_CHUNKS as f32 * 32.0 * voxel::VOXEL_SCALE) / 2.0;
        let center_voxel_x = (center / voxel::VOXEL_SCALE) as i32;
        let center_voxel_z = (center / voxel::VOXEL_SCALE) as i32;
        let spawn_terrain_height = Self::terrain_height(center_voxel_x, center_voxel_z);
        let spawn_y = spawn_terrain_height as f32 * voxel::VOXEL_SCALE + 1.0;
        let spawn_pos = Vec3::new(center, spawn_y, center);

        log::info!(
            "[BigWorld] Generating {}x{} meter world (terrain height at center: {} voxels = {:.1}m)...",
            WORLD_SIZE_METERS,
            WORLD_SIZE_METERS,
            spawn_terrain_height,
            spawn_terrain_height as f32 * voxel::VOXEL_SCALE
        );

        // Generate chunk data in parallel
        let all_positions: Vec<_> = (0..WORLD_SIZE_CHUNKS)
            .flat_map(|cx| {
                (0..Self::WORLD_HEIGHT_CHUNKS)
                    .flat_map(move |cy| {
                        (0..WORLD_SIZE_CHUNKS).map(move |cz| ChunkPosition::new(cx, cy, cz))
                    })
            })
            .collect();

        let all_chunks: Vec<_> = all_positions
            .par_iter()
            .map(|&pos| {
                let chunk = Self::generate_chunk_data_static(pos);
                (pos, chunk)
            })
            .collect();

        // Insert into world
        let mut world = ChunkManager::with_metadata("Big World", [center, spawn_y, center]);
        for (pos, chunk) in all_chunks {
            world.insert_chunk(pos, chunk);
        }

        log::info!("[BigWorld] Chunk data generated. Building octree and LOD hierarchy...");

        // Create octree and insert all chunks
        let mut octree = VoxelOctree::for_world_size_meters(WORLD_SIZE_METERS);
        for pos in &all_positions {
            if let Some(chunk) = world.get_chunk(*pos) {
                octree.insert_chunk(*pos, world::lod::VoxelData::from_full(*chunk.data()));
            }
        }

        // Build full LOD hierarchy in one pass
        let _regenerated = octree.process_dirty_lods();
        log::info!(
            "[BigWorld] Octree built: {} nodes, {} data blocks",
            octree.node_count(),
            octree.data_count()
        );

        // Build LOD-aware mesh tasks
        let lod_config = world::LodConfig::default();
        let mut mesh_tasks: Vec<MeshKey> = Vec::new();

        // LOD0: individual chunk meshes within LOD0 distance
        let lod0_distance = lod_config.distances[0];

        for pos in &all_positions {
            let chunk_center = pos.center_world_pos();
            let dist = Vec3::new(
                chunk_center.0 - spawn_pos.x,
                chunk_center.1 - spawn_pos.y,
                chunk_center.2 - spawn_pos.z,
            )
            .length();
            if dist <= lod0_distance {
                mesh_tasks.push(MeshKey::Chunk(*pos));
            }
        }

        let lod0_count = mesh_tasks.len();

        // LOD1-3: LOD node meshes for areas beyond LOD0
        for lod in 1..lod_config.levels {
            let lod_distance_min = lod_config.distances[(lod - 1) as usize];
            let lod_distance_max = lod_config.distances[lod as usize];

            let chunks_per_node = 1i32 << lod;
            let node_min = 0i32;
            let node_max = (WORLD_SIZE_CHUNKS / chunks_per_node).max(1);

            let node_y_max = (Self::WORLD_HEIGHT_CHUNKS / chunks_per_node).max(1);
            for nx in node_min..node_max {
                for nz in node_min..node_max {
                    for ny in 0..node_y_max {
                        let key = LodNodeKey::new(nx, ny, nz, lod);
                        let center = key.center_world_pos();
                        let dist = Vec3::new(
                            center.0 - spawn_pos.x,
                            center.1 - spawn_pos.y,
                            center.2 - spawn_pos.z,
                        )
                        .length();

                        if dist >= lod_distance_min && dist < lod_distance_max {
                            mesh_tasks.push(MeshKey::LodNode(key));
                        }
                    }
                }
            }
        }

        // Sort by distance from spawn (closer first)
        mesh_tasks.sort_by(|a, b| {
            let dist_a = match a {
                MeshKey::Chunk(pos) => {
                    let c = pos.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
                MeshKey::LodNode(key) => {
                    let c = key.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
            };
            let dist_b = match b {
                MeshKey::Chunk(pos) => {
                    let c = pos.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
                MeshKey::LodNode(key) => {
                    let c = key.center_world_pos();
                    Vec3::new(c.0 - spawn_pos.x, c.1 - spawn_pos.y, c.2 - spawn_pos.z).length()
                }
            };
            dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
        });

        log::info!(
            "[BigWorld] Generation complete: {} mesh tasks ({} LOD0 chunks + {} LOD nodes)",
            mesh_tasks.len(),
            lod0_count,
            mesh_tasks.len() - lod0_count,
        );

        let _ = tx.send(BigWorldGenResult {
            world,
            octree,
            mesh_tasks,
            spawn_pos,
            lod0_count,
        });
    }

    /// Update loading screen - process LOD mesh tasks incrementally
    fn update_loading(&mut self) {
        const TASKS_PER_FRAME: usize = 64;

        // Phase 1: Check if background generation thread has finished
        if let Some(ref rx) = self.big_world_gen_receiver {
            match rx.try_recv() {
                Ok(result) => {
                    // Background generation complete — transfer results
                    log::info!(
                        "[BigWorld] Background generation received. Loading {} mesh tasks ({} LOD0 + {} LOD)...",
                        result.mesh_tasks.len(),
                        result.lod0_count,
                        result.mesh_tasks.len() - result.lod0_count,
                    );
                    let total_tasks = result.mesh_tasks.len();
                    self.world = result.world;
                    self.octree = Some(result.octree);
                    self.big_world_lod_tasks = Some(result.mesh_tasks);
                    self.player.position = result.spawn_pos;
                    self.player.velocity = Vec3::ZERO;
                    self.camera.position = self.player.eye_position();
                    self.loading_state = LoadingState::new("Building meshes...", total_tasks);
                    self.big_world_gen_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still generating — keep showing loading screen
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Thread panicked or dropped sender
                    log::error!("[BigWorld] Generation thread disconnected!");
                    self.big_world_gen_receiver = None;
                    self.ui_screen = UiScreen::MainMenu;
                    return;
                }
            }
        }

        // Phase 2: Process mesh tasks
        // Take ownership of the tasks to process
        let tasks = match self.big_world_lod_tasks.take() {
            Some(tasks) => tasks,
            None => {
                // Already finished — shouldn't happen, but safe fallback
                self.finish_big_world_loading();
                return;
            }
        };

        if tasks.is_empty() {
            self.finish_big_world_loading();
            return;
        }

        // Process a batch of tasks this frame
        let to_process = tasks.len().min(TASKS_PER_FRAME);
        let (tasks_to_process, remaining): (Vec<_>, Vec<_>) = tasks
            .into_iter()
            .enumerate()
            .partition(|(i, _)| *i < to_process);

        let tasks_to_process: Vec<_> = tasks_to_process.into_iter().map(|(_, t)| t).collect();
        let remaining: Vec<_> = remaining.into_iter().map(|(_, t)| t).collect();

        // Generate meshes based on task type
        for task in &tasks_to_process {
            match task {
                MeshKey::Chunk(pos) => {
                    let vertices = generate_chunk_mesh(&self.world, *pos);
                    if !vertices.is_empty() {
                        self.chunk_meshes.insert(
                            MeshKey::Chunk(*pos),
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                }
                MeshKey::LodNode(key) => {
                    if let Some(ref octree) = self.octree {
                        let vertices = if let Some(data) = octree.get_node_lod_data(key) {
                            generate_octree_lod_mesh(data, key)
                        } else if let Some(v) = octree.get_node_homogeneous(key) {
                            if v != voxel::AIR {
                                let data = world::lod::VoxelData::Homogeneous(v);
                                generate_octree_lod_mesh(&data, key)
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        };

                        if !vertices.is_empty() {
                            self.chunk_meshes.insert(
                                MeshKey::LodNode(*key),
                                MeshResources::new(&self.render_ctx.device, &vertices),
                            );
                        }
                    }
                }
            }
        }

        // Update loading progress
        let loaded = self.loading_state.chunks_total - remaining.len();
        self.loading_state.update(loaded);

        // Put remaining tasks back, or finish loading if done
        if !remaining.is_empty() {
            self.big_world_lod_tasks = Some(remaining);
        } else {
            self.finish_big_world_loading();
        }
    }

    /// Finish big world loading: set up path tracer, streaming, and switch to game
    fn finish_big_world_loading(&mut self) {
        log::info!("[BigWorld] Initial loading complete! Setting up streaming...");

        // Update path tracer
        self.path_tracer.update_world_voxels(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &self.world,
        );
        self.path_tracer.reset_accumulation();

        // Create streaming system with world bounds
        let mut config = StreamingConfig::default();
        if let Some(ref octree) = self.octree {
            // Use octree bounds for X/Z, constrain Y to terrain range
            config.world_min = glam::IVec3::new(octree.world_min.x, 0, octree.world_min.z);
            config.world_max = glam::IVec3::new(
                octree.world_max.x + 1,      // exclusive
                Self::WORLD_HEIGHT_CHUNKS,    // Y layers for terrain
                octree.world_max.z + 1,
            );
        }
        let mut streamer = ChunkStreamer::new(config);
        streamer.set_player_position(self.player.position);

        // Preload streamer with all currently loaded meshes
        let mut loaded_chunks = Vec::new();
        let mut loaded_lod_nodes = Vec::new();
        for key in self.chunk_meshes.keys() {
            match key {
                MeshKey::Chunk(pos) => loaded_chunks.push(*pos),
                MeshKey::LodNode(key) => loaded_lod_nodes.push(*key),
            }
        }

        streamer.preload_chunks(loaded_chunks);
        streamer.preload_lod_nodes(loaded_lod_nodes);

        self.chunk_streamer = Some(streamer);
        self.use_streaming = true;

        log::info!(
            "[BigWorld] Streaming enabled! {} chunk meshes, {} LOD meshes",
            self.chunk_meshes.keys().filter(|k| matches!(k, MeshKey::Chunk(_))).count(),
            self.chunk_meshes.keys().filter(|k| matches!(k, MeshKey::LodNode(_))).count(),
        );

        self.ui_screen = UiScreen::InGame;
        self.grab_mouse(true);
    }

    /// Height of big world terrain in Y chunks
    const WORLD_HEIGHT_CHUNKS: i32 = 4;

    /// Static version of chunk data generation for parallel use
    fn generate_chunk_data_static(pos: ChunkPosition) -> Chunk {
        use voxel::CHUNK_SIZE;

        let mut chunk = Chunk::new();

        // Quick reject: chunks above max terrain height are all air
        let chunk_bottom_voxel = pos.y * CHUNK_SIZE as i32;
        let chunk_top_voxel = chunk_bottom_voxel + CHUNK_SIZE as i32;
        if chunk_bottom_voxel > 96 || pos.y < 0 {
            return chunk;
        }

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                // World voxel coordinates
                let wx = pos.x as i32 * CHUNK_SIZE as i32 + lx as i32;
                let wz = pos.z as i32 * CHUNK_SIZE as i32 + lz as i32;

                // Compute terrain height at this column
                let height = Self::terrain_height(wx, wz);

                // Skip if entire column in this chunk is above terrain
                if chunk_bottom_voxel >= height {
                    continue;
                }

                for ly in 0..CHUNK_SIZE {
                    let wy = chunk_bottom_voxel + ly as i32;

                    if wy >= height {
                        break; // Rest of column is air
                    }

                    let depth_below_surface = height - wy;
                    let voxel = if depth_below_surface <= 1 {
                        // Surface: grass (vary shade by position)
                        let grass_noise = Self::hash_2d(wx.wrapping_add(1000), wz.wrapping_add(2000));
                        if grass_noise < 0.33 { 49 } // Grass green
                        else if grass_noise < 0.66 { 50 } // Light grass
                        else { 51 } // Dark grass
                    } else if depth_below_surface <= 5 {
                        // Subsurface: dirt
                        33 // Dirt
                    } else {
                        // Deep: stone (vary shade)
                        let stone_noise = Self::hash_2d(wx.wrapping_add(3000), wz.wrapping_add(4000));
                        if stone_noise < 0.4 { 42 } // Stone gray
                        else if stone_noise < 0.7 { 43 } // Light stone
                        else { 44 } // Dark stone
                    };

                    chunk.set(lx, ly, lz, voxel);
                }
            }
        }

        chunk
    }

    /// Compute terrain height in voxels at world voxel coordinates (wx, wz)
    fn terrain_height(wx: i32, wz: i32) -> i32 {
        let x = wx as f32;
        let z = wz as f32;

        // Large rolling hills
        let h1 = Self::fbm(x * 0.0015, z * 0.0015, 3) * 48.0;
        // Medium bumps
        let h2 = Self::fbm(x * 0.006 + 100.0, z * 0.006 + 200.0, 3) * 16.0;
        // Small detail
        let h3 = Self::fbm(x * 0.025 + 300.0, z * 0.025 + 400.0, 2) * 4.0;

        // Base height + hills + bumps + detail
        // Range: ~24 to ~88 voxels (1.5m to 5.5m world height)
        let height = 40.0 + h1 + h2 + h3;
        height.max(8.0) as i32
    }

    /// Fractional Brownian Motion (layered noise)
    fn fbm(x: f32, z: f32, octaves: u32) -> f32 {
        let mut value = 0.0f32;
        let mut amplitude = 1.0f32;
        let mut frequency = 1.0f32;
        let mut max_value = 0.0f32;

        for _ in 0..octaves {
            value += Self::smooth_noise(x * frequency, z * frequency) * amplitude;
            max_value += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }

        // Returns [-1, 1] range
        (value / max_value) * 2.0 - 1.0
    }

    /// Smooth value noise with bicubic-like interpolation
    fn smooth_noise(x: f32, z: f32) -> f32 {
        let ix = x.floor() as i32;
        let iz = z.floor() as i32;
        let fx = x - x.floor();
        let fz = z - z.floor();

        // Smoothstep interpolation
        let fx = fx * fx * (3.0 - 2.0 * fx);
        let fz = fz * fz * (3.0 - 2.0 * fz);

        let v00 = Self::hash_2d(ix, iz);
        let v10 = Self::hash_2d(ix + 1, iz);
        let v01 = Self::hash_2d(ix, iz + 1);
        let v11 = Self::hash_2d(ix + 1, iz + 1);

        let a = v00 + (v10 - v00) * fx;
        let b = v01 + (v11 - v01) * fx;
        a + (b - a) * fz
    }

    /// Hash function: maps integer (x,z) to pseudo-random float in [0, 1]
    fn hash_2d(x: i32, z: i32) -> f32 {
        let n = (x.wrapping_mul(374761393) as u32)
            .wrapping_add(z.wrapping_mul(668265263) as u32);
        let n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        let n = n ^ (n >> 16);
        (n & 0x7FFFFFFF) as f32 / 0x7FFFFFFF as f32
    }

    /// Generate chunk data on-demand for streaming
    fn generate_chunk_data(&self, pos: ChunkPosition) -> Chunk {
        Self::generate_chunk_data_static(pos)
    }

    fn load_level(&mut self, name: &str) -> bool {
        // Clear pending chunks queue
        self.pending_chunks.clear();
        self.pending_set.clear();

        // Disable streaming for loaded levels
        self.use_streaming = false;
        self.chunk_streamer = None;
        self.octree = None;

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
                            MeshKey::Chunk(chunk_pos),
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

    fn save_big_world_to_file(&mut self) {
        // Create maps directory
        if let Err(e) = std::fs::create_dir_all("maps") {
            log::error!("Failed to create maps directory: {}", e);
            return;
        }

        let path = "maps/bigworld.world";
        let spawn = self.player.position.to_array();

        log::info!("[BigWorld] Saving to {}...", path);

        match save_big_world_fast(path, &self.world, 8, spawn) {
            Ok(()) => {
                log::info!("[BigWorld] Saved successfully to {}", path);
            }
            Err(e) => {
                log::error!("[BigWorld] Failed to save: {}", e);
            }
        }
    }

    fn load_big_world_from_file(&mut self) {
        let path = "maps/bigworld.world";

        if !std::path::Path::new(path).exists() {
            log::error!("[BigWorld] File not found: {}", path);
            return;
        }

        // Show loading screen
        self.loading_state = LoadingState::new("Loading big world...", 100);
        self.ui_screen = UiScreen::Loading;

        log::info!("[BigWorld] Loading from {}...", path);

        match load_big_world_fast(path) {
            Ok(loaded) => {
                log::info!(
                    "[BigWorld] Loaded {} meshes, spawn at {:?}",
                    loaded.meshes.len(),
                    loaded.spawn_position
                );

                // Clear existing state
                self.chunk_meshes.clear();
                self.pending_chunks.clear();
                self.pending_set.clear();
                self.use_streaming = false;
                self.chunk_streamer = None;
                self.octree = None;

                // Store loaded world
                self.world = loaded.world;

                // Upload meshes to GPU
                for (pos, vertices) in loaded.meshes {
                    if !vertices.is_empty() {
                        self.chunk_meshes.insert(
                            MeshKey::Chunk(pos),
                            MeshResources::new(&self.render_ctx.device, &vertices),
                        );
                    }
                }

                // Set player position
                self.player.position = Vec3::from_array(loaded.spawn_position);
                self.player.velocity = Vec3::ZERO;
                self.camera.position = self.player.eye_position();

                // Reset path tracer (skip voxel update for big world)
                self.path_tracer.reset_accumulation();

                self.editor_state.level_name = "bigworld".to_string();

                log::info!(
                    "[BigWorld] Ready! {} meshes loaded",
                    self.chunk_meshes.len()
                );

                self.ui_screen = UiScreen::InGame;
                self.grab_mouse(true);
            }
            Err(e) => {
                log::error!("[BigWorld] Failed to load: {}", e);
                self.ui_screen = UiScreen::MainMenu;
            }
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
                        log::info!("[FreeCam] Enabled — LOD loads around player, camera flies free");
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
                    self.world.set_voxels_batch(&batch);
                }
            }
            _ => {}
        }
    }

    /// Generate voxel positions for the given brush shape and size
    fn get_brush_positions(
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

    fn build_debug_info(&self) -> DebugInfo {
        let player_chunk = ChunkPosition::from_world_pos(
            self.player.position.x,
            self.player.position.y,
            self.player.position.z,
        );

        let mut chunk_meshes = 0usize;
        let mut lod_meshes = 0usize;
        for key in self.chunk_meshes.keys() {
            match key {
                MeshKey::Chunk(_) => chunk_meshes += 1,
                MeshKey::LodNode(_) => lod_meshes += 1,
            }
        }

        DebugInfo {
            player_pos: self.player.position.to_array(),
            player_chunk: [player_chunk.x, player_chunk.y, player_chunk.z],
            camera_pos: if self.free_cam { Some(self.camera.position.to_array()) } else { None },
            total_meshes: self.chunk_meshes.len(),
            chunk_meshes,
            lod_meshes,
            streaming_active: self.use_streaming,
            streaming_loaded: self.chunk_streamer.as_ref().map_or(0, |s| s.loaded_count()),
            streaming_queue: self.chunk_streamer.as_ref().map_or(0, |s| s.queue_size()),
            streaming_lod_nodes: self.chunk_streamer.as_ref().map_or(0, |s| s.loaded_lod_count()),
            octree_active: self.octree.is_some(),
            octree_nodes: self.octree.as_ref().map_or(0, |o| o.node_count()),
            octree_data_blocks: self.octree.as_ref().map_or(0, |o| o.data_count()),
            octree_depth: self.octree.as_ref().map_or(0, |o| o.depth()),
            world_chunks: self.world.chunk_count(),
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

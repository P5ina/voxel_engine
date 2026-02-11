use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc;

use glam::Vec3;
use winit::window::Window;

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

mod app;
mod dev_tools;
mod editor;
mod input;
mod map_browser;
mod save_load;
mod streaming_system;
mod terrain_gen;
mod world_gen;

pub use app::{App, run};

use camera::Camera;
use model::load_glb;
use pathtracer::PathTracer;
use player::Player;
use renderer::{CameraResources, LightingParams, MeshResources, PaletteResources, RenderContext};
use ui::LoadingState;
#[cfg(feature = "dev-tools")]
use ui::WorldSelectState;
use ui::{EditorState, EguiRenderer, GameSettings, MapSelectState, UiMessage, UiScreen};
use voxel::{Chunk, VOXEL_SCALE, generate_chunk_mesh};
use world::{ChunkManager, ChunkPosition, ChunkStreamer, LodNodeKey, RegionManager, VoxelOctree};

/// Result sent back from background save thread
pub(crate) type SaveWorldResult = (Option<VoxelOctree>, Result<(), world::BigWorldError>);

/// Unified mesh key for both chunk meshes and LOD node meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MeshKey {
    Chunk(ChunkPosition),
    LodNode(LodNodeKey),
}

/// Messages from background world generation thread
pub(crate) enum BigWorldGenMessage {
    /// Progress update: (message, loaded, total)
    Progress(String, usize, usize),
    /// Generation complete
    Done(Box<BigWorldGenResult>),
}

/// Result from background world generation thread
pub(crate) struct BigWorldGenResult {
    pub(crate) world: ChunkManager,
    pub(crate) octree: VoxelOctree,
    pub(crate) mesh_tasks: Vec<MeshKey>,
    pub(crate) spawn_pos: Vec3,
    pub(crate) lod0_count: usize,
}

/// Result from background streaming mesh generation
pub(crate) struct StreamingMeshResult {
    pub(crate) pos: ChunkPosition,
    pub(crate) vertices: Vec<Vertex>,
}

/// Result from background LOD mesh generation
pub(crate) struct LodMeshResult {
    pub(crate) key: LodNodeKey,
    pub(crate) vertices: Vec<Vertex>,
}

pub(crate) use renderer::Vertex;

#[derive(Default)]
pub(crate) struct InputState {
    pub(crate) forward: bool,
    pub(crate) backward: bool,
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) jump: bool,
    pub(crate) up: bool,
    pub(crate) down: bool,
    pub(crate) ctrl: bool,
    pub(crate) sprint: bool,
}

pub struct AppState {
    pub(crate) window: Arc<Window>,

    // Rendering
    pub(crate) render_ctx: RenderContext,
    pub(crate) camera_resources: CameraResources,
    pub(crate) palette_resources: PaletteResources,
    pub(crate) chunk_meshes: HashMap<MeshKey, MeshResources>,
    pub(crate) lighting: LightingParams,
    pub(crate) path_tracer: PathTracer,
    pub(crate) character_manager: character::CharacterManager,

    // Game state
    pub(crate) camera: Camera,
    pub(crate) player: Player,
    pub(crate) world: ChunkManager,

    // Input
    pub(crate) input: InputState,
    pub(crate) mouse_grabbed: bool,
    pub(crate) is_walking: bool,
    pub(crate) free_cam: bool,
    /// Player yaw/pitch frozen when entering free cam (for frustum culling visualization)
    pub(crate) player_yaw: f32,
    pub(crate) player_pitch: f32,

    // UI
    pub(crate) egui: EguiRenderer,
    pub(crate) ui_screen: UiScreen,
    pub(crate) game_settings: GameSettings,
    pub(crate) prev_screen: UiScreen,
    pub(crate) map_select_state: MapSelectState,
    pub(crate) selected_map_path: Option<String>,

    // Editor
    pub(crate) editor_state: EditorState,
    #[cfg(feature = "dev-tools")]
    pub(crate) enter_editor_after_gen: bool,

    // World select
    #[cfg(feature = "dev-tools")]
    pub(crate) world_select_state: WorldSelectState,

    // Chunk loading queue
    pub(crate) pending_chunks: VecDeque<ChunkPosition>,
    pub(crate) pending_set: HashSet<ChunkPosition>,

    // Streaming system for large worlds
    pub(crate) chunk_streamer: Option<ChunkStreamer>,
    pub(crate) octree: Option<VoxelOctree>,
    pub(crate) use_streaming: bool,
    pub(crate) region_manager: Option<RegionManager>,

    // Background streaming mesh generation
    pub(crate) streaming_mesh_rx: Option<mpsc::Receiver<StreamingMeshResult>>,
    pub(crate) streaming_mesh_tx: Option<mpsc::Sender<StreamingMeshResult>>,
    pub(crate) streaming_inflight: HashSet<ChunkPosition>,

    // Background LOD mesh generation
    pub(crate) lod_mesh_rx: Option<mpsc::Receiver<LodMeshResult>>,
    pub(crate) lod_mesh_tx: Option<mpsc::Sender<LodMeshResult>>,

    /// False until nearby chunks around the player are meshed (prevents fall-through)
    pub(crate) chunks_ready: bool,

    // Loading screen
    pub(crate) loading_state: LoadingState,
    pub(crate) big_world_lod_tasks: Option<Vec<MeshKey>>,
    pub(crate) big_world_gen_receiver: Option<mpsc::Receiver<BigWorldGenMessage>>,
    pub(crate) save_world_receiver: Option<mpsc::Receiver<SaveWorldResult>>,

    // Timing
    pub(crate) last_frame: std::time::Instant,
    pub(crate) fps: f32,
    pub(crate) frame_time_accum: f32,
    pub(crate) frame_count: u32,
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
        for chunk_pos in world.chunk_positions().collect::<Vec<_>>() {
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
        let path_tracer = PathTracer::new(
            &render_ctx.device,
            &render_ctx.queue,
            size.width,
            size.height,
            render_ctx.format(),
            &camera_resources.bind_group_layout,
            &palette_resources.bind_group_layout,
            character_manager.bind_group_layout(),
        );
        Ok(Self {
            window,
            render_ctx,
            camera_resources,
            palette_resources,
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
            player_yaw: 0.0,
            player_pitch: 0.0,
            egui,
            ui_screen: UiScreen::default(),
            game_settings: GameSettings::default(),
            prev_screen: UiScreen::MainMenu,
            map_select_state: MapSelectState::default(),
            selected_map_path: None,
            editor_state: EditorState::default(),
            #[cfg(feature = "dev-tools")]
            enter_editor_after_gen: false,
            #[cfg(feature = "dev-tools")]
            world_select_state: WorldSelectState::default(),
            pending_chunks: VecDeque::new(),
            pending_set: HashSet::new(),
            chunk_streamer: None,
            octree: None,
            use_streaming: false,
            region_manager: None,
            streaming_mesh_rx: None,
            streaming_mesh_tx: None,
            streaming_inflight: HashSet::new(),
            lod_mesh_rx: None,
            lod_mesh_tx: None,
            chunks_ready: true, // default world is always fully loaded
            loading_state: LoadingState::default(),
            big_world_lod_tasks: None,
            big_world_gen_receiver: None,
            save_world_receiver: None,
            last_frame: std::time::Instant::now(),
            fps: 0.0,
            frame_time_accum: 0.0,
            frame_count: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.render_ctx.resize(width, height);
            self.camera.resize(width, height);
            self.path_tracer
                .resize(&self.render_ctx.device, width, height);
        }
    }

    /// Maximum number of chunks to rebuild per frame for smooth loading
    pub(crate) const CHUNKS_PER_FRAME: usize = 8;

    /// Maximum number of inflight background mesh tasks to prevent memory explosion
    pub(crate) const MAX_INFLIGHT: usize = 128;

    /// Conservative terrain occlusion check for far chunks.
    /// Returns true when a terrain ridge is higher than the line from camera
    /// to the top of the chunk AABB, so the chunk is very likely hidden.
    fn occluded_by_terrain_heightfield(camera_pos: Vec3, min: Vec3, max: Vec3) -> bool {
        let target_center = (min + max) * 0.5;
        let to_target = target_center - camera_pos;
        let dist_xz = glam::Vec2::new(to_target.x, to_target.z).length();

        // Never cull nearby chunks to avoid popping in close range.
        if dist_xz < 96.0 {
            return false;
        }

        // Trace to chunk top for conservative all-hidden test.
        let target_y = max.y;
        let steps = ((dist_xz / 16.0).ceil() as i32).clamp(12, 160);
        let height_margin = 2.0;

        for i in 1..steps {
            let t = i as f32 / steps as f32;
            let sx = camera_pos.x + to_target.x * t;
            let sz = camera_pos.z + to_target.z * t;
            let ray_top_y = camera_pos.y + (target_y - camera_pos.y) * t;

            let vx = (sx / VOXEL_SCALE).floor() as i32;
            let vz = (sz / VOXEL_SCALE).floor() as i32;
            let terrain_y = Self::terrain_height(vx, vz) as f32 * VOXEL_SCALE;

            if terrain_y > ray_top_y + height_margin {
                return true;
            }
        }

        false
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
            if self.free_cam {
                FREECAM_FAST_SPEED
            } else {
                SPRINT_SPEED
            }
        } else if self.free_cam {
            FREECAM_SPEED
        } else {
            WALK_SPEED
        };

        if self.free_cam {
            // Free cam: fly camera freely, player stays put
            let forward = self.camera.forward();
            let right = self.camera.right();
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
            if self.input.jump {
                move_dir += Vec3::Y;
            }
            if self.input.down {
                move_dir -= Vec3::Y;
            }

            if move_dir.length_squared() > 0.0 {
                move_dir = move_dir.normalize();
            }
            self.camera.position += move_dir * move_speed * dt;
            self.is_walking = false;
        } else if !self.chunks_ready && self.use_streaming {
            // Freeze player until nearby chunks are loaded (prevents fall-through)
            self.camera.position = self.player.eye_position();
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
        if !self.world.dirty_chunks().is_empty() || !self.pending_chunks.is_empty() {
            self.rebuild_dirty_meshes();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.ui_screen == UiScreen::InGame {
            self.update();
        } else if !self.update_dev_screen() {
            self.last_frame = std::time::Instant::now();
        }
        self.window.request_redraw();

        if !self.render_ctx.is_configured {
            return Ok(());
        }

        // Begin egui frame
        self.egui.begin_frame(&self.window);

        // Build UI
        let msg = if let Some(msg) = self.build_dev_ui() {
            msg
        } else {
            match self.ui_screen {
                UiScreen::MainMenu => ui::main_menu(&self.egui.ctx),
                UiScreen::MapSelect => ui::map_select(&self.egui.ctx, &self.map_select_state),
                UiScreen::Matchmaking => ui::matchmaking(&self.egui.ctx),
                UiScreen::Settings => ui::settings(&self.egui.ctx, &mut self.game_settings),
                UiScreen::PauseMenu => ui::pause_menu(&self.egui.ctx),
                UiScreen::InGame => {
                    let debug = if self.game_settings.show_debug {
                        Some(self.build_debug_info())
                    } else {
                        None
                    };
                    ui::hud(&self.egui.ctx, self.fps, debug.as_ref());
                    None
                }
                _ => None,
            }
        };

        if let Some(msg) = msg {
            self.handle_ui_message(msg);
        }

        self.render_ctx.set_present_mode(self.game_settings.vsync);

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
        let should_render_3d = matches!(self.ui_screen, UiScreen::InGame | UiScreen::PauseMenu)
            || self.render_dev_3d();

        if should_render_3d {
            self.camera.fov = self.game_settings.fov.to_radians();
            self.camera_resources
                .update(&self.render_ctx.queue, &self.camera);

            self.path_tracer.update_voxel_volume(
                &self.render_ctx.queue,
                &self.world,
                self.camera.position,
            );

            self.path_tracer.update_params(
                &self.render_ctx.queue,
                &self.camera,
                self.lighting.sun_direction,
                self.lighting.sun_intensity,
                self.lighting.sun_color,
                self.is_walking,
                self.game_settings.max_bounces,
            );

            let show_debug = self.game_settings.show_debug;
            // In free cam, cull from the player's frozen perspective so you can
            // see frustum culling at work from the outside.
            let frustum_planes = if self.free_cam {
                camera::frustum_planes_from(
                    self.player.eye_position(),
                    self.player_yaw,
                    self.player_pitch,
                    self.camera.fov,
                    self.camera.aspect,
                    self.camera.near,
                    self.camera.far,
                )
            } else {
                self.camera.frustum_planes()
            };
            let chunk_world_size = voxel::CHUNK_SIZE as f32 * VOXEL_SCALE;
            let camera_pos = self.camera.position;
            let do_frustum_cull = true;
            let enable_terrain_occlusion = !self.free_cam
                && self.use_streaming
                && matches!(
                    self.game_settings.performance_preset,
                    ui::PerformancePreset::Potato
                );
            let meshes: Vec<(&wgpu::Buffer, u32, u32)> =
                self.chunk_meshes.iter().filter_map(move |(key, m)| {
                    let (min, max) = match key {
                        MeshKey::Chunk(pos) => {
                            let min = glam::Vec3::new(
                                pos.x as f32 * chunk_world_size,
                                pos.y as f32 * chunk_world_size,
                                pos.z as f32 * chunk_world_size,
                            );
                            (min, min + glam::Vec3::splat(chunk_world_size))
                        }
                        MeshKey::LodNode(key) => {
                            let side = (1 << key.lod_level) as f32 * chunk_world_size;
                            let min = glam::Vec3::new(
                                key.x as f32 * side,
                                key.y as f32 * side,
                                key.z as f32 * side,
                            );
                            (min, min + glam::Vec3::splat(side))
                        }
                    };
                    if do_frustum_cull && !camera::aabb_in_frustum(min, max, &frustum_planes) {
                        return None;
                    }
                    if enable_terrain_occlusion
                        && matches!(key, MeshKey::Chunk(_))
                        && Self::occluded_by_terrain_heightfield(camera_pos, min, max)
                    {
                        return None;
                    }
                    let lod = if show_debug {
                        match key {
                            MeshKey::Chunk(_) => 0u32,
                            MeshKey::LodNode(k) => k.lod_level as u32,
                        }
                    } else {
                        0u32
                    };
                    Some((&m.vertex_buffer, m.num_vertices, lod))
                }).collect();

            self.path_tracer.update_shadow_cascades(
                &self.render_ctx.queue,
                &self.camera,
                self.lighting.sun_direction,
            );

            self.path_tracer.render(
                &mut encoder,
                &view,
                &self.camera_resources.bind_group,
                &self.palette_resources.bind_group,
                self.character_manager.bind_group(),
                &meshes,
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
        if self.handle_dev_ui_message(&msg) {
            return;
        }

        match msg {
            UiMessage::OpenMapSelect => {
                self.prev_screen = UiScreen::MainMenu;
                self.map_select_state.worlds = self.scan_playable_worlds();
                self.ui_screen = UiScreen::MapSelect;
            }
            UiMessage::SelectMap(path) => {
                self.selected_map_path = Some(path);
                self.prev_screen = UiScreen::MapSelect;
                self.ui_screen = UiScreen::Matchmaking;
            }
            UiMessage::FindMatch => {
                if let Some(path) = self.selected_map_path.take() {
                    self.load_big_world_from_file(&path);
                }
            }
            UiMessage::CancelMatchmaking => {
                self.ui_screen = UiScreen::MapSelect;
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
                self.ui_screen = self.dev_resume_target().unwrap_or(UiScreen::InGame);
                self.grab_mouse(true);
            }
            UiMessage::QuitToMenu => {
                self.region_manager = None;
                self.ui_screen = UiScreen::MainMenu;
                self.grab_mouse(false);
            }
            #[cfg(feature = "dev-tools")]
            _ => {}
        }
    }
}

//! ECS Application Handler - bridges winit with Specs ECS
//! 
//! Note: RenderResources is stored outside of the ECS World to avoid borrow checker issues

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::PhysicalKey,
    window::Window,
};

use specs::{Dispatcher, World, WorldExt};

use crate::ecs::{self, build_dispatcher, setup_world};
use crate::ecs::entities::create_local_player;
use crate::ecs::components::EditorMode;
use crate::ecs::resources::{GameSettings, GameTime, InputResource, MeshStorage, WorldResource};
use crate::ecs::systems::mesh::{MeshBuildResult, PendingMeshBuilds};
use crate::renderer::{CameraResources, LightingParams, MeshResources, PaletteResources, RenderContext};
use crate::pathtracer::PathTracer;
use crate::character::CharacterManager;
use crate::ui::{EguiRenderer, UiScreen, UiMessage, MapSelectState, main_menu, map_select, settings};

/// Render resources stored outside ECS to avoid borrow issues
pub struct AppRenderResources {
    pub ctx: RenderContext,
    pub camera: CameraResources,
    pub palette: PaletteResources,
    pub lighting: LightingParams,
    pub path_tracer: PathTracer,
    pub character_manager: CharacterManager,
    pub egui: EguiRenderer,
}

/// ECS-based application state
pub struct EcsApp {
    world: World,
    dispatcher: Dispatcher<'static, 'static>,
    window: Option<Arc<Window>>,
    render_res: Option<AppRenderResources>,
}

impl EcsApp {
    pub fn new() -> Self {
        let mut world = World::new();
        
        // Register all components
        setup_world(&mut world);
        
        // Build system dispatcher
        let dispatcher = build_dispatcher();
        
        Self {
            world,
            dispatcher,
            window: None,
            render_res: None,
        }
    }

    /// Initialize resources after window creation
    async fn init_resources(&mut self, window: Arc<Window>) -> anyhow::Result<()> {
        let size = window.inner_size();
        let aspect = size.width as f32 / size.height.max(1) as f32;

        // Create render resources
        let mut render_ctx = RenderContext::new(window.clone()).await?;
        
        // Configure surface for presentation
        render_ctx.configure();
        
        let camera_resources = CameraResources::new(&render_ctx.device, &crate::camera::Camera::new(glam::Vec3::ZERO, 1.0));
        let palette_resources = PaletteResources::new(&render_ctx.device);
        let lighting = LightingParams::new();
        
        // Character manager
        let character_manager = CharacterManager::new(&render_ctx.device);
        
        // Path tracer
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
        
        // Egui renderer for UI
        let egui = EguiRenderer::new(&render_ctx.device, render_ctx.format(), &window);

        // Store render resources directly (not in ECS)
        self.render_res = Some(AppRenderResources {
            ctx: render_ctx,
            camera: camera_resources,
            palette: palette_resources,
            lighting,
            path_tracer,
            character_manager,
            egui,
        });

        // Insert game resources (non-render)
        self.world.insert(GameTime::default());
        self.world.insert(InputResource::default());
        self.world.insert(GameSettings::default());
        self.world.insert(MeshStorage::default());
        self.world.insert(PendingMeshBuilds::default());
        self.world.insert(ecs::resources::Lighting::default());
        self.world.insert(UiScreen::MainMenu); // Start with main menu
        self.world.insert(MapSelectState::default());
        self.world.insert(EditorMode::default());
        
        // Create world with initial chunks
        let mut world_res = WorldResource::new();
        world_res.chunk_manager = create_initial_world();
        
        // Mark all chunks as dirty for initial mesh generation
        // Collect positions first to avoid borrow issues
        let chunk_positions: Vec<_> = world_res.chunk_manager.chunk_positions().collect();
        for pos in chunk_positions {
            world_res.mark_dirty(pos);
        }
        
        self.world.insert(world_res);
        self.world.insert(ecs::resources::MeshGenerationResource::default());
        self.world.insert(ecs::resources::WindowDimensions {
            width: size.width,
            height: size.height,
            scale_factor: window.scale_factor(),
        });
        self.world.insert(ecs::resources::EntityLookup::default());

        // Create local player entity
        create_local_player(&mut self.world, aspect);

        log::info!("[ECS] Initialized resources and created player entity");

        Ok(())
    }

    /// Handle window resize
    fn resize(&mut self, width: u32, height: u32) {
        let mut dims = self.world.write_resource::<ecs::resources::WindowDimensions>();
        dims.width = width;
        dims.height = height;

        if width > 0 && height > 0 {
            if let Some(render_res) = &mut self.render_res {
                render_res.ctx.resize(width, height);
                render_res.path_tracer.resize(&render_res.ctx.device, width, height);
            }
        }
    }

    /// Process input events and update InputResource
    fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        // First, pass event to egui for UI handling
        if let Some(render_res) = &mut self.render_res {
            if let Some(window) = &self.window {
                let response = render_res.egui.handle_window_event(window, event);
                if response.consumed {
                    return true; // Event was consumed by UI
                }
            }
        }
        
        let mut input = self.world.write_resource::<InputResource>();

        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::KeyCode;
                let is_pressed = event.state.is_pressed();
                
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::KeyW => input.forward = is_pressed,
                        KeyCode::KeyS => input.backward = is_pressed,
                        KeyCode::KeyA => input.left = is_pressed,
                        KeyCode::KeyD => input.right = is_pressed,
                        KeyCode::Space => input.jump = is_pressed,
                        KeyCode::ShiftLeft => input.ctrl = is_pressed,
                        KeyCode::ShiftRight => input.sprint = is_pressed,
                        KeyCode::KeyQ => input.up = is_pressed,
                        KeyCode::KeyE => input.down = is_pressed,
                        KeyCode::Escape => {
                            if is_pressed {
                                // TODO: Toggle pause menu
                            }
                        }
                        _ => return false,
                    }
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    /// Handle mouse motion
    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        let mut input = self.world.write_resource::<InputResource>();
        input.mouse_delta_x += delta.0 as f32;
        input.mouse_delta_y += delta.1 as f32;
    }

    /// Run systems and render
    fn update_and_render(&mut self) {
        // Dispatch all game logic systems
        self.dispatcher.dispatch(&self.world);
        self.world.maintain();

        // Clear mouse delta after systems have processed it
        {
            let mut input = self.world.write_resource::<InputResource>();
            input.reset_mouse();
        }

        // Perform rendering
        self.render_frame();
    }

    /// Render a single frame
    fn render_frame(&mut self) {
        // Extract all data we need from ECS first
        let camera_data = self.extract_camera_data();
        let (lighting_mode, max_bounces) = self.extract_render_settings();
        let is_walking = self.check_is_walking();

        // Perform rendering
        self.execute_render(camera_data, lighting_mode, max_bounces, is_walking);
    }

    /// Extract camera data from ECS
    fn extract_camera_data(&self) -> Option<(crate::camera::Camera, glam::Vec3)> {
        let lookup = self.world.read_resource::<ecs::resources::EntityLookup>();
        let positions = self.world.read_storage::<ecs::components::Position>();
        let cameras = self.world.read_storage::<ecs::components::Camera>();
        let dims = self.world.read_resource::<ecs::resources::WindowDimensions>();
        
        let cam_entity = lookup.active_camera?;
        let camera_pos = positions.get(cam_entity)?.0;
        let cam = cameras.get(cam_entity)?;
        
        // Note: camera::Camera has private bob/shake fields, so we just use defaults
        let render_camera = crate::camera::Camera::new(camera_pos, dims.aspect_ratio());
        
        Some((render_camera, camera_pos))
    }

    /// Extract render settings from ECS
    fn extract_render_settings(&self) -> (crate::ui::LightingMode, u32) {
        let settings = self.world.read_resource::<GameSettings>();
        (settings.lighting_mode, settings.max_bounces)
    }

    /// Check if player is walking
    fn check_is_walking(&self) -> bool {
        let lookup = self.world.read_resource::<ecs::resources::EntityLookup>();
        let walking_states = self.world.read_storage::<ecs::components::WalkingState>();
        
        lookup.local_player
            .and_then(|e| walking_states.get(e))
            .map(|w| w.is_walking)
            .unwrap_or(false)
    }
    
    /// Handle UI messages from button clicks
    fn handle_ui_message(&mut self, msg: UiMessage) {
        match msg {
            UiMessage::OpenMapSelect => {
                log::info!("[UI] Opening map select screen");
                {
                    let mut editor_mode = self.world.write_resource::<EditorMode>();
                    editor_mode.enabled = false;
                }
                let mut screen = self.world.write_resource::<UiScreen>();
                *screen = UiScreen::MapSelect;
            }
            #[cfg(feature = "dev-tools")]
            UiMessage::OpenDevTools => {
                log::info!("[UI] Opening editor map select");
                {
                    let mut editor_mode = self.world.write_resource::<EditorMode>();
                    editor_mode.enabled = true;
                }
                let mut screen = self.world.write_resource::<UiScreen>();
                *screen = UiScreen::MapSelect;
            }
            UiMessage::SelectMap(_map) => {
                log::info!("[UI] Map selected, starting game");
                let editor_mode_enabled = {
                    let editor_mode = self.world.read_resource::<EditorMode>();
                    editor_mode.enabled
                };
                let mut screen = self.world.write_resource::<UiScreen>();
                if editor_mode_enabled {
                    #[cfg(feature = "dev-tools")]
                    {
                        *screen = UiScreen::Editor;
                    }
                    #[cfg(not(feature = "dev-tools"))]
                    {
                        *screen = UiScreen::InGame;
                    }
                } else {
                    *screen = UiScreen::InGame;
                }
                
                // Grab mouse for gameplay
                if let Some(window) = &self.window {
                    let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Confined);
                    window.set_cursor_visible(false);
                }
            }
            UiMessage::Settings => {
                log::info!("[UI] Opening settings screen");
                let mut screen = self.world.write_resource::<UiScreen>();
                *screen = UiScreen::Settings;
            }
            UiMessage::Back => {
                log::info!("[UI] Going back to main menu");
                let mut screen = self.world.write_resource::<UiScreen>();
                *screen = UiScreen::MainMenu;
            }
            UiMessage::Exit => {
                log::info!("[UI] Exit requested");
                // TODO: Properly exit the application
            }
            _ => {
                log::debug!("[UI] Unhandled message: {:?}", msg);
            }
        }
    }

    /// Process pending mesh builds and upload to GPU
    fn process_pending_meshes(&mut self) {
        let render_res = match &mut self.render_res {
            Some(res) => res,
            None => return,
        };
        
        // Take pending builds from ECS
        let pending_builds: Vec<MeshBuildResult> = {
            let mut pending = self.world.write_resource::<PendingMeshBuilds>();
            std::mem::take(&mut pending.builds)
        };
        
        // Process each pending build
        let mut mesh_storage = self.world.write_resource::<MeshStorage>();
        
        for build in pending_builds {
            if build.vertices.is_empty() {
                // Remove empty mesh
                mesh_storage.remove(&build.key);
            } else {
                // Create GPU mesh
                let mesh = MeshResources::new(&render_res.ctx.device, &build.vertices);
                mesh_storage.insert(build.key, mesh);
                log::debug!("[ECS] Created GPU mesh for {:?} with {} vertices", build.key, build.vertices.len());
            }
        }
    }

    /// Execute the actual GPU render commands
    fn execute_render(
        &mut self,
        camera_data: Option<(crate::camera::Camera, glam::Vec3)>,
        lighting_mode: crate::ui::LightingMode,
        _max_bounces: u32,
        is_walking: bool,
    ) {
        // Process any pending mesh builds first
        self.process_pending_meshes();

        let ui_msg = {
            let render_res = match &mut self.render_res {
                Some(res) => res,
                None => return,
            };

            // Sync lighting from ECS to render resources
            {
                let lighting = self.world.read_resource::<ecs::resources::Lighting>();
                render_res.lighting.sun_direction = lighting.sun_direction.to_array();
                render_res.lighting.sun_intensity = lighting.sun_intensity;
                render_res.lighting.sun_color = lighting.sun_color.to_array();
            }

            let camera_pos = camera_data
                .as_ref()
                .map(|(_, pos)| *pos)
                .unwrap_or(glam::Vec3::ZERO);

            let mut ui_msg: Option<UiMessage> = None;

            match render_res.ctx.surface.get_current_texture() {
                Ok(output) => {
                    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

                    // Update voxel volume
                    {
                        let world_res = self.world.read_resource::<WorldResource>();
                        render_res.path_tracer.update_voxel_volume(
                            &render_res.ctx.queue,
                            &world_res.chunk_manager,
                            camera_pos,
                        );
                    }

                    // Extract mesh buffers (borrowed during render)
                    let mesh_storage = self.world.read_resource::<MeshStorage>();
                    let mesh_refs: Vec<(&wgpu::Buffer, u32, u32)> = mesh_storage
                        .meshes
                        .iter()
                        .map(|(_, m)| (&m.vertex_buffer, m.num_vertices, 0u32))
                        .collect();

                    let mut encoder = render_res
                        .ctx
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Render Encoder"),
                        });

                    if let Some((camera, _)) = camera_data {
                        render_res.camera.update(&render_res.ctx.queue, &camera);

                        render_res.path_tracer.update_params(
                            &render_res.ctx.queue,
                            &camera,
                            render_res.lighting.sun_direction,
                            render_res.lighting.sun_intensity,
                            render_res.lighting.sun_color,
                            is_walking,
                            4,
                        );

                        render_res.path_tracer.update_shadow_cascades(
                            &render_res.ctx.queue,
                            &camera,
                            render_res.lighting.sun_direction,
                        );

                        render_res.path_tracer.render(
                            &mut encoder,
                            &view,
                            &render_res.camera.bind_group,
                            &render_res.palette.bind_group,
                            render_res.character_manager.bind_group(),
                            &mesh_refs,
                            lighting_mode,
                        );
                    } else {
                        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Clear Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                    }

                    if let Some(window) = &self.window {
                        render_res.egui.begin_frame(window);

                        let ui_screen = *self.world.read_resource::<UiScreen>();
                        ui_msg = match ui_screen {
                            UiScreen::MainMenu => main_menu(&render_res.egui.ctx),
                            UiScreen::Settings => {
                                let mut game_settings =
                                    self.world.write_resource::<GameSettings>();
                                settings(&render_res.egui.ctx, &mut *game_settings)
                            }
                            UiScreen::MapSelect => {
                                let map_state = self.world.read_resource::<MapSelectState>();
                                map_select(&render_res.egui.ctx, &map_state)
                            }
                            _ => None,
                        };

                        let screen_descriptor = egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [
                                render_res.ctx.config.width,
                                render_res.ctx.config.height,
                            ],
                            pixels_per_point: window.scale_factor() as f32,
                        };

                        render_res.egui.end_frame(
                            &render_res.ctx.device,
                            &render_res.ctx.queue,
                            &mut encoder,
                            window,
                            &view,
                            screen_descriptor,
                        );
                    }

                    render_res.ctx.queue.submit(std::iter::once(encoder.finish()));
                    output.present();
                }
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    if let Some(window) = &self.window {
                        let size = window.inner_size();
                        self.resize(size.width, size.height);
                    }
                }
                Err(e) => {
                    log::error!("Render error: {}", e);
                }
            }

            ui_msg
        };

        if let Some(msg) = ui_msg {
            self.handle_ui_message(msg);
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl Default for EcsApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for EcsApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("THE DROP - Voxel Engine (ECS)");
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        
        pollster::block_on(self.init_resources(window.clone())).unwrap();
        self.window = Some(window);
        
        log::info!("[ECS] Application resumed and initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                self.resize(size.width, size.height);
                return;
            }
            WindowEvent::RedrawRequested => {
                self.update_and_render();
                return;
            }
            _ => {}
        }

        // Handle input
        self.handle_window_event(&event);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.handle_mouse_motion(delta);
        }
    }
}

/// Create initial world with a platform for the player to stand on
fn create_initial_world() -> crate::world::ChunkManager {
    use crate::voxel::{Chunk, VOXEL_SCALE};
    use crate::world::{ChunkManager, ChunkPosition};
    
    // Create world with 8x8 chunks platform (256x256 voxels)
    let mut world = ChunkManager::with_metadata("Default World", [8.0, 1.25, 8.0]);
    
    // Create flat ground
    for cx in 0..8 {
        for cz in 0..8 {
            let mut chunk = Chunk::new();
            chunk.fill_ground(1, 42); // Stone gray color
            world.insert_chunk(ChunkPosition::new(cx, 0, cz), chunk);
        }
    }
    
    log::info!("[ECS] Created initial world with {} chunks", 8 * 8);
    
    world
}

/// Run the ECS-based application
pub fn run_ecs() -> Result<(), winit::error::EventLoopError> {
    env_logger::init();

    // Rayon workers configuration
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .ok();

    let event_loop = EventLoop::new()?;
    let mut app = EcsApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

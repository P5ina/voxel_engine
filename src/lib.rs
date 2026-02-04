use std::sync::Arc;

use glam::Vec3;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window},
};

mod camera;
mod pathtracer;
mod player;
mod renderer;
mod ui;
mod voxel;

use camera::Camera;
use pathtracer::PathTracer;
use player::Player;
use renderer::{
    CameraResources, DepthBuffer, LightingParams, MeshResources, RenderContext, TextureResources,
};
use ui::{EguiRenderer, GameSettings, UiMessage, UiScreen};
use voxel::{generate_mesh, raycast, BlockType, Chunk};

#[derive(Default)]
struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    jump: bool,
}

pub struct AppState {
    window: Arc<Window>,

    // Rendering
    render_ctx: RenderContext,
    depth_buffer: DepthBuffer,
    camera_resources: CameraResources,
    texture_resources: TextureResources,
    mesh_resources: MeshResources,
    lighting: LightingParams,
    path_tracer: PathTracer,

    // Game state
    camera: Camera,
    player: Player,
    chunk: Chunk,
    mesh_dirty: bool,

    // Input
    input: InputState,
    mouse_grabbed: bool,

    // UI
    egui: EguiRenderer,
    ui_screen: UiScreen,
    game_settings: GameSettings,
    prev_screen: UiScreen,

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

        // Generate chunk mesh
        let mut chunk = Chunk::new();
        chunk.fill_ground(8);
        let vertices = generate_mesh(&chunk);
        let mesh_resources = MeshResources::new(&render_ctx.device, &vertices);

        // Lighting
        let mut lighting = LightingParams::new();
        lighting.update_time(0.1);

        // UI
        let egui = EguiRenderer::new(&render_ctx.device, render_ctx.format(), &window);

        // Path Tracer
        let path_tracer = PathTracer::new(
            &render_ctx.device,
            &render_ctx.queue,
            size.width,
            size.height,
            render_ctx.format(),
            &camera_resources.bind_group_layout,
            &texture_resources.bind_group_layout,
        );
        path_tracer.update_voxels(&render_ctx.queue, &chunk);

        Ok(Self {
            window,
            render_ctx,
            depth_buffer,
            camera_resources,
            texture_resources,
            mesh_resources,
            lighting,
            path_tracer,
            camera,
            player,
            chunk,
            mesh_dirty: false,
            input: InputState::default(),
            mouse_grabbed: false,
            egui,
            ui_screen: UiScreen::default(),
            game_settings: GameSettings::default(),
            prev_screen: UiScreen::MainMenu,
            last_frame: std::time::Instant::now(),
            fps: 0.0,
            frame_time_accum: 0.0,
            frame_count: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.render_ctx.resize(width, height);
            self.depth_buffer
                .resize(&self.render_ctx.device, width, height);
            self.camera.resize(width, height);
            self.path_tracer.resize(&self.render_ctx.device, width, height);
        }
    }

    fn rebuild_mesh(&mut self) {
        let vertices = generate_mesh(&self.chunk);
        self.mesh_resources
            .update(&self.render_ctx.device, &vertices);

        self.path_tracer
            .update_voxels(&self.render_ctx.queue, &self.chunk);
        self.path_tracer.reset_accumulation();

        self.mesh_dirty = false;
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
        const MOVE_SPEED: f32 = 35.0;
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

        self.player.update(&self.chunk, dt);
        self.camera.position = self.player.eye_position();
        self.camera.fov = self.game_settings.fov.to_radians();

        // Head bob when walking
        let is_walking = input.length_squared() > 0.0;
        self.camera
            .update_bob(is_walking, self.player.on_ground, dt);

        self.camera_resources
            .update(&self.render_ctx.queue, &self.camera);
        self.lighting.update_time(0.1);

        if self.mesh_dirty {
            self.rebuild_mesh();
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.ui_screen == UiScreen::InGame {
            self.update();
        } else {
            self.last_frame = std::time::Instant::now();
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
        if self.ui_screen == UiScreen::InGame || self.ui_screen == UiScreen::PauseMenu {
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

            self.path_tracer.render(
                &mut encoder,
                &view,
                &self.depth_buffer.view,
                &self.camera_resources.bind_group,
                &self.texture_resources.bind_group,
                &self.mesh_resources.vertex_buffer,
                self.mesh_resources.num_vertices,
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
                self.ui_screen = UiScreen::InGame;
                self.grab_mouse(true);
            }
            UiMessage::QuitToMenu => {
                self.ui_screen = UiScreen::MainMenu;
                self.grab_mouse(false);
            }
        }
    }

    fn grab_mouse(&mut self, grab: bool) {
        self.mouse_grabbed = grab;
        if grab {
            let _ = self
                .window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Locked));
            self.window.set_cursor_visible(false);
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        }
    }

    fn handle_key(&mut self, _event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        if !is_pressed {
            if self.ui_screen == UiScreen::InGame {
                match code {
                    KeyCode::KeyW => self.input.forward = false,
                    KeyCode::KeyS => self.input.backward = false,
                    KeyCode::KeyA => self.input.left = false,
                    KeyCode::KeyD => self.input.right = false,
                    KeyCode::Space => self.input.jump = false,
                    _ => {}
                }
            }
            return;
        }

        match code {
            KeyCode::Escape => match self.ui_screen {
                UiScreen::InGame => {
                    self.ui_screen = UiScreen::PauseMenu;
                    self.grab_mouse(false);
                }
                UiScreen::PauseMenu => {
                    self.ui_screen = UiScreen::InGame;
                    self.grab_mouse(true);
                }
                UiScreen::Settings => {
                    self.ui_screen = self.prev_screen;
                }
                UiScreen::MainMenu => {}
            },
            _ if self.ui_screen == UiScreen::InGame => match code {
                KeyCode::KeyW => self.input.forward = true,
                KeyCode::KeyS => self.input.backward = true,
                KeyCode::KeyA => self.input.left = true,
                KeyCode::KeyD => self.input.right = true,
                KeyCode::Space => self.input.jump = true,
                _ => {}
            },
            _ => {}
        }
    }

    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        if self.mouse_grabbed && self.ui_screen == UiScreen::InGame {
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

        const MAX_REACH: f32 = 8.0;
        let origin = self.camera.position;
        let direction = self.camera.forward();

        let hit = raycast(origin, direction, MAX_REACH, |x, y, z| {
            self.chunk.get_signed(x, y, z).is_solid()
        });

        match button {
            MouseButton::Left => {
                if let Some(hit) = hit {
                    let [x, y, z] = hit.block_pos;
                    if x >= 0 && y >= 0 && z >= 0 {
                        self.chunk
                            .set(x as usize, y as usize, z as usize, BlockType::Air);
                        self.mesh_dirty = true;
                    }
                }
            }
            MouseButton::Right => {
                if let Some(hit) = hit {
                    let [x, y, z] = hit.block_pos;
                    let [nx, ny, nz] = hit.normal;
                    let place_x = x + nx;
                    let place_y = y + ny;
                    let place_z = z + nz;

                    if place_x >= 0
                        && place_y >= 0
                        && place_z >= 0
                        && !self.player.intersects_block(place_x, place_y, place_z)
                    {
                        self.chunk.set(
                            place_x as usize,
                            place_y as usize,
                            place_z as usize,
                            BlockType::Stone,
                        );
                        self.mesh_dirty = true;
                    }
                }
            }
            _ => {}
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

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::AppState;
use crate::ui::UiScreen;

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
            && key_state.is_pressed()
            && state.ui_screen == UiScreen::Editor
        {
            state.grab_mouse(!state.mouse_grabbed);
            return;
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
        if let Some(state) = &mut self.state
            && let DeviceEvent::MouseMotion { delta } = event
        {
            state.handle_mouse_motion(delta);
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

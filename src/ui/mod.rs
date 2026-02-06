mod screens;
mod state;

pub use screens::*;
pub use state::*;

use winit::window::Window;

pub struct EguiRenderer {
    pub ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiRenderer {
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();

        // Dark style
        ctx.set_style(egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        });

        let viewport_id = ctx.viewport_id();
        let state = egui_winit::State::new(ctx.clone(), viewport_id, window, None, None, None);

        let renderer =
            egui_wgpu::Renderer::new(device, output_format, egui_wgpu::RendererOptions::default());

        Self {
            ctx,
            state,
            renderer,
        }
    }

    pub fn handle_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input = self.state.take_egui_input(window);
        self.ctx.begin_pass(raw_input);
    }

    pub fn end_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        window: &Window,
        view: &wgpu::TextureView,
        screen_descriptor: egui_wgpu::ScreenDescriptor,
    ) {
        let output = self.ctx.end_pass();

        self.state
            .handle_platform_output(window, output.platform_output);

        let tris = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        for (id, delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }

        self.renderer
            .update_buffers(device, queue, encoder, &tris, &screen_descriptor);

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            let mut render_pass = render_pass.forget_lifetime();
            self.renderer
                .render(&mut render_pass, &tris, &screen_descriptor);
        }

        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}

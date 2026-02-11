pub mod gbuffer;
pub mod materials;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::ui::LightingMode;
use crate::voxel::{CHUNK_SIZE, VOXEL_SCALE};
use crate::world::{ChunkManager, ChunkPosition};

/// Size of the 3D voxel volume texture (in voxels per axis).
/// Covers VOLUME_SIZE * VOXEL_SCALE meters per axis, centered on camera.
const VOLUME_SIZE: usize = 256;
/// Number of chunks that fit along one axis of the volume (256/32 = 8).
const CHUNKS_PER_VOL_AXIS: i32 = (VOLUME_SIZE / CHUNK_SIZE) as i32;

pub use gbuffer::GBuffer;
pub use materials::Palette;

/// Denoise parameters uniform
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct DenoiseParams {
    pub screen_width: u32,
    pub screen_height: u32,
    pub step_size: u32,
    pub _padding: u32,
}

/// Path tracer parameters uniform
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct PathTracerParams {
    pub camera_position: [f32; 3],
    pub frame_index: u32,
    pub inv_view_proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub camera_up: [f32; 3],
    pub accumulated_frames: u32,
    pub camera_right: [f32; 3],
    pub max_bounces: u32,
    pub camera_forward: [f32; 3],
    pub voxel_size: f32,
    pub volume_min: [f32; 3],
    pub screen_width: u32,
    pub volume_max: [f32; 3],
    pub screen_height: u32,
    pub sun_direction: [f32; 3],
    pub sun_intensity: f32,
    pub sun_color: [f32; 3],
    pub _padding: u32,
}

impl Default for PathTracerParams {
    fn default() -> Self {
        Self {
            camera_position: [0.0; 3],
            frame_index: 0,
            inv_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            camera_up: [0.0, 1.0, 0.0],
            accumulated_frames: 0,
            camera_right: [1.0, 0.0, 0.0],
            max_bounces: 4,
            camera_forward: [0.0, 0.0, -1.0],
            voxel_size: VOXEL_SCALE,
            volume_min: [0.0; 3],
            screen_width: 800,
            volume_max: [32.0 * VOXEL_SCALE; 3],
            screen_height: 600,
            sun_direction: [0.5, 1.0, 0.3],
            sun_intensity: 1.0,
            sun_color: [1.0, 0.98, 0.9],
            _padding: 0,
        }
    }
}

/// Accumulation buffer for temporal accumulation
/// Uses 3 textures: current (path trace output), history (prev accumulated), output (new accumulated)
#[allow(dead_code)]
pub struct AccumulationBuffer {
    pub current_texture: wgpu::Texture,
    pub current_view: wgpu::TextureView,
    pub history_texture: wgpu::Texture,
    pub history_view: wgpu::TextureView,
    pub output_texture: wgpu::Texture,
    pub output_view: wgpu::TextureView,
}

impl AccumulationBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let create_texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };

        let current_texture = create_texture("PT Current Accumulation");
        let history_texture = create_texture("PT History Accumulation");
        let output_texture = create_texture("PT Output Accumulation");

        let current_view = current_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let history_view = history_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            current_texture,
            current_view,
            history_texture,
            history_view,
            output_texture,
            output_view,
        }
    }

    /// Copy output to history for next frame's accumulation
    pub fn swap(&mut self, encoder: &mut wgpu::CommandEncoder, width: u32, height: u32) {
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.history_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Main path tracer structure
#[allow(dead_code)]
pub struct PathTracer {
    pub gbuffer: GBuffer,
    pub accumulation: AccumulationBuffer,
    pub params: PathTracerParams,
    pub params_buffer: wgpu::Buffer,
    pub materials_buffer: wgpu::Buffer,
    pub voxel_texture: wgpu::Texture,
    pub voxel_view: wgpu::TextureView,

    // World bounds for the voxel volume (updated by update_voxel_volume)
    pub world_size: (usize, usize, usize),
    pub world_origin: (i32, i32, i32),
    /// Chunk-coordinate min of the current volume (toroidal, for incremental updates)
    volume_chunk_min: (i32, i32, i32),

    // G-buffer pass
    pub gbuffer_pipeline: wgpu::RenderPipeline,

    // Path trace pass
    pub pathtrace_pipeline: wgpu::ComputePipeline,
    pub pathtrace_bind_group_layout: wgpu::BindGroupLayout,
    pub pathtrace_bind_group: wgpu::BindGroup,

    // Direct lighting pass (simple mode)
    pub direct_pipeline: wgpu::ComputePipeline,
    pub direct_bind_group: wgpu::BindGroup,

    // Accumulation pass
    pub accumulate_pipeline: wgpu::ComputePipeline,
    pub accumulate_bind_group_layout: wgpu::BindGroupLayout,
    pub accumulate_bind_group: wgpu::BindGroup,

    // Denoise pass (3 passes with step sizes 1, 2, 4)
    pub denoise_bind_group_layout: wgpu::BindGroupLayout,
    pub denoise_params_buffers: [wgpu::Buffer; 3],
    pub denoise_ping_texture: wgpu::Texture,
    pub denoise_ping_view: wgpu::TextureView,
    pub denoise_pong_texture: wgpu::Texture,
    pub denoise_pong_view: wgpu::TextureView,
    // 3 passes: [0] accum->ping, [1] ping->pong, [2] pong->ping
    pub denoise_bind_groups: [wgpu::BindGroup; 3],

    // Tonemap pass
    pub tonemap_pipeline: wgpu::RenderPipeline,
    pub tonemap_bind_group_layout: wgpu::BindGroupLayout,
    pub tonemap_bind_group: wgpu::BindGroup,
    pub tonemap_bind_group_simple: wgpu::BindGroup,
    pub tonemap_sampler: wgpu::Sampler,

    // Internal depth buffer (at internal resolution for G-buffer pass)
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,

    // State
    pub frame_index: u32,
    pub accumulated_frames: u32,
    pub prev_view_proj: Mat4,
    pub width: u32,
    pub height: u32,
    pub render_scale: f32,
    pub internal_width: u32,
    pub internal_height: u32,
}

impl PathTracer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        character_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let render_scale: f32 = 1.0;
        let internal_width = ((width as f32 * render_scale) as u32).max(1);
        let internal_height = ((height as f32 * render_scale) as u32).max(1);

        // Create G-buffer at internal resolution
        let gbuffer = GBuffer::new(device, internal_width, internal_height);

        // Create accumulation buffers at internal resolution
        let accumulation = AccumulationBuffer::new(device, internal_width, internal_height);

        // Create params buffer
        let params = PathTracerParams::default();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PathTracer Params Buffer"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create materials buffer with full 256-color palette
        let palette = Palette::new();
        let materials_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Materials Buffer"),
            contents: bytemuck::cast_slice(palette.as_slice()),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // Create voxel 3D texture for DDA ray tracing (shadow rays + path tracing bounces)
        let voxel_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Voxel Volume Texture"),
            size: wgpu::Extent3d {
                width: VOLUME_SIZE as u32,
                height: VOLUME_SIZE as u32,
                depth_or_array_layers: VOLUME_SIZE as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let voxel_view = voxel_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create tonemap sampler (non-filtering for Rgba32Float)
        let tonemap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Tonemap Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group layouts and pipelines
        let (_gbuffer_bind_group_layout, gbuffer_pipeline) = Self::create_gbuffer_pipeline(
            device,
            camera_bind_group_layout,
            texture_bind_group_layout,
        );

        let (pathtrace_bind_group_layout, pathtrace_pipeline) =
            Self::create_pathtrace_pipeline(device, character_bind_group_layout);

        let direct_pipeline = Self::create_direct_pipeline(
            device,
            &pathtrace_bind_group_layout,
            character_bind_group_layout,
        );

        let (accumulate_bind_group_layout, accumulate_pipeline) =
            Self::create_accumulate_pipeline(device);

        let (denoise_bind_group_layout, _denoise_pipeline) = Self::create_denoise_pipeline(device);

        let (tonemap_bind_group_layout, tonemap_pipeline) =
            Self::create_tonemap_pipeline(device, surface_format);

        // Create denoise resources (3 passes with step sizes 1, 2, 4)
        let step_sizes: [u32; 3] = [1, 2, 4];
        let denoise_params_buffers: [wgpu::Buffer; 3] = std::array::from_fn(|i| {
            let params = DenoiseParams {
                screen_width: internal_width,
                screen_height: internal_height,
                step_size: step_sizes[i],
                _padding: 0,
            };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Denoise Params Buffer {}", i)),
                contents: bytemuck::cast_slice(&[params]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });

        let (denoise_ping_texture, denoise_ping_view, denoise_pong_texture, denoise_pong_view) =
            Self::create_denoise_textures(device, internal_width, internal_height);

        // Create internal depth buffer at internal resolution
        let (depth_texture, depth_view) =
            Self::create_depth_texture(device, internal_width, internal_height);

        // Create bind groups
        let pathtrace_bind_group = Self::create_pathtrace_bind_group(
            device,
            &pathtrace_bind_group_layout,
            &gbuffer,
            &params_buffer,
            &materials_buffer,
            &voxel_view,
            &accumulation.current_view,
        );

        // Direct lighting uses same bind group layout as path tracing
        let direct_bind_group = Self::create_pathtrace_bind_group(
            device,
            &pathtrace_bind_group_layout,
            &gbuffer,
            &params_buffer,
            &materials_buffer,
            &voxel_view,
            &accumulation.current_view,
        );

        let accumulate_bind_group = Self::create_accumulate_bind_group(
            device,
            &accumulate_bind_group_layout,
            &accumulation,
            &params_buffer,
        );

        let tonemap_bind_group = Self::create_tonemap_bind_group(
            device,
            &tonemap_bind_group_layout,
            &accumulation.output_view,
            &tonemap_sampler,
            &params_buffer,
        );

        // Simple mode tonemap reads current_texture directly (no copy needed)
        let tonemap_bind_group_simple = Self::create_tonemap_bind_group(
            device,
            &tonemap_bind_group_layout,
            &accumulation.current_view,
            &tonemap_sampler,
            &params_buffer,
        );

        // Create 3 denoise bind groups for multi-pass denoising
        // Pass 0: accum.output -> ping, Pass 1: ping -> pong, Pass 2: pong -> ping
        let denoise_bind_groups = Self::create_denoise_bind_groups_multipass(
            device,
            &denoise_bind_group_layout,
            &denoise_params_buffers,
            &gbuffer,
            &accumulation.output_view,
            &denoise_ping_view,
            &denoise_pong_view,
        );

        Self {
            gbuffer,
            accumulation,
            params,
            params_buffer,
            materials_buffer,
            voxel_texture,
            voxel_view,
            gbuffer_pipeline,
            pathtrace_pipeline,
            pathtrace_bind_group_layout,
            pathtrace_bind_group,
            direct_pipeline,
            direct_bind_group,
            accumulate_pipeline,
            accumulate_bind_group_layout,
            accumulate_bind_group,
            denoise_bind_group_layout,
            denoise_params_buffers,
            denoise_ping_texture,
            denoise_ping_view,
            denoise_pong_texture,
            denoise_pong_view,
            denoise_bind_groups,
            tonemap_pipeline,
            tonemap_bind_group_layout,
            tonemap_bind_group,
            tonemap_bind_group_simple,
            tonemap_sampler,
            depth_texture,
            depth_view,
            frame_index: 0,
            accumulated_frames: 0,
            prev_view_proj: Mat4::IDENTITY,
            width,
            height,
            render_scale,
            internal_width,
            internal_height,
            world_size: (VOLUME_SIZE, VOLUME_SIZE, VOLUME_SIZE),
            world_origin: (0, 0, 0),
            volume_chunk_min: (i32::MIN, i32::MIN, i32::MIN),
        }
    }

    fn create_gbuffer_pipeline(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
        use crate::renderer::Vertex;

        // G-buffer doesn't need extra bind group (uses camera + texture)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GBuffer Bind Group Layout"),
            entries: &[],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_gbuffer.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GBuffer Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout, texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GBuffer Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[
                    // Position + depth
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba32Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    // Normal + material_id
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    // Albedo
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        (bind_group_layout, pipeline)
    }

    fn create_pathtrace_pipeline(
        device: &wgpu::Device,
        character_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PathTrace Bind Group Layout"),
            entries: &[
                // Params uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Materials buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Voxel texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D3,
                        sample_type: wgpu::TextureSampleType::Uint,
                    },
                    count: None,
                },
                // G-buffer position
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // G-buffer normal
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // G-buffer albedo
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Output texture (write)
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_pathtrace.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PathTrace Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, character_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("PathTrace Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        (bind_group_layout, pipeline)
    }

    fn create_direct_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        character_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_direct.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Direct Lighting Pipeline Layout"),
            bind_group_layouts: &[bind_group_layout, character_bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Direct Lighting Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    fn create_accumulate_pipeline(
        device: &wgpu::Device,
    ) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Accumulate Bind Group Layout"),
            entries: &[
                // Params uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Current frame (read only)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // History (read only)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Output (write only)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_accumulate.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Accumulate Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Accumulate Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        (bind_group_layout, pipeline)
    }

    fn create_denoise_pipeline(
        device: &wgpu::Device,
    ) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Denoise Bind Group Layout"),
            entries: &[
                // Params uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Color input
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Normal
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Position
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Output
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_denoise.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Denoise Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Denoise Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        (bind_group_layout, pipeline)
    }

    fn create_denoise_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let create_texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };

        let ping = create_texture("Denoise Ping");
        let pong = create_texture("Denoise Pong");
        let ping_view = ping.create_view(&wgpu::TextureViewDescriptor::default());
        let pong_view = pong.create_view(&wgpu::TextureViewDescriptor::default());

        (ping, ping_view, pong, pong_view)
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("PT Internal Depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn create_denoise_bind_groups_multipass(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        params_buffers: &[wgpu::Buffer; 3],
        gbuffer: &GBuffer,
        accum_output_view: &wgpu::TextureView,
        ping_view: &wgpu::TextureView,
        pong_view: &wgpu::TextureView,
    ) -> [wgpu::BindGroup; 3] {
        // Pass 0: accum.output -> ping (step 1)
        // Pass 1: ping -> pong (step 2)
        // Pass 2: pong -> ping (step 4)
        // Final result in ping

        let create_bind_group = |label: &str,
                                 params: &wgpu::Buffer,
                                 input: &wgpu::TextureView,
                                 output: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&gbuffer.normal_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&gbuffer.position_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(output),
                    },
                ],
            })
        };

        [
            create_bind_group(
                "Denoise Pass 0 (accum->ping)",
                &params_buffers[0],
                accum_output_view,
                ping_view,
            ),
            create_bind_group(
                "Denoise Pass 1 (ping->pong)",
                &params_buffers[1],
                ping_view,
                pong_view,
            ),
            create_bind_group(
                "Denoise Pass 2 (pong->ping)",
                &params_buffers[2],
                pong_view,
                ping_view,
            ),
        ]
    }

    fn create_tonemap_pipeline(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tonemap Bind Group Layout"),
            entries: &[
                // Input texture (Rgba32Float is not filterable)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // Sampler (non-filtering)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                // Params
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../pt_tonemap.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tonemap Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tonemap Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        (bind_group_layout, pipeline)
    }

    fn create_pathtrace_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        gbuffer: &GBuffer,
        params_buffer: &wgpu::Buffer,
        materials_buffer: &wgpu::Buffer,
        voxel_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PathTrace Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: materials_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(voxel_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.position_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&gbuffer.albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        })
    }

    fn create_accumulate_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        accumulation: &AccumulationBuffer,
        params_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Accumulate Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&accumulation.current_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&accumulation.history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&accumulation.output_view),
                },
            ],
        })
    }

    fn create_tonemap_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        input_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        params_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tonemap Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.width = width;
        self.height = height;
        self.internal_width = ((width as f32 * self.render_scale) as u32).max(1);
        self.internal_height = ((height as f32 * self.render_scale) as u32).max(1);

        let iw = self.internal_width;
        let ih = self.internal_height;

        // Recreate G-buffer at internal resolution
        self.gbuffer = GBuffer::new(device, iw, ih);

        // Recreate accumulation buffers at internal resolution
        self.accumulation = AccumulationBuffer::new(device, iw, ih);

        // Recreate internal depth buffer
        let (dt, dv) = Self::create_depth_texture(device, iw, ih);
        self.depth_texture = dt;
        self.depth_view = dv;

        // Recreate bind groups
        self.pathtrace_bind_group = Self::create_pathtrace_bind_group(
            device,
            &self.pathtrace_bind_group_layout,
            &self.gbuffer,
            &self.params_buffer,
            &self.materials_buffer,
            &self.voxel_view,
            &self.accumulation.current_view,
        );

        self.direct_bind_group = Self::create_pathtrace_bind_group(
            device,
            &self.pathtrace_bind_group_layout,
            &self.gbuffer,
            &self.params_buffer,
            &self.materials_buffer,
            &self.voxel_view,
            &self.accumulation.current_view,
        );

        self.accumulate_bind_group = Self::create_accumulate_bind_group(
            device,
            &self.accumulate_bind_group_layout,
            &self.accumulation,
            &self.params_buffer,
        );

        self.tonemap_bind_group = Self::create_tonemap_bind_group(
            device,
            &self.tonemap_bind_group_layout,
            &self.accumulation.output_view,
            &self.tonemap_sampler,
            &self.params_buffer,
        );

        self.tonemap_bind_group_simple = Self::create_tonemap_bind_group(
            device,
            &self.tonemap_bind_group_layout,
            &self.accumulation.current_view,
            &self.tonemap_sampler,
            &self.params_buffer,
        );

        // Recreate denoise buffers at internal resolution
        let (ping, ping_view, pong, pong_view) = Self::create_denoise_textures(device, iw, ih);
        self.denoise_ping_texture = ping;
        self.denoise_ping_view = ping_view;
        self.denoise_pong_texture = pong;
        self.denoise_pong_view = pong_view;

        // Recreate denoise params buffers with new dimensions
        let step_sizes: [u32; 3] = [1, 2, 4];
        self.denoise_params_buffers = std::array::from_fn(|i| {
            let params = DenoiseParams {
                screen_width: iw,
                screen_height: ih,
                step_size: step_sizes[i],
                _padding: 0,
            };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Denoise Params Buffer {}", i)),
                contents: bytemuck::cast_slice(&[params]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });

        // Recreate denoise bind groups
        self.denoise_bind_groups = Self::create_denoise_bind_groups_multipass(
            device,
            &self.denoise_bind_group_layout,
            &self.denoise_params_buffers,
            &self.gbuffer,
            &self.accumulation.output_view,
            &self.denoise_ping_view,
            &self.denoise_pong_view,
        );

        // Reset accumulation
        self.accumulated_frames = 0;
    }

    pub fn set_render_scale(&mut self, device: &wgpu::Device, width: u32, height: u32, scale: f32) {
        let scale = scale.clamp(0.5, 1.0);
        if (self.render_scale - scale).abs() < f32::EPSILON {
            return;
        }
        self.render_scale = scale;
        self.resize(device, width, height);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_params(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        sun_direction: [f32; 3],
        sun_intensity: f32,
        sun_color: [f32; 3],
        is_moving: bool,
        max_bounces: u32,
    ) {
        // Calculate view-projection matrix (must match G-buffer render)
        let view_proj = camera.view_projection_matrix();

        // Check if view changed by comparing matrices
        let matrix_changed = view_proj.to_cols_array_2d() != self.prev_view_proj.to_cols_array_2d();

        if matrix_changed || is_moving {
            self.accumulated_frames = 0;
        }

        self.prev_view_proj = view_proj;

        // Calculate camera vectors
        let camera_position = camera.position;
        let camera_forward = camera.forward();
        let right = camera.right();
        let up = camera_forward.cross(right).normalize();

        let inv_view_proj = view_proj.inverse();

        self.params = PathTracerParams {
            camera_position: camera_position.to_array(),
            frame_index: self.frame_index,
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            camera_up: up.to_array(),
            accumulated_frames: self.accumulated_frames,
            camera_right: right.to_array(),
            max_bounces,
            camera_forward: camera_forward.to_array(),
            voxel_size: VOXEL_SCALE,
            volume_min: [
                self.world_origin.0 as f32 * VOXEL_SCALE,
                self.world_origin.1 as f32 * VOXEL_SCALE,
                self.world_origin.2 as f32 * VOXEL_SCALE,
            ],
            screen_width: self.internal_width,
            volume_max: [
                (self.world_origin.0 + self.world_size.0 as i32) as f32 * VOXEL_SCALE,
                (self.world_origin.1 + self.world_size.1 as i32) as f32 * VOXEL_SCALE,
                (self.world_origin.2 + self.world_size.2 as i32) as f32 * VOXEL_SCALE,
            ],
            screen_height: self.internal_height,
            sun_direction,
            sun_intensity,
            sun_color,
            _padding: 0,
        };

        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));

        self.frame_index = self.frame_index.wrapping_add(1);
        self.accumulated_frames += 1;
    }

    /// Upload a single chunk's voxel data to its toroidal position in the volume texture.
    /// Since VOLUME_SIZE=256 and CHUNK_SIZE=32, each chunk maps to a clean 32³ sub-region
    /// at texture offset (chunk_coord % 8) * 32 — no wrapping across texture edges.
    #[allow(clippy::needless_range_loop)]
    fn upload_chunk_to_volume(
        &self,
        queue: &wgpu::Queue,
        world: &ChunkManager,
        chunk_pos: ChunkPosition,
    ) {
        let cpv = CHUNKS_PER_VOL_AXIS;
        let cs = CHUNK_SIZE as u32;

        // Toroidal texture offset for this chunk
        let tx = chunk_pos.x.rem_euclid(cpv) as u32 * cs;
        let ty = chunk_pos.y.rem_euclid(cpv) as u32 * cs;
        let tz = chunk_pos.z.rem_euclid(cpv) as u32 * cs;

        // Build 32³ buffer (zeros = air if chunk not loaded)
        let mut buf = [0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
        if let Some(chunk) = world.get_chunk(chunk_pos) {
            let data = chunk.data();
            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        buf[x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE] = data[x][y][z];
                    }
                }
            }
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.voxel_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: tx,
                    y: ty,
                    z: tz,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &buf,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cs),
                rows_per_image: Some(cs),
            },
            wgpu::Extent3d {
                width: cs,
                height: cs,
                depth_or_array_layers: cs,
            },
        );
    }

    /// Update the 3D voxel volume texture using toroidal addressing.
    /// When the camera moves to a new chunk, only the newly-visible chunks are uploaded
    /// (typically 1 slab = ~64 chunks instead of 512 for a full rebuild).
    pub fn update_voxel_volume(
        &mut self,
        queue: &wgpu::Queue,
        world: &ChunkManager,
        camera_pos: glam::Vec3,
    ) {
        let cs = CHUNK_SIZE as i32;
        let cpv = CHUNKS_PER_VOL_AXIS;
        let half = cpv / 2;

        // Camera chunk position (div_euclid for correct negative handling)
        let cam_vx = (camera_pos.x / VOXEL_SCALE).floor() as i32;
        let cam_vy = (camera_pos.y / VOXEL_SCALE).floor() as i32;
        let cam_vz = (camera_pos.z / VOXEL_SCALE).floor() as i32;
        let cam_cx = cam_vx.div_euclid(cs);
        let cam_cy = cam_vy.div_euclid(cs);
        let cam_cz = cam_vz.div_euclid(cs);

        let new_min = (cam_cx - half, cam_cy - half, cam_cz - half);

        if new_min == self.volume_chunk_min {
            return;
        }

        let old_min = self.volume_chunk_min;
        let is_first = old_min.0 == i32::MIN;
        self.volume_chunk_min = new_min;

        let new_max = (new_min.0 + cpv, new_min.1 + cpv, new_min.2 + cpv);

        if is_first
            || (new_min.0 - old_min.0).abs() >= cpv
            || (new_min.1 - old_min.1).abs() >= cpv
            || (new_min.2 - old_min.2).abs() >= cpv
        {
            // Full rebuild (first load or camera jumped far)
            for cx in new_min.0..new_max.0 {
                for cy in new_min.1..new_max.1 {
                    for cz in new_min.2..new_max.2 {
                        self.upload_chunk_to_volume(queue, world, ChunkPosition::new(cx, cy, cz));
                    }
                }
            }
        } else {
            // Incremental: only upload chunks that entered the volume
            let old_max = (old_min.0 + cpv, old_min.1 + cpv, old_min.2 + cpv);
            for cx in new_min.0..new_max.0 {
                for cy in new_min.1..new_max.1 {
                    for cz in new_min.2..new_max.2 {
                        if cx >= old_min.0
                            && cx < old_max.0
                            && cy >= old_min.1
                            && cy < old_max.1
                            && cz >= old_min.2
                            && cz < old_max.2
                        {
                            continue; // Already in old volume
                        }
                        self.upload_chunk_to_volume(queue, world, ChunkPosition::new(cx, cy, cz));
                    }
                }
            }
        }

        // Update volume bounds for shader (in voxel coordinates)
        self.world_origin = (new_min.0 * cs, new_min.1 * cs, new_min.2 * cs);
        self.world_size = (VOLUME_SIZE, VOLUME_SIZE, VOLUME_SIZE);
        // Note: no accumulated_frames reset here — camera movement is already
        // detected by update_params() via matrix_changed, which handles the reset.
    }

    /// Notify the path tracer that voxels were edited.
    /// Re-uploads only the affected chunks so shadows update immediately.
    pub fn notify_voxel_edits(
        &mut self,
        queue: &wgpu::Queue,
        world: &ChunkManager,
        edits: &[(i32, i32, i32, u8)],
    ) {
        let cs = CHUNK_SIZE as i32;
        let cpv = CHUNKS_PER_VOL_AXIS;
        let min = self.volume_chunk_min;
        if min.0 == i32::MIN {
            return; // Volume not initialized yet
        }
        let max = (min.0 + cpv, min.1 + cpv, min.2 + cpv);

        let mut seen = std::collections::HashSet::new();
        for &(wx, wy, wz, _) in edits {
            let cx = wx.div_euclid(cs);
            let cy = wy.div_euclid(cs);
            let cz = wz.div_euclid(cs);

            if cx >= min.0
                && cx < max.0
                && cy >= min.1
                && cy < max.1
                && cz >= min.2
                && cz < max.2
                && seen.insert((cx, cy, cz))
            {
                self.upload_chunk_to_volume(queue, world, ChunkPosition::new(cx, cy, cz));
            }
        }

        if !seen.is_empty() {
            self.accumulated_frames = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render<'a>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        camera_bind_group: &wgpu::BindGroup,
        texture_bind_group: &wgpu::BindGroup,
        character_bind_group: &wgpu::BindGroup,
        meshes: impl Iterator<Item = (&'a wgpu::Buffer, u32, u32)>,
        lighting_mode: LightingMode,
    ) {
        let iw = self.internal_width;
        let ih = self.internal_height;

        // Pass 1: G-buffer (at internal resolution)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GBuffer Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.gbuffer.position_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.gbuffer.normal_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.gbuffer.albedo_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            render_pass.set_pipeline(&self.gbuffer_pipeline);
            render_pass.set_bind_group(0, camera_bind_group, &[]);
            render_pass.set_bind_group(1, texture_bind_group, &[]);

            for (vertex_buffer, num_vertices, lod_level) in meshes {
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.draw(0..num_vertices, lod_level..lod_level + 1);
            }
        }

        // Pass 2: Lighting (compute at internal resolution)
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Lighting Pass"),
                timestamp_writes: None,
            });

            let workgroups_x = iw.div_ceil(8);
            let workgroups_y = ih.div_ceil(8);

            match lighting_mode {
                LightingMode::PathTracing => {
                    compute_pass.set_pipeline(&self.pathtrace_pipeline);
                    compute_pass.set_bind_group(0, &self.pathtrace_bind_group, &[]);
                    compute_pass.set_bind_group(1, character_bind_group, &[]);
                }
                LightingMode::Simple => {
                    compute_pass.set_pipeline(&self.direct_pipeline);
                    compute_pass.set_bind_group(0, &self.direct_bind_group, &[]);
                    compute_pass.set_bind_group(1, character_bind_group, &[]);
                }
            }

            compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        // Pass 3: Accumulation (compute) - only for path tracing
        // Simple mode skips accumulation and copy — tonemap reads current_texture directly
        if matches!(lighting_mode, LightingMode::PathTracing) {
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Accumulate Pass"),
                    timestamp_writes: None,
                });

                compute_pass.set_pipeline(&self.accumulate_pipeline);
                compute_pass.set_bind_group(0, &self.accumulate_bind_group, &[]);

                let workgroups_x = iw.div_ceil(8);
                let workgroups_y = ih.div_ceil(8);
                compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
            }

            // Swap accumulation buffers for next frame
            self.accumulation.swap(encoder, iw, ih);
        }

        // Tonemap to screen (full resolution output)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tonemap Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            render_pass.set_pipeline(&self.tonemap_pipeline);
            let bind_group = match lighting_mode {
                LightingMode::PathTracing => &self.tonemap_bind_group,
                LightingMode::Simple => &self.tonemap_bind_group_simple,
            };
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Full-screen triangle
        }
    }

    pub fn reset_accumulation(&mut self) {
        self.accumulated_frames = 0;
    }
}

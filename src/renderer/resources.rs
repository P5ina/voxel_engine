use wgpu::util::DeviceExt;

use crate::camera::{Camera, CameraUniform};
use crate::pathtracer::materials::{Material, Palette};

/// Camera resources (buffer and bind group)
pub struct CameraResources {
    pub uniform: CameraUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl CameraResources {
    pub fn new(device: &wgpu::Device, camera: &Camera) -> Self {
        let mut uniform = CameraUniform::new();
        uniform.update(camera);

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform,
            buffer,
            bind_group,
            bind_group_layout,
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, camera: &Camera) {
        self.uniform.update(camera);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }
}

/// Palette resources (256-color palette as storage buffer)
pub struct PaletteResources {
    /// Kept alive: data source for the GPU buffer.
    _palette: Palette,
    /// Kept alive: backs `bind_group`.
    _buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl PaletteResources {
    pub fn new(device: &wgpu::Device) -> Self {
        let palette = Palette::new();

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Palette Buffer"),
            contents: bytemuck::cast_slice(palette.as_slice()),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Palette Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new((256 * std::mem::size_of::<Material>()) as u64)
                            .unwrap(),
                    ),
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Palette Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            _palette: palette,
            _buffer: buffer,
            bind_group,
            bind_group_layout,
        }
    }
}

/// Mesh resources (vertex buffer)
pub struct MeshResources {
    pub vertex_buffer: wgpu::Buffer,
    pub num_vertices: u32,
}

impl MeshResources {
    pub fn new(device: &wgpu::Device, vertices: &[super::Vertex]) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            vertex_buffer,
            num_vertices: vertices.len() as u32,
        }
    }

    pub fn update(&mut self, device: &wgpu::Device, vertices: &[super::Vertex]) {
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        self.num_vertices = vertices.len() as u32;
    }
}

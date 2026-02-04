/// G-buffer textures for deferred path tracing
pub struct GBuffer {
    /// World position + linear depth (Rgba32Float)
    pub position_texture: wgpu::Texture,
    pub position_view: wgpu::TextureView,

    /// World normal (xyz) + material_id (w) (Rgba16Float)
    pub normal_texture: wgpu::Texture,
    pub normal_view: wgpu::TextureView,

    /// Albedo color from texture (Rgba8Unorm)
    pub albedo_texture: wgpu::Texture,
    pub albedo_view: wgpu::TextureView,
}

impl GBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        // Position texture (world position + depth)
        let position_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Position"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let position_view = position_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Normal texture (normal + material_id)
        let normal_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Normal"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Albedo texture (texture color)
        let albedo_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Albedo"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let albedo_view = albedo_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            position_texture,
            position_view,
            normal_texture,
            normal_view,
            albedo_texture,
            albedo_view,
        }
    }
}

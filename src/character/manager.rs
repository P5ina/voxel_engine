//! Character manager - handles all characters and GPU resources

use std::collections::HashMap;

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::bvh::builder::BvhBuilder;
use crate::bvh::gpu_data::{CharacterParams, GpuBvhNode, GpuTriangle};
use crate::bvh::node::Triangle;
use crate::character::first_person::FirstPersonView;
use crate::character::local_player::LocalPlayer;
use crate::character::remote_player::RemotePlayer;
use crate::model::{LoadedModel, SkeletonState};

/// Manages all characters and their GPU resources
pub struct CharacterManager {
    /// Local player (if any)
    pub local_player: Option<LocalPlayer>,
    /// Remote players by ID
    pub remote_players: HashMap<u64, RemotePlayer>,
    /// First-person view
    pub first_person: FirstPersonView,

    // GPU resources
    bvh_nodes_buffer: wgpu::Buffer,
    triangles_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    texture_array: wgpu::Texture,
    texture_view: wgpu::TextureView,
    texture_sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,

    // State
    params: CharacterParams,
    max_nodes: usize,
    max_triangles: usize,
    texture_size: u32,
    texture_layers: u32,
}

impl CharacterManager {
    /// Initial buffer sizes
    const INITIAL_MAX_NODES: usize = 4096;
    const INITIAL_MAX_TRIANGLES: usize = 8192;
    const INITIAL_TEXTURE_SIZE: u32 = 16;
    const INITIAL_TEXTURE_LAYERS: u32 = 16;

    pub fn new(device: &wgpu::Device) -> Self {
        let max_nodes = Self::INITIAL_MAX_NODES;
        let max_triangles = Self::INITIAL_MAX_TRIANGLES;
        let texture_size = Self::INITIAL_TEXTURE_SIZE;
        let texture_layers = Self::INITIAL_TEXTURE_LAYERS;

        // Create buffers
        let bvh_nodes_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Character BVH Nodes Buffer"),
            size: (max_nodes * std::mem::size_of::<GpuBvhNode>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let triangles_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Character Triangles Buffer"),
            size: (max_triangles * std::mem::size_of::<GpuTriangle>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params = CharacterParams::default();
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Character Params Buffer"),
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create texture array for character textures
        let texture_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Character Texture Array"),
            size: wgpu::Extent3d {
                width: texture_size,
                height: texture_size,
                depth_or_array_layers: texture_layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture_array.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Character Texture Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group layout
        let bind_group_layout = Self::create_bind_group_layout(device);

        // Create bind group
        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &bvh_nodes_buffer,
            &triangles_buffer,
            &params_buffer,
            &texture_view,
            &texture_sampler,
        );

        Self {
            local_player: None,
            remote_players: HashMap::new(),
            first_person: FirstPersonView::new(),
            bvh_nodes_buffer,
            triangles_buffer,
            params_buffer,
            texture_array,
            texture_view,
            texture_sampler,
            bind_group_layout,
            bind_group,
            params,
            max_nodes,
            max_triangles,
            texture_size,
            texture_layers,
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Character Bind Group Layout"),
            entries: &[
                // BVH nodes
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Triangles
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
                // Params
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Texture array
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bvh_nodes: &wgpu::Buffer,
        triangles: &wgpu::Buffer,
        params: &wgpu::Buffer,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Character Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: bvh_nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: triangles.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Get the bind group layout
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get the bind group
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Update all characters and rebuild GPU data
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dt: f32,
        is_walking: bool,
        is_sprinting: bool,
        camera_position: Vec3,
        camera_rotation: glam::Quat,
    ) {
        // Update local player animation
        if let Some(player) = &mut self.local_player {
            player.update(dt);
        }

        // Update remote players
        for player in self.remote_players.values_mut() {
            player.update(dt);
        }

        // Update first-person view
        self.first_person
            .update(dt, is_walking, is_sprinting, Vec3::ZERO);

        // Rebuild BVH with all character triangles
        self.rebuild_bvh(device, queue, camera_position, camera_rotation);
    }

    /// Rebuild BVH from all visible characters
    fn rebuild_bvh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_position: Vec3,
        camera_rotation: glam::Quat,
    ) {
        let mut builder = BvhBuilder::new();

        // Collect triangles from local player (with visibility check)
        if let Some(player) = &self.local_player {
            self.add_model_triangles(
                &mut builder,
                &player.model,
                player.skeleton_state.as_ref(),
                player.transform_matrix(),
                |mesh_idx| player.is_mesh_visible(mesh_idx),
            );
        }

        // Collect triangles from remote players
        for player in self.remote_players.values() {
            self.add_model_triangles(
                &mut builder,
                &player.model,
                player.skeleton_state.as_ref(),
                player.transform_matrix(),
                |_| true,
            );
        }

        // Collect triangles from first-person view
        if let Some(model) = &self.first_person.model {
            // Use view_transform to get correct positioning
            let fp_transform = self
                .first_person
                .view_transform(camera_position, camera_rotation);

            // Get hidden meshes for current item
            let hidden = &self.first_person.hidden_meshes;

            self.add_model_triangles_with_names(
                &mut builder,
                model,
                self.first_person.skeleton_state.as_ref(),
                fp_transform,
                |mesh_name| !hidden.iter().any(|h| mesh_name.contains(h)),
            );
        }

        // Build GPU data
        let (gpu_nodes, gpu_triangles) = builder.build_gpu();

        // Update params
        self.params.node_count = gpu_nodes.len() as u32;
        self.params.triangle_count = gpu_triangles.len() as u32;
        self.params.enabled = if gpu_triangles.is_empty() { 0 } else { 1 };

        // Resize buffers if needed
        if gpu_nodes.len() > self.max_nodes {
            self.max_nodes = gpu_nodes.len().next_power_of_two();
            self.bvh_nodes_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Character BVH Nodes Buffer"),
                size: (self.max_nodes * std::mem::size_of::<GpuBvhNode>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.recreate_bind_group(device);
        }

        if gpu_triangles.len() > self.max_triangles {
            self.max_triangles = gpu_triangles.len().next_power_of_two();
            self.triangles_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Character Triangles Buffer"),
                size: (self.max_triangles * std::mem::size_of::<GpuTriangle>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.recreate_bind_group(device);
        }

        // Upload data
        if !gpu_nodes.is_empty() {
            queue.write_buffer(&self.bvh_nodes_buffer, 0, bytemuck::cast_slice(&gpu_nodes));
        }

        if !gpu_triangles.is_empty() {
            queue.write_buffer(
                &self.triangles_buffer,
                0,
                bytemuck::cast_slice(&gpu_triangles),
            );
        }

        queue.write_buffer(&self.params_buffer, 0, bytemuck::cast_slice(&[self.params]));
    }

    fn recreate_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = Self::create_bind_group(
            device,
            &self.bind_group_layout,
            &self.bvh_nodes_buffer,
            &self.triangles_buffer,
            &self.params_buffer,
            &self.texture_view,
            &self.texture_sampler,
        );
    }

    /// Add triangles from a model to the BVH builder (by mesh index)
    fn add_model_triangles<F>(
        &self,
        builder: &mut BvhBuilder,
        model: &LoadedModel,
        skeleton_state: Option<&SkeletonState>,
        transform: glam::Mat4,
        visibility_check: F,
    ) where
        F: Fn(usize) -> bool,
    {
        for (mesh_idx, mesh) in model.meshes.iter().enumerate() {
            if !visibility_check(mesh_idx) {
                continue;
            }

            self.add_mesh_triangles(builder, mesh, skeleton_state, transform);
        }
    }

    /// Add triangles from a model to the BVH builder (by mesh name)
    fn add_model_triangles_with_names<F>(
        &self,
        builder: &mut BvhBuilder,
        model: &LoadedModel,
        skeleton_state: Option<&SkeletonState>,
        transform: glam::Mat4,
        visibility_check: F,
    ) where
        F: Fn(&str) -> bool,
    {
        for mesh in &model.meshes {
            if !visibility_check(&mesh.name) {
                continue;
            }

            self.add_mesh_triangles(builder, mesh, skeleton_state, transform);
        }
    }

    /// Add triangles from a single mesh to the BVH builder
    fn add_mesh_triangles(
        &self,
        builder: &mut BvhBuilder,
        mesh: &crate::model::PolygonMesh,
        skeleton_state: Option<&SkeletonState>,
        transform: glam::Mat4,
    ) {
        let texture_id = mesh.texture_index.unwrap_or(0) as u32;
        let material_id = mesh.material_index.unwrap_or(0) as u32;

        for (v0, v1, v2) in mesh.triangles() {
            // Apply skinning if available
            let (p0, p1, p2) = if let Some(state) = skeleton_state {
                let p0 = state.transform_vertex(
                    Vec3::from(v0.position),
                    v0.joint_indices,
                    v0.joint_weights,
                );
                let p1 = state.transform_vertex(
                    Vec3::from(v1.position),
                    v1.joint_indices,
                    v1.joint_weights,
                );
                let p2 = state.transform_vertex(
                    Vec3::from(v2.position),
                    v2.joint_indices,
                    v2.joint_weights,
                );
                (p0, p1, p2)
            } else {
                (
                    Vec3::from(v0.position),
                    Vec3::from(v1.position),
                    Vec3::from(v2.position),
                )
            };

            // Apply model transform
            let wp0 = transform.transform_point3(p0);
            let wp1 = transform.transform_point3(p1);
            let wp2 = transform.transform_point3(p2);

            // Triangle::new computes geometric normal from edges
            let mut tri = Triangle::new(wp0, wp1, wp2);
            tri.uv0 = v0.uv;
            tri.uv1 = v1.uv;
            tri.uv2 = v2.uv;
            tri.material_id = material_id;
            tri.texture_id = texture_id;

            builder.add_triangle(tri);
        }
    }

    /// Upload a texture to the texture array
    pub fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer: u32,
        data: &[u8],
        width: u32,
        height: u32,
    ) {
        // Resize texture array if needed
        if layer >= self.texture_layers || width > self.texture_size || height > self.texture_size {
            let new_layers = (layer + 1).max(self.texture_layers);
            let new_size = width.max(height).max(self.texture_size);

            self.texture_array = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Character Texture Array"),
                size: wgpu::Extent3d {
                    width: new_size,
                    height: new_size,
                    depth_or_array_layers: new_layers,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            self.texture_view = self
                .texture_array
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                });

            self.texture_size = new_size;
            self.texture_layers = new_layers;
            self.recreate_bind_group(device);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture_array,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

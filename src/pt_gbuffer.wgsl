// G-buffer shader for path tracing
// Outputs world position, normal, color_index, and albedo from palette

struct CameraUniform {
    view_proj: mat4x4<f32>,
    world_position: vec3<f32>,
    _padding: f32,
};

// Material from 256-color palette
struct Material {
    albedo: vec3<f32>,
    roughness: f32,
    emission: vec3<f32>,
    metallic: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Palette of 256 materials
@group(1) @binding(0)
var<storage, read> palette: array<Material, 256>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) ao: f32,
    @location(3) color_index: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) ao: f32,
    @location(3) color_index: f32,
    @location(4) @interpolate(flat) lod_level: u32,
};

struct GBufferOutput {
    @location(0) position: vec4<f32>,  // world position + linear depth
    @location(1) normal: vec4<f32>,    // normal xyz + color_index
    @location(2) albedo: vec4<f32>,    // palette color * ao
};

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.world_normal = in.normal;
    out.ao = in.ao;
    out.color_index = in.color_index;
    out.lod_level = instance_index;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> GBufferOutput {
    var out: GBufferOutput;

    // Get color from palette using color_index
    let idx = u32(in.color_index);
    let material = palette[idx];

    // Calculate linear depth (distance from camera)
    let depth = length(in.world_position - camera.world_position);

    // Apply debug LOD tint (lod_level passed via instance_index)
    var albedo = material.albedo * in.ao;
    if (in.lod_level == 1u) {
        albedo = albedo * vec3<f32>(0.6, 1.0, 0.6);  // green
    } else if (in.lod_level == 2u) {
        albedo = albedo * vec3<f32>(0.6, 0.6, 1.0);  // blue
    } else if (in.lod_level == 3u) {
        albedo = albedo * vec3<f32>(1.0, 0.6, 0.6);  // red
    }

    // Output G-buffer
    out.position = vec4<f32>(in.world_position, depth);
    out.normal = vec4<f32>(normalize(in.world_normal), in.color_index);
    out.albedo = vec4<f32>(albedo, 1.0);

    return out;
}

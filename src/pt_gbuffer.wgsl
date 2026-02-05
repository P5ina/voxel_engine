// G-buffer shader for path tracing
// Outputs world position, normal, material_id, and albedo

struct CameraUniform {
    view_proj: mat4x4<f32>,
    world_position: vec3<f32>,
    _padding: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_atlas: texture_2d<f32>;
@group(1) @binding(1)
var s_atlas: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) ao: f32,
    @location(4) material_id: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) ao: f32,
    @location(4) material_id: f32,
};

struct GBufferOutput {
    @location(0) position: vec4<f32>,  // world position + linear depth
    @location(1) normal: vec4<f32>,    // normal xyz + material_id
    @location(2) albedo: vec4<f32>,    // texture color
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.world_normal = in.normal;
    out.uv = in.uv;
    out.ao = in.ao;
    out.material_id = in.material_id;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> GBufferOutput {
    var out: GBufferOutput;

    // Sample texture
    let tex_color = textureSample(t_atlas, s_atlas, in.uv);

    // Calculate linear depth (distance from camera)
    let depth = length(in.world_position - camera.world_position);

    // Use material_id from vertex data
    let material_id = in.material_id;

    // Output G-buffer
    out.position = vec4<f32>(in.world_position, depth);
    out.normal = vec4<f32>(normalize(in.world_normal), material_id);
    out.albedo = vec4<f32>(tex_color.rgb * in.ao, tex_color.a);

    return out;
}

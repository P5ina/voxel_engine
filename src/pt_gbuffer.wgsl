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
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) ao: f32,
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
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> GBufferOutput {
    var out: GBufferOutput;

    // Sample texture
    let tex_color = textureSample(t_atlas, s_atlas, in.uv);

    // Calculate linear depth (distance from camera)
    let depth = length(in.world_position - camera.world_position);

    // Determine material ID based on texture coordinates
    // The atlas is 2x2, so we can determine block type from UV
    // UV (0-0.5, 0-0.5) = Grass top, (0.5-1, 0-0.5) = Stone
    // UV (0-0.5, 0.5-1) = Dirt, (0.5-1, 0.5-1) = Grass side
    var material_id: f32 = 2.0; // Default to stone

    // Simple heuristic based on normal direction and UV
    let n = normalize(in.world_normal);
    if n.y > 0.5 {
        // Top face - likely grass
        material_id = 3.0;
    } else if n.y < -0.5 {
        // Bottom face - dirt
        material_id = 1.0;
    } else {
        // Side face - check texture
        // Grass side or stone based on green channel
        if tex_color.g > tex_color.r && tex_color.g > 0.3 {
            material_id = 3.0; // Grass
        } else if tex_color.r > tex_color.g && tex_color.r > 0.4 {
            material_id = 1.0; // Dirt
        } else {
            material_id = 2.0; // Stone
        }
    }

    // Output G-buffer
    out.position = vec4<f32>(in.world_position, depth);
    out.normal = vec4<f32>(normalize(in.world_normal), material_id);
    out.albedo = vec4<f32>(tex_color.rgb * in.ao, tex_color.a);

    return out;
}

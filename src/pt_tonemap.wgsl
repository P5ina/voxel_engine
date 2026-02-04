// Tonemapping shader with ACES and gamma correction

struct PathTracerParams {
    camera_position: vec3<f32>,
    frame_index: u32,
    inv_view_proj: mat4x4<f32>,
    camera_up: vec3<f32>,
    accumulated_frames: u32,
    camera_right: vec3<f32>,
    max_bounces: u32,
    camera_forward: vec3<f32>,
    voxel_size: f32,
    volume_min: vec3<f32>,
    screen_width: u32,
    volume_max: vec3<f32>,
    screen_height: u32,
    sun_direction: vec3<f32>,
    sun_intensity: f32,
    sun_color: vec3<f32>,
    _padding: u32,
};

@group(0) @binding(0)
var t_input: texture_2d<f32>;

@group(0) @binding(1)
var s_input: sampler;

@group(0) @binding(2)
var<uniform> params: PathTracerParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Full-screen triangle
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate full-screen triangle
    // Vertex 0: (-1, -1), Vertex 1: (3, -1), Vertex 2: (-1, 3)
    let x = f32((vertex_index & 1u) << 2u) - 1.0;
    let y = f32((vertex_index & 2u) << 1u) - 1.0;

    out.position = vec4<f32>(x, -y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (y + 1.0) * 0.5);

    return out;
}

// ACES filmic tonemapping
// https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

// sRGB gamma correction
fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let cutoff = linear < vec3<f32>(0.0031308);
    let higher = vec3<f32>(1.055) * pow(linear, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    let lower = linear * vec3<f32>(12.92);
    return select(higher, lower, cutoff);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Load accumulated color (use textureLoad for non-filterable format)
    let tex_dims = textureDimensions(t_input);
    let pixel = vec2<i32>(in.uv * vec2<f32>(f32(tex_dims.x), f32(tex_dims.y)));
    let hdr_color = textureLoad(t_input, pixel, 0).rgb;

    // Apply exposure
    let exposed = hdr_color * 1.0;

    // ACES tonemapping
    let tonemapped = aces_tonemap(exposed);

    // Gamma correction (linear to sRGB)
    let srgb = linear_to_srgb(tonemapped);

    return vec4<f32>(srgb, 1.0);
}

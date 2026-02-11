// Temporal accumulation shader
// Blends current frame with history for progressive refinement

struct PathTracerParams {
    camera_position: vec3<f32>,
    frame_index: u32,
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
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
var<uniform> params: PathTracerParams;

@group(0) @binding(1)
var current: texture_2d<f32>;

@group(0) @binding(2)
var history: texture_2d<f32>;

@group(0) @binding(3)
var output: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = global_id.xy;

    if pixel.x >= params.screen_width || pixel.y >= params.screen_height {
        return;
    }

    let current_color = textureLoad(current, vec2<i32>(pixel), 0);
    let history_color = textureLoad(history, vec2<i32>(pixel), 0);

    // Progressive accumulation
    // weight = 1 / (accumulated_frames + 1)
    // For first frame (accumulated_frames = 0): weight = 1.0 (use only current)
    // For subsequent frames: blend with decreasing weight for new samples

    let weight = 1.0 / f32(params.accumulated_frames + 1u);

    // Blend current with history
    let accumulated = mix(history_color, current_color, weight);

    // Write to output buffer
    textureStore(output, pixel, accumulated);
}

// A-Trous Wavelet Filter for path tracing denoising
// Edge-preserving blur using normal and position data

struct DenoiseParams {
    screen_width: u32,
    screen_height: u32,
    step_size: u32,  // 1, 2, 4, 8, 16 for each pass
    _padding: u32,
}

@group(0) @binding(0)
var<uniform> params: DenoiseParams;

@group(0) @binding(1)
var t_color: texture_2d<f32>;

@group(0) @binding(2)
var t_normal: texture_2d<f32>;

@group(0) @binding(3)
var t_position: texture_2d<f32>;

@group(0) @binding(4)
var t_output: texture_storage_2d<rgba32float, write>;

// A-Trous 5x5 kernel weights (B3 spline)
const KERNEL_WEIGHTS: array<f32, 25> = array<f32, 25>(
    1.0/256.0,  4.0/256.0,  6.0/256.0,  4.0/256.0, 1.0/256.0,
    4.0/256.0, 16.0/256.0, 24.0/256.0, 16.0/256.0, 4.0/256.0,
    6.0/256.0, 24.0/256.0, 36.0/256.0, 24.0/256.0, 6.0/256.0,
    4.0/256.0, 16.0/256.0, 24.0/256.0, 16.0/256.0, 4.0/256.0,
    1.0/256.0,  4.0/256.0,  6.0/256.0,  4.0/256.0, 1.0/256.0,
);

// Kernel offsets (relative to center)
const KERNEL_OFFSETS: array<vec2<i32>, 25> = array<vec2<i32>, 25>(
    vec2<i32>(-2, -2), vec2<i32>(-1, -2), vec2<i32>(0, -2), vec2<i32>(1, -2), vec2<i32>(2, -2),
    vec2<i32>(-2, -1), vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1), vec2<i32>(2, -1),
    vec2<i32>(-2,  0), vec2<i32>(-1,  0), vec2<i32>(0,  0), vec2<i32>(1,  0), vec2<i32>(2,  0),
    vec2<i32>(-2,  1), vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1), vec2<i32>(2,  1),
    vec2<i32>(-2,  2), vec2<i32>(-1,  2), vec2<i32>(0,  2), vec2<i32>(1,  2), vec2<i32>(2,  2),
);

// Edge-stopping parameters (lower = more edge preservation)
const SIGMA_POSITION: f32 = 0.1;  // Position sensitivity (was 0.5)
const SIGMA_COLOR: f32 = 0.05;    // Color sensitivity (was 0.2)

fn edge_stopping_normal(n1: vec3<f32>, n2: vec3<f32>) -> f32 {
    let d = max(0.0, dot(n1, n2));
    // Very aggressive edge detection - only blur nearly identical normals
    return pow(d, 256.0);
}

fn edge_stopping_position(p1: vec3<f32>, p2: vec3<f32>, n1: vec3<f32>) -> f32 {
    let diff = p1 - p2;
    // Use plane distance (perpendicular to normal)
    let plane_dist = abs(dot(diff, n1));
    // Also consider absolute distance
    let abs_dist = length(diff);
    // Reject if too far in any direction
    return exp(-plane_dist * plane_dist / (SIGMA_POSITION * SIGMA_POSITION))
         * exp(-abs_dist * abs_dist / 4.0);
}

fn edge_stopping_color(c1: vec3<f32>, c2: vec3<f32>) -> f32 {
    let diff = c1 - c2;
    let dist = dot(diff, diff);
    return exp(-dist / (SIGMA_COLOR * SIGMA_COLOR));
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = vec2<i32>(global_id.xy);

    if u32(pixel.x) >= params.screen_width || u32(pixel.y) >= params.screen_height {
        return;
    }

    // Load center pixel data
    let center_color = textureLoad(t_color, pixel, 0).rgb;
    let center_normal_data = textureLoad(t_normal, pixel, 0);
    let center_position_data = textureLoad(t_position, pixel, 0);

    // Check if sky pixel (no geometry)
    if center_position_data.w == 0.0 {
        textureStore(t_output, pixel, vec4<f32>(center_color, 1.0));
        return;
    }

    let center_normal = normalize(center_normal_data.xyz);
    let center_position = center_position_data.xyz;

    var sum_color = vec3<f32>(0.0);
    var sum_weight = 0.0;

    let step = i32(params.step_size);

    // Apply 5x5 A-Trous filter
    for (var i = 0; i < 25; i++) {
        let offset = KERNEL_OFFSETS[i] * step;
        let sample_pixel = pixel + offset;

        // Bounds check
        if sample_pixel.x < 0 || sample_pixel.y < 0 ||
           u32(sample_pixel.x) >= params.screen_width ||
           u32(sample_pixel.y) >= params.screen_height {
            continue;
        }

        let sample_color = textureLoad(t_color, sample_pixel, 0).rgb;
        let sample_normal_data = textureLoad(t_normal, sample_pixel, 0);
        let sample_position_data = textureLoad(t_position, sample_pixel, 0);

        // Skip sky pixels
        if sample_position_data.w == 0.0 {
            continue;
        }

        let sample_normal = normalize(sample_normal_data.xyz);
        let sample_position = sample_position_data.xyz;

        // Calculate edge-stopping weights
        let w_normal = edge_stopping_normal(center_normal, sample_normal);
        let w_position = edge_stopping_position(center_position, sample_position, center_normal);
        let w_color = edge_stopping_color(center_color, sample_color);

        // Combined weight
        let kernel_weight = KERNEL_WEIGHTS[i];
        let weight = kernel_weight * w_normal * w_position * w_color;

        sum_color += sample_color * weight;
        sum_weight += weight;
    }

    // Normalize
    var result = center_color;
    if sum_weight > 0.001 {
        result = sum_color / sum_weight;
    }

    textureStore(t_output, pixel, vec4<f32>(result, 1.0));
}

// Simple direct lighting shader (sun + ambient)
// Fast alternative to path tracing

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
var<uniform> params: PathTracerParams;

@group(0) @binding(1)
var<storage, read> materials: array<vec4<f32>>;

@group(0) @binding(2)
var t_voxels: texture_3d<u32>;

@group(0) @binding(3)
var t_position: texture_2d<f32>;

@group(0) @binding(4)
var t_normal: texture_2d<f32>;

@group(0) @binding(5)
var t_albedo: texture_2d<f32>;

@group(0) @binding(6)
var output: texture_storage_2d<rgba32float, write>;

// Check if position is inside the voxel volume
fn in_bounds(pos: vec3<i32>) -> bool {
    let size = vec3<i32>(textureDimensions(t_voxels));
    return all(pos >= vec3<i32>(0)) && all(pos < size);
}

// Get voxel at position
fn get_voxel(pos: vec3<i32>) -> u32 {
    if !in_bounds(pos) {
        return 0u;
    }
    return textureLoad(t_voxels, pos, 0).r;
}

// Simple shadow ray using DDA
fn trace_shadow_ray(origin: vec3<f32>, direction: vec3<f32>, max_dist: f32) -> f32 {
    let inv_dir = 1.0 / direction;
    let sign_dir = sign(direction);

    var pos = vec3<i32>(floor(origin / params.voxel_size));
    let step = vec3<i32>(sign_dir);
    let t_delta = abs(inv_dir) * params.voxel_size;

    var t_max: vec3<f32>;
    let frac = fract(origin / params.voxel_size);

    if direction.x > 0.0 {
        t_max.x = (1.0 - frac.x) * params.voxel_size * abs(inv_dir.x);
    } else {
        t_max.x = frac.x * params.voxel_size * abs(inv_dir.x);
    }
    if direction.y > 0.0 {
        t_max.y = (1.0 - frac.y) * params.voxel_size * abs(inv_dir.y);
    } else {
        t_max.y = frac.y * params.voxel_size * abs(inv_dir.y);
    }
    if direction.z > 0.0 {
        t_max.z = (1.0 - frac.z) * params.voxel_size * abs(inv_dir.z);
    } else {
        t_max.z = frac.z * params.voxel_size * abs(inv_dir.z);
    }

    var t = 0.0;

    for (var i = 0; i < 128; i++) {
        if t_max.x < t_max.y && t_max.x < t_max.z {
            t = t_max.x;
            t_max.x += t_delta.x;
            pos.x += step.x;
        } else if t_max.y < t_max.z {
            t = t_max.y;
            t_max.y += t_delta.y;
            pos.y += step.y;
        } else {
            t = t_max.z;
            t_max.z += t_delta.z;
            pos.z += step.z;
        }

        if t > max_dist {
            return 1.0; // No occlusion
        }

        if !in_bounds(pos) {
            return 1.0; // Out of bounds = no occlusion
        }

        if get_voxel(pos) != 0u {
            return 0.0; // Hit something = shadow
        }
    }

    return 1.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = global_id.xy;

    if pixel.x >= params.screen_width || pixel.y >= params.screen_height {
        return;
    }

    // Read G-buffer
    let pos_data = textureLoad(t_position, vec2<i32>(pixel), 0);
    let normal_data = textureLoad(t_normal, vec2<i32>(pixel), 0);
    let albedo_data = textureLoad(t_albedo, vec2<i32>(pixel), 0);

    // Check if this pixel hit geometry
    if pos_data.w == 0.0 {
        // Sky
        let uv = vec2<f32>(f32(pixel.x), f32(pixel.y)) / vec2<f32>(f32(params.screen_width), f32(params.screen_height));
        let sky_color = mix(
            vec3<f32>(0.4, 0.6, 0.9),
            vec3<f32>(0.7, 0.85, 1.0),
            1.0 - uv.y
        );
        textureStore(output, pixel, vec4<f32>(sky_color, 1.0));
        return;
    }

    let world_pos = pos_data.xyz;
    let world_normal = normalize(normal_data.xyz);
    let albedo = albedo_data.rgb;

    // Ambient light
    let ambient = vec3<f32>(0.15);

    // Sun direct lighting
    let sun_dir = normalize(params.sun_direction);
    let n_dot_l = max(dot(world_normal, sun_dir), 0.0);

    var direct = vec3<f32>(0.0);
    if n_dot_l > 0.0 {
        // Shadow ray
        let shadow_origin = world_pos + world_normal * 0.02;
        let shadow = trace_shadow_ray(shadow_origin, sun_dir, 100.0);
        direct = params.sun_color * params.sun_intensity * n_dot_l * shadow;
    }

    // Simple hemisphere ambient occlusion approximation
    // Just darken surfaces facing down
    let ao = 0.5 + 0.5 * world_normal.y;

    // Final color
    let color = albedo * (ambient * ao + direct);

    textureStore(output, pixel, vec4<f32>(color, 1.0));
}

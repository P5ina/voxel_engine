// Simple direct lighting shader (sun + ambient)
// Fast alternative to path tracing
// Supports hybrid tracing: voxels + polygonal characters

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

// BVH node structure (32 bytes)
struct BvhNode {
    bounds_min: vec3<f32>,
    left_or_first: u32,
    bounds_max: vec3<f32>,
    right_or_count: u32,
};

// Triangle structure (64 bytes)
struct GpuTriangle {
    v0: vec3<f32>,
    _pad0: f32,
    edge1: vec3<f32>,
    _pad1: f32,
    edge2: vec3<f32>,
    _pad2: f32,
    normal: vec3<f32>,
    material_id: u32,
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    uv2: vec2<f32>,
    texture_id: u32,
    _pad3: u32,
};

// Character parameters
struct CharacterParams {
    node_count: u32,
    triangle_count: u32,
    enabled: u32,
    _padding: u32,
};

// Hit info for primary rays
struct CharHitInfo {
    hit: bool,
    t: f32,
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    texture_id: u32,
};

// Group 0: Voxel tracing resources
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

// Group 1: Character tracing resources
@group(1) @binding(0)
var<storage, read> bvh_nodes: array<BvhNode>;

@group(1) @binding(1)
var<storage, read> triangles: array<GpuTriangle>;

@group(1) @binding(2)
var<uniform> char_params: CharacterParams;

@group(1) @binding(3)
var char_textures: texture_2d_array<f32>;

@group(1) @binding(4)
var char_sampler: sampler;

// BVH constants
const BVH_LEAF_FLAG: u32 = 0x80000000u;

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

// Ray-AABB intersection for BVH
fn ray_aabb_intersect(origin: vec3<f32>, direction: vec3<f32>, bounds_min: vec3<f32>, bounds_max: vec3<f32>, t_max: f32) -> f32 {
    let inv_dir = 1.0 / direction;

    let t1 = (bounds_min - origin) * inv_dir;
    let t2 = (bounds_max - origin) * inv_dir;

    let t_near = min(t1, t2);
    let t_far = max(t1, t2);

    let t_enter = max(max(t_near.x, t_near.y), t_near.z);
    let t_exit = min(min(t_far.x, t_far.y), t_far.z);

    if t_enter > t_exit || t_exit < 0.0 || t_enter > t_max {
        return -1.0;
    }

    return max(t_enter, 0.0);
}

// Moller-Trumbore ray-triangle intersection - returns (t, u, v)
fn ray_triangle_intersect_full(origin: vec3<f32>, direction: vec3<f32>, tri_idx: u32) -> vec3<f32> {
    let tri = triangles[tri_idx];
    let edge1 = tri.edge1;
    let edge2 = tri.edge2;

    let h = cross(direction, edge2);
    let a = dot(edge1, h);

    if abs(a) < 0.0001 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let f = 1.0 / a;
    let s = origin - tri.v0;
    let u = f * dot(s, h);

    if u < 0.0 || u > 1.0 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let q = cross(s, edge1);
    let v = f * dot(direction, q);

    if v < 0.0 || u + v > 1.0 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let t = f * dot(edge2, q);

    if t > 0.0001 {
        return vec3<f32>(t, u, v);
    }

    return vec3<f32>(-1.0, 0.0, 0.0);
}

// Simple ray-triangle for shadow (just returns t)
fn ray_triangle_intersect(origin: vec3<f32>, direction: vec3<f32>, tri_idx: u32) -> f32 {
    return ray_triangle_intersect_full(origin, direction, tri_idx).x;
}

// BVH traversal for primary rays - returns full hit info
fn trace_bvh_primary(origin: vec3<f32>, direction: vec3<f32>, max_dist: f32) -> CharHitInfo {
    var result: CharHitInfo;
    result.hit = false;
    result.t = max_dist;

    if char_params.enabled == 0u || char_params.node_count == 0u {
        return result;
    }

    var stack: array<u32, 32>;
    var stack_ptr = 0;

    stack[0] = 0u;
    stack_ptr = 1;

    var closest_t = max_dist;

    while stack_ptr > 0 {
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr];

        if node_idx >= char_params.node_count {
            continue;
        }

        let node = bvh_nodes[node_idx];

        let t_aabb = ray_aabb_intersect(origin, direction, node.bounds_min, node.bounds_max, closest_t);
        if t_aabb < 0.0 {
            continue;
        }

        let is_leaf = (node.right_or_count & BVH_LEAF_FLAG) != 0u;

        if is_leaf {
            let first_tri = node.left_or_first;
            let tri_count = node.right_or_count & (~BVH_LEAF_FLAG);

            for (var i = 0u; i < tri_count; i++) {
                let tri_idx = first_tri + i;
                if tri_idx >= char_params.triangle_count {
                    continue;
                }

                let hit_data = ray_triangle_intersect_full(origin, direction, tri_idx);
                let t = hit_data.x;

                if t > 0.0 && t < closest_t {
                    closest_t = t;
                    result.hit = true;
                    result.t = t;
                    result.position = origin + direction * t;

                    let tri = triangles[tri_idx];
                    result.normal = tri.normal;

                    // Interpolate UV
                    let u = hit_data.y;
                    let v = hit_data.z;
                    let w = 1.0 - u - v;
                    result.uv = tri.uv0 * w + tri.uv1 * u + tri.uv2 * v;
                    result.texture_id = tri.texture_id;
                }
            }
        } else {
            let left = node.left_or_first;
            let right = node.right_or_count;

            if stack_ptr < 31 && left < char_params.node_count {
                stack[stack_ptr] = left;
                stack_ptr += 1;
            }
            if stack_ptr < 31 && right < char_params.node_count {
                stack[stack_ptr] = right;
                stack_ptr += 1;
            }
        }
    }

    return result;
}

// BVH shadow ray traversal - returns true if occluded
fn trace_bvh_shadow(origin: vec3<f32>, direction: vec3<f32>, max_dist: f32) -> bool {
    if char_params.enabled == 0u || char_params.node_count == 0u {
        return false;
    }

    var stack: array<u32, 32>;
    var stack_ptr = 0;

    stack[0] = 0u;
    stack_ptr = 1;

    while stack_ptr > 0 {
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr];

        if node_idx >= char_params.node_count {
            continue;
        }

        let node = bvh_nodes[node_idx];

        let t_aabb = ray_aabb_intersect(origin, direction, node.bounds_min, node.bounds_max, max_dist);
        if t_aabb < 0.0 {
            continue;
        }

        let is_leaf = (node.right_or_count & BVH_LEAF_FLAG) != 0u;

        if is_leaf {
            let first_tri = node.left_or_first;
            let tri_count = node.right_or_count & (~BVH_LEAF_FLAG);

            for (var i = 0u; i < tri_count; i++) {
                let tri_idx = first_tri + i;
                if tri_idx >= char_params.triangle_count {
                    continue;
                }

                let t = ray_triangle_intersect(origin, direction, tri_idx);
                if t > 0.0 && t < max_dist {
                    return true;
                }
            }
        } else {
            let left = node.left_or_first;
            let right = node.right_or_count;

            if stack_ptr < 31 && left < char_params.node_count {
                stack[stack_ptr] = left;
                stack_ptr += 1;
            }
            if stack_ptr < 31 && right < char_params.node_count {
                stack[stack_ptr] = right;
                stack_ptr += 1;
            }
        }
    }

    return false;
}

// Convert world position to voxel texture coordinates
fn world_to_voxel(world_pos: vec3<f32>) -> vec3<i32> {
    let local_pos = world_pos - params.volume_min;
    return vec3<i32>(floor(local_pos / params.voxel_size));
}

// Simple shadow ray using DDA for voxels only (skip BVH for performance)
fn trace_shadow_ray(origin: vec3<f32>, direction: vec3<f32>, max_dist: f32) -> f32 {

    // Then check voxels with DDA
    let inv_dir = 1.0 / direction;
    let sign_dir = sign(direction);

    // Convert world position to voxel coordinates (accounting for volume_min offset)
    let local_origin = origin - params.volume_min;
    var pos = vec3<i32>(floor(local_origin / params.voxel_size));
    let step = vec3<i32>(sign_dir);
    let t_delta = abs(inv_dir) * params.voxel_size;

    var t_max_axis: vec3<f32>;
    let frac = fract(local_origin / params.voxel_size);

    if direction.x > 0.0 {
        t_max_axis.x = (1.0 - frac.x) * params.voxel_size * abs(inv_dir.x);
    } else {
        t_max_axis.x = frac.x * params.voxel_size * abs(inv_dir.x);
    }
    if direction.y > 0.0 {
        t_max_axis.y = (1.0 - frac.y) * params.voxel_size * abs(inv_dir.y);
    } else {
        t_max_axis.y = frac.y * params.voxel_size * abs(inv_dir.y);
    }
    if direction.z > 0.0 {
        t_max_axis.z = (1.0 - frac.z) * params.voxel_size * abs(inv_dir.z);
    } else {
        t_max_axis.z = frac.z * params.voxel_size * abs(inv_dir.z);
    }

    var t = 0.0;

    for (var i = 0; i < 64; i++) {
        if t_max_axis.x < t_max_axis.y && t_max_axis.x < t_max_axis.z {
            t = t_max_axis.x;
            t_max_axis.x += t_delta.x;
            pos.x += step.x;
        } else if t_max_axis.y < t_max_axis.z {
            t = t_max_axis.y;
            t_max_axis.y += t_delta.y;
            pos.y += step.y;
        } else {
            t = t_max_axis.z;
            t_max_axis.z += t_delta.z;
            pos.z += step.z;
        }

        if t > max_dist {
            return 1.0;
        }

        if !in_bounds(pos) {
            return 1.0;
        }

        if get_voxel(pos) != 0u {
            return 0.0;
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

    // Read G-buffer (voxel geometry)
    let pos_data = textureLoad(t_position, vec2<i32>(pixel), 0);
    let normal_data = textureLoad(t_normal, vec2<i32>(pixel), 0);
    let albedo_data = textureLoad(t_albedo, vec2<i32>(pixel), 0);

    // Generate primary ray for character check
    let uv = (vec2<f32>(f32(pixel.x), f32(pixel.y)) + 0.5) / vec2<f32>(f32(params.screen_width), f32(params.screen_height));
    let ndc = uv * 2.0 - 1.0;
    let clip = vec4<f32>(ndc.x, -ndc.y, 1.0, 1.0);
    let world_dir_h = params.inv_view_proj * clip;
    let world_dir = normalize(world_dir_h.xyz / world_dir_h.w - params.camera_position);

    // Trace against characters (BVH)
    let char_hit = trace_bvh_primary(params.camera_position, world_dir, 1000.0);

    // Get G-buffer depth
    let gbuffer_depth = pos_data.w;
    let gbuffer_hit = gbuffer_depth > 0.0;

    // Determine which hit to use
    var world_pos: vec3<f32>;
    var world_normal: vec3<f32>;
    var albedo: vec3<f32>;

    // Check if character is closer than voxel
    if char_hit.hit && (!gbuffer_hit || char_hit.t < gbuffer_depth) {
        // Use character hit
        world_pos = char_hit.position;
        world_normal = char_hit.normal;

        // Flip normal if facing away from camera
        let view_dir = normalize(params.camera_position - world_pos);
        if dot(world_normal, view_dir) < 0.0 {
            world_normal = -world_normal;
        }

        // Sample texture for character
        albedo = textureSampleLevel(char_textures, char_sampler, char_hit.uv, i32(char_hit.texture_id), 0.0).rgb;
    } else if gbuffer_hit {
        // Use G-buffer (voxel) hit
        world_pos = pos_data.xyz;
        world_normal = normalize(normal_data.xyz);
        albedo = albedo_data.rgb;
    } else {
        // Sky
        let sky_uv = vec2<f32>(f32(pixel.x), f32(pixel.y)) / vec2<f32>(f32(params.screen_width), f32(params.screen_height));
        let sky_color = mix(
            vec3<f32>(0.4, 0.6, 0.9),
            vec3<f32>(0.7, 0.85, 1.0),
            1.0 - sky_uv.y
        );
        textureStore(output, pixel, vec4<f32>(sky_color, 1.0));
        return;
    }

    // Ambient light
    let ambient = vec3<f32>(0.15);

    // Sun direct lighting
    let sun_dir = normalize(params.sun_direction);
    let n_dot_l = max(dot(world_normal, sun_dir), 0.0);

    var direct = vec3<f32>(0.0);
    if n_dot_l > 0.0 {
        // Shadow ray
        let shadow_origin = world_pos + world_normal * 0.02;
        let shadow = trace_shadow_ray(shadow_origin, sun_dir, 32.0);
        direct = params.sun_color * params.sun_intensity * n_dot_l * shadow;
    }

    // Simple hemisphere ambient occlusion approximation
    let ao = 0.5 + 0.5 * world_normal.y;

    // Final color
    var color = albedo * (ambient * ao + direct);

    textureStore(output, pixel, vec4<f32>(color, 1.0));
}

// Path tracing compute shader with DDA ray-voxel intersection and BVH ray-triangle intersection
// Multiple bounces, Russian roulette, cosine-weighted hemisphere sampling
// Supports hybrid tracing: voxels + polygonal characters

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

struct Material {
    albedo: vec3<f32>,
    roughness: f32,
    emission: vec3<f32>,
    metallic: f32,
};

struct Ray {
    origin: vec3<f32>,
    direction: vec3<f32>,
};

struct HitInfo {
    hit: bool,
    position: vec3<f32>,
    normal: vec3<f32>,
    t: f32,
    material_id: u32,
    uv: vec2<f32>,        // UV coordinates (for characters)
    texture_id: u32,      // Texture index (for characters)
    is_character: bool,   // Whether hit is on a character
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

// Group 0: Voxel tracing resources
@group(0) @binding(0)
var<uniform> params: PathTracerParams;

@group(0) @binding(1)
var<storage, read> materials: array<Material>;

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

// PCG random number generator
var<private> rng_state: u32;

fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn init_rng(pixel: vec2<u32>, frame: u32) {
    rng_state = pcg_hash(pixel.x + pixel.y * params.screen_width + frame * params.screen_width * params.screen_height);
}

fn rand() -> f32 {
    rng_state = pcg_hash(rng_state);
    return f32(rng_state) / 4294967295.0;
}

fn rand2() -> vec2<f32> {
    return vec2<f32>(rand(), rand());
}

const PI: f32 = 3.14159265359;

// Create orthonormal basis from normal
fn create_basis(normal: vec3<f32>) -> mat3x3<f32> {
    var tangent: vec3<f32>;
    if abs(normal.y) > 0.99 {
        tangent = normalize(cross(normal, vec3<f32>(1.0, 0.0, 0.0)));
    } else {
        tangent = normalize(cross(normal, vec3<f32>(0.0, 1.0, 0.0)));
    }
    let bitangent = cross(normal, tangent);
    return mat3x3<f32>(tangent, normal, bitangent);
}

// Cosine-weighted hemisphere sampling (for diffuse)
fn sample_hemisphere_cosine(normal: vec3<f32>) -> vec3<f32> {
    let r = rand2();
    let phi = 2.0 * PI * r.x;
    let cos_theta = sqrt(r.y);
    let sin_theta = sqrt(1.0 - r.y);

    let local_dir = vec3<f32>(
        sin_theta * cos(phi),
        cos_theta,
        sin_theta * sin(phi)
    );

    let basis = create_basis(normal);
    return normalize(basis * local_dir);
}

// GGX Normal Distribution Function
fn ggx_ndf(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// Smith geometry function (single direction)
fn smith_g1(n_dot_v: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_v * n_dot_v * (1.0 - a2) + a2;
    return (2.0 * n_dot_v) / (n_dot_v + sqrt(d));
}

// Smith geometry function (both directions)
fn smith_g(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return smith_g1(n_dot_v, roughness) * smith_g1(n_dot_l, roughness);
}

// Fresnel-Schlick approximation
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let t = 1.0 - cos_theta;
    let t2 = t * t;
    let t5 = t2 * t2 * t;
    return f0 + (1.0 - f0) * t5;
}

// GGX importance sampling - sample microfacet normal (half vector)
fn sample_ggx(normal: vec3<f32>, roughness: f32) -> vec3<f32> {
    let r = rand2();
    let a = roughness * roughness;
    let a2 = a * a;

    // Sample spherical coordinates for microfacet normal
    let phi = 2.0 * PI * r.x;
    let cos_theta = sqrt((1.0 - r.y) / (1.0 + (a2 - 1.0) * r.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);

    // Local half vector
    let local_h = vec3<f32>(
        sin_theta * cos(phi),
        cos_theta,
        sin_theta * sin(phi)
    );

    // Transform to world space
    let basis = create_basis(normal);
    return normalize(basis * local_h);
}

// PDF for GGX importance sampling
fn ggx_pdf(n_dot_h: f32, h_dot_v: f32, roughness: f32) -> f32 {
    let d = ggx_ndf(n_dot_h, roughness);
    return (d * n_dot_h) / (4.0 * h_dot_v);
}

// PDF for cosine-weighted hemisphere sampling
fn cosine_pdf(n_dot_l: f32) -> f32 {
    return n_dot_l / PI;
}

// ============================================================================
// BVH Ray Tracing for Characters
// ============================================================================

const BVH_LEAF_FLAG: u32 = 0x80000000u;
const BVH_STACK_SIZE: i32 = 32;

// Moller-Trumbore ray-triangle intersection
// Returns (t, u, v) where u,v are barycentric coordinates
fn ray_triangle_intersect(ray: Ray, tri_idx: u32) -> vec3<f32> {
    let tri = triangles[tri_idx];
    let edge1 = tri.edge1;
    let edge2 = tri.edge2;

    let h = cross(ray.direction, edge2);
    let a = dot(edge1, h);

    // Ray parallel to triangle
    if abs(a) < 0.0001 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let f = 1.0 / a;
    let s = ray.origin - tri.v0;
    let u = f * dot(s, h);

    if u < 0.0 || u > 1.0 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let q = cross(s, edge1);
    let v = f * dot(ray.direction, q);

    if v < 0.0 || u + v > 1.0 {
        return vec3<f32>(-1.0, 0.0, 0.0);
    }

    let t = f * dot(edge2, q);

    if t > 0.0001 {
        return vec3<f32>(t, u, v);
    }

    return vec3<f32>(-1.0, 0.0, 0.0);
}

// Ray-AABB intersection (slab method)
fn ray_aabb_intersect(ray: Ray, bounds_min: vec3<f32>, bounds_max: vec3<f32>, t_max: f32) -> f32 {
    let inv_dir = 1.0 / ray.direction;

    let t1 = (bounds_min - ray.origin) * inv_dir;
    let t2 = (bounds_max - ray.origin) * inv_dir;

    let t_near = min(t1, t2);
    let t_far = max(t1, t2);

    let t_enter = max(max(t_near.x, t_near.y), t_near.z);
    let t_exit = min(min(t_far.x, t_far.y), t_far.z);

    if t_enter > t_exit || t_exit < 0.0 || t_enter > t_max {
        return -1.0;
    }

    return max(t_enter, 0.0);
}

// BVH traversal with stack
fn trace_bvh(ray: Ray, max_dist: f32) -> HitInfo {
    var result: HitInfo;
    result.hit = false;
    result.t = max_dist;
    result.is_character = false;

    if char_params.enabled == 0u || char_params.node_count == 0u {
        return result;
    }

    // Stack for BVH traversal
    var stack: array<u32, 32>;
    var stack_ptr = 0;

    // Start with root node
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

        // Test ray against node AABB
        let t_aabb = ray_aabb_intersect(ray, node.bounds_min, node.bounds_max, closest_t);
        if t_aabb < 0.0 {
            continue;
        }

        // Check if leaf node
        let is_leaf = (node.right_or_count & BVH_LEAF_FLAG) != 0u;

        if is_leaf {
            let first_tri = node.left_or_first;
            let tri_count = node.right_or_count & (~BVH_LEAF_FLAG);

            // Test all triangles in leaf
            for (var i = 0u; i < tri_count; i++) {
                let tri_idx = first_tri + i;
                if tri_idx >= char_params.triangle_count {
                    continue;
                }

                let hit_data = ray_triangle_intersect(ray, tri_idx);
                let t = hit_data.x;

                if t > 0.0 && t < closest_t {
                    closest_t = t;
                    result.hit = true;
                    result.t = t;
                    result.position = ray.origin + ray.direction * t;

                    let tri = triangles[tri_idx];
                    // Compute geometric normal from edges instead of using stored normal
                    result.normal = normalize(cross(tri.edge1, tri.edge2));

                    // Interpolate UV
                    let u = hit_data.y;
                    let v = hit_data.z;
                    let w = 1.0 - u - v;
                    result.uv = tri.uv0 * w + tri.uv1 * u + tri.uv2 * v;

                    result.texture_id = tri.texture_id;
                    result.material_id = tri.material_id;
                    result.is_character = true;
                }
            }
        } else {
            // Interior node - push children
            if stack_ptr < BVH_STACK_SIZE - 1 {
                stack[stack_ptr] = node.left_or_first;
                stack_ptr += 1;
            }
            if stack_ptr < BVH_STACK_SIZE - 1 {
                stack[stack_ptr] = node.right_or_count;
                stack_ptr += 1;
            }
        }
    }

    return result;
}

// ============================================================================
// Hybrid Scene Tracing
// ============================================================================

// Trace ray against both voxels and characters, return closest hit
fn trace_scene(ray: Ray, max_dist: f32) -> HitInfo {
    // Trace voxels
    let voxel_hit = trace_ray_dda(ray, max_dist);

    // Trace characters (BVH)
    let char_hit = trace_bvh(ray, max_dist);

    // Return closer hit
    if !voxel_hit.hit && !char_hit.hit {
        var result: HitInfo;
        result.hit = false;
        result.t = max_dist;
        return result;
    }

    if !char_hit.hit || (voxel_hit.hit && voxel_hit.t < char_hit.t) {
        return voxel_hit;
    }

    return char_hit;
}

// Trace shadow ray against full scene (characters + voxels via DDA)
fn trace_shadow_scene(origin: vec3<f32>, direction: vec3<f32>) -> f32 {
    // Check character shadows via BVH
    let ray = Ray(origin + direction * 0.01, direction);
    let char_hit = trace_bvh(ray, 64.0);
    if char_hit.hit {
        return 0.0;
    }
    // Check voxel shadows via DDA through voxel volume
    return trace_shadow_ray(origin, direction);
}

// Check if world voxel position is inside the volume
fn in_volume(pos: vec3<i32>) -> bool {
    let size = vec3<i32>(textureDimensions(t_voxels));
    let origin = vec3<i32>(params.volume_min / params.voxel_size);
    let rel = pos - origin;
    return all(rel >= vec3<i32>(0)) && all(rel < size);
}

// Get voxel at world voxel position (toroidal addressing)
fn get_voxel(pos: vec3<i32>) -> u32 {
    if !in_volume(pos) {
        return 0u;
    }
    let size = vec3<i32>(textureDimensions(t_voxels));
    let tex = ((pos % size) + size) % size;
    return textureLoad(t_voxels, tex, 0).r;
}

// DDA ray-voxel intersection (world voxel coordinates, toroidal addressing)
fn trace_ray_dda(ray: Ray, max_dist: f32) -> HitInfo {
    var result: HitInfo;
    result.hit = false;
    result.t = max_dist;
    result.is_character = false;
    result.uv = vec2<f32>(0.0);
    result.texture_id = 0u;

    let inv_dir = 1.0 / ray.direction;
    let sign_dir = sign(ray.direction);

    // Work in world voxel coordinates (toroidal get_voxel handles the wrap)
    let world_origin = ray.origin / params.voxel_size;

    // Starting voxel position (world voxel coords)
    var pos = vec3<i32>(floor(world_origin));

    // Step direction
    let step = vec3<i32>(sign_dir);

    // Distance to next voxel boundary
    let t_delta = abs(inv_dir) * params.voxel_size;

    // Initial t_max (distance to first boundary in each dimension)
    var t_max: vec3<f32>;
    let frac = fract(world_origin);

    if ray.direction.x > 0.0 {
        t_max.x = (1.0 - frac.x) * params.voxel_size * abs(inv_dir.x);
    } else {
        t_max.x = frac.x * params.voxel_size * abs(inv_dir.x);
    }
    if ray.direction.y > 0.0 {
        t_max.y = (1.0 - frac.y) * params.voxel_size * abs(inv_dir.y);
    } else {
        t_max.y = frac.y * params.voxel_size * abs(inv_dir.y);
    }
    if ray.direction.z > 0.0 {
        t_max.z = (1.0 - frac.z) * params.voxel_size * abs(inv_dir.z);
    } else {
        t_max.z = frac.z * params.voxel_size * abs(inv_dir.z);
    }

    var t = 0.0;
    var normal = vec3<f32>(0.0);

    // Check starting voxel first
    let start_voxel = get_voxel(pos);
    if start_voxel != 0u {
        result.hit = true;
        result.position = ray.origin;
        result.normal = -ray.direction;
        result.t = 0.0;
        result.material_id = start_voxel;
        return result;
    }

    // DDA traversal
    for (var i = 0; i < 512; i++) {
        // Find which dimension to step in
        if t_max.x < t_max.y && t_max.x < t_max.z {
            t = t_max.x;
            t_max.x += t_delta.x;
            pos.x += step.x;
            normal = vec3<f32>(-f32(step.x), 0.0, 0.0);
        } else if t_max.y < t_max.z {
            t = t_max.y;
            t_max.y += t_delta.y;
            pos.y += step.y;
            normal = vec3<f32>(0.0, -f32(step.y), 0.0);
        } else {
            t = t_max.z;
            t_max.z += t_delta.z;
            pos.z += step.z;
            normal = vec3<f32>(0.0, 0.0, -f32(step.z));
        }

        if t > max_dist {
            break;
        }

        if !in_volume(pos) {
            break;
        }

        let voxel = get_voxel(pos);
        if voxel != 0u {
            result.hit = true;
            result.position = ray.origin + ray.direction * t;
            result.normal = normal;
            result.t = t;
            result.material_id = voxel;
            return result;
        }
    }

    return result;
}

// Shadow ray using DDA through the 3D voxel volume texture
fn trace_shadow_ray(origin: vec3<f32>, direction: vec3<f32>) -> f32 {
    let ray = Ray(origin + direction * 0.01, direction);
    let hit = trace_ray_dda(ray, 64.0);
    if hit.hit {
        return 0.0;
    }
    return 1.0;
}

// Evaluate PBR BRDF (simplified, more vibrant)
fn evaluate_brdf(
    view_dir: vec3<f32>,
    light_dir: vec3<f32>,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32
) -> vec3<f32> {
    let h = normalize(view_dir + light_dir);
    let n_dot_v = max(dot(normal, view_dir), 0.001);
    let n_dot_l = max(dot(normal, light_dir), 0.001);
    let n_dot_h = max(dot(normal, h), 0.001);
    let h_dot_v = max(dot(h, view_dir), 0.001);

    // F0 for dielectrics is ~0.04, for metals it's the albedo
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);

    // Fresnel
    let f = fresnel_schlick(h_dot_v, f0);

    // Specular intensity based on roughness (smoother = more specular)
    let spec_intensity = (1.0 - roughness) * (1.0 - roughness);

    // Diffuse - metals have no diffuse, smoother surfaces have less diffuse
    let k_d = (1.0 - metallic) * mix(1.0, 0.5, spec_intensity);
    let diffuse = k_d * albedo;

    // Specular - blend between diffuse reflection and mirror based on roughness
    let specular = f * spec_intensity * 0.5;

    return diffuse + specular;
}

// Path trace from a hit point
fn path_trace(start_pos: vec3<f32>, start_normal: vec3<f32>, start_albedo: vec3<f32>, start_material_id: u32) -> vec3<f32> {
    var throughput = vec3<f32>(1.0);
    var radiance = vec3<f32>(0.0);

    var pos = start_pos;
    var normal = start_normal;
    var albedo = start_albedo;
    var material_id = start_material_id;

    // Get material
    var mat = materials[material_id];
    var roughness = max(mat.roughness, 0.04);
    var metallic = mat.metallic;

    // Add emission
    radiance += mat.emission;

    var view_dir = normalize(params.camera_position - pos);

    // Direct lighting (sun)
    let sun_dir = normalize(params.sun_direction);
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    if n_dot_l > 0.0 {
        let shadow = trace_shadow_scene(pos + normal * 0.01, sun_dir);
        let direct = params.sun_color * params.sun_intensity * n_dot_l * shadow;
        radiance += throughput * albedo * direct;
    }

    // Indirect lighting with bounces
    for (var bounce = 0u; bounce < params.max_bounces; bounce++) {
        // Russian roulette - but keep specular rays alive longer
        let dominated_by_specular = roughness < 0.3;
        let survival_boost = select(1.0, 0.95, dominated_by_specular);
        let p = max(max(max(throughput.x, throughput.y), throughput.z), survival_boost);
        if rand() > p {
            break;
        }
        throughput /= p;

        // Specular probability - low roughness = almost all specular
        // This is the key: roughness controls reflection vs diffuse, not just Fresnel
        let smoothness = 1.0 - roughness;
        let spec_prob = smoothness * smoothness; // roughness=0 -> 100% specular, roughness=0.5 -> 25%

        var new_dir: vec3<f32>;
        var bounce_color: vec3<f32>;

        if rand() < spec_prob {
            // Specular reflection
            if roughness < 0.1 {
                // Near-perfect mirror: use exact reflection
                new_dir = reflect(-view_dir, normal);
            } else {
                // Glossy: use GGX microfacet sampling
                let h = sample_ggx(normal, roughness);
                new_dir = reflect(-view_dir, h);
            }

            // Check for valid reflection direction
            if dot(new_dir, normal) <= 0.0 {
                // Fallback to perfect reflection if GGX gave bad result
                new_dir = reflect(-view_dir, normal);
            }

            // Specular color: white for dielectrics, tinted for metals
            bounce_color = mix(vec3<f32>(0.96), albedo, metallic);
        } else {
            // Diffuse reflection
            new_dir = sample_hemisphere_cosine(normal);
            bounce_color = albedo;
        }

        // Trace ray (reduced distance for performance)
        let ray = Ray(pos + normal * 0.01, new_dir);
        let hit = trace_scene(ray, 16.0);

        if !hit.hit {
            // Sky
            let sky_color = mix(
                vec3<f32>(0.5, 0.7, 1.0),
                vec3<f32>(0.1, 0.2, 0.4),
                pow(1.0 - max(new_dir.y, 0.0), 2.0)
            );
            radiance += throughput * bounce_color * sky_color * 0.3;
            break;
        }

        // Update throughput
        throughput *= bounce_color;

        // Prevent fireflies
        throughput = min(throughput, vec3<f32>(5.0));

        // Move to hit point
        pos = hit.position;
        normal = hit.normal;
        view_dir = -new_dir;
        material_id = hit.material_id;
        mat = materials[material_id];
        roughness = max(mat.roughness, 0.04);
        metallic = mat.metallic;

        // Get albedo from texture for characters, material for voxels
        if hit.is_character {
            albedo = textureSampleLevel(char_textures, char_sampler, hit.uv, i32(hit.texture_id), 0.0).rgb;
        } else {
            albedo = mat.albedo;
        }

        // Emission
        radiance += throughput * mat.emission;

        // Direct lighting at bounce
        let bounce_n_dot_l = max(dot(normal, sun_dir), 0.0);
        if bounce_n_dot_l > 0.0 {
            let shadow = trace_shadow_scene(pos + normal * 0.01, sun_dir);
            let direct = params.sun_color * params.sun_intensity * bounce_n_dot_l * shadow;
            radiance += throughput * albedo * direct * 0.5;
        }
    }

    return radiance;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let pixel = global_id.xy;

    if pixel.x >= params.screen_width || pixel.y >= params.screen_height {
        return;
    }

    // Initialize RNG
    init_rng(pixel, params.frame_index);

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
    let primary_ray = Ray(params.camera_position, world_dir);

    // Trace against characters (BVH)
    let char_hit = trace_bvh(primary_ray, 1000.0);

    // Get G-buffer depth
    let gbuffer_depth = pos_data.w; // w stores linear depth, 0 means no hit
    let gbuffer_hit = gbuffer_depth > 0.0;

    // Determine which hit to use
    var world_pos: vec3<f32>;
    var world_normal: vec3<f32>;
    var albedo: vec3<f32>;
    var material_id: u32;
    var is_character = false;

    // Check if character is closer than voxel
    if char_hit.hit && (!gbuffer_hit || char_hit.t < gbuffer_depth) {
        // Use character hit
        world_pos = char_hit.position;
        world_normal = char_hit.normal;
        material_id = char_hit.material_id;
        is_character = true;

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
        material_id = u32(normal_data.w);
        albedo = albedo_data.rgb;
    } else {
        // Sky - output background color
        let sky_uv = vec2<f32>(f32(pixel.x), f32(pixel.y)) / vec2<f32>(f32(params.screen_width), f32(params.screen_height));
        let sky_color = mix(
            vec3<f32>(0.4, 0.6, 0.9),
            vec3<f32>(0.7, 0.85, 1.0),
            1.0 - sky_uv.y
        );
        textureStore(output, pixel, vec4<f32>(sky_color, 1.0));
        return;
    }

    // Path trace
    var color = path_trace(world_pos, world_normal, albedo, material_id);

    // Output
    textureStore(output, pixel, vec4<f32>(color, 1.0));
}

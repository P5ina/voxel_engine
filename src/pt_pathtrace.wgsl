// Path tracing compute shader with DDA ray-voxel intersection
// Multiple bounces, Russian roulette, cosine-weighted hemisphere sampling

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
};

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

// Cosine-weighted hemisphere sampling
fn sample_hemisphere_cosine(normal: vec3<f32>) -> vec3<f32> {
    let r = rand2();
    let phi = 2.0 * 3.14159265359 * r.x;
    let cos_theta = sqrt(r.y);
    let sin_theta = sqrt(1.0 - r.y);

    // Create tangent space
    var tangent: vec3<f32>;
    if abs(normal.y) > 0.99 {
        tangent = normalize(cross(normal, vec3<f32>(1.0, 0.0, 0.0)));
    } else {
        tangent = normalize(cross(normal, vec3<f32>(0.0, 1.0, 0.0)));
    }
    let bitangent = cross(normal, tangent);

    // Sample direction in tangent space
    let local_dir = vec3<f32>(
        sin_theta * cos(phi),
        cos_theta,
        sin_theta * sin(phi)
    );

    // Transform to world space
    return normalize(tangent * local_dir.x + normal * local_dir.y + bitangent * local_dir.z);
}

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

// DDA ray-voxel intersection
fn trace_ray_dda(ray: Ray, max_dist: f32) -> HitInfo {
    var result: HitInfo;
    result.hit = false;
    result.t = max_dist;

    let inv_dir = 1.0 / ray.direction;
    let sign_dir = sign(ray.direction);

    // Starting voxel position
    var pos = vec3<i32>(floor(ray.origin / params.voxel_size));

    // Step direction
    let step = vec3<i32>(sign_dir);

    // Distance to next voxel boundary
    let t_delta = abs(inv_dir) * params.voxel_size;

    // Initial t_max (distance to first boundary in each dimension)
    var t_max: vec3<f32>;
    let frac = fract(ray.origin / params.voxel_size);

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
    for (var i = 0; i < 256; i++) {
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

        if !in_bounds(pos) {
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

// Trace shadow ray to sun
fn trace_shadow_ray(origin: vec3<f32>, direction: vec3<f32>) -> f32 {
    let ray = Ray(origin + direction * 0.01, direction);
    let hit = trace_ray_dda(ray, 100.0);
    if hit.hit {
        return 0.0;
    }
    return 1.0;
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

    // Add emission
    radiance += mat.emission;

    // Direct lighting (sun)
    let sun_dir = normalize(params.sun_direction);
    let n_dot_l = max(dot(normal, sun_dir), 0.0);
    if n_dot_l > 0.0 {
        let shadow = trace_shadow_ray(pos + normal * 0.01, sun_dir);
        let direct = params.sun_color * params.sun_intensity * n_dot_l * shadow;
        radiance += throughput * albedo * direct;
    }

    // Indirect lighting with bounces
    for (var bounce = 0u; bounce < params.max_bounces; bounce++) {
        // Russian roulette
        let p = max(max(throughput.x, throughput.y), throughput.z);
        if rand() > p {
            break;
        }
        throughput /= p;

        // Sample new direction
        let new_dir = sample_hemisphere_cosine(normal);

        // Trace ray
        let ray = Ray(pos + normal * 0.01, new_dir);
        let hit = trace_ray_dda(ray, 50.0);

        if !hit.hit {
            // Sky contribution
            let sky_color = mix(
                vec3<f32>(0.5, 0.7, 1.0),
                vec3<f32>(0.1, 0.2, 0.4),
                pow(1.0 - max(new_dir.y, 0.0), 2.0)
            );
            radiance += throughput * albedo * sky_color * 0.3;
            break;
        }

        // Update throughput with BRDF
        // For Lambertian: albedo / PI, but cosine sampling cancels PI
        throughput *= albedo;

        // Move to new position
        pos = hit.position;
        normal = hit.normal;
        material_id = hit.material_id;
        mat = materials[material_id];
        albedo = mat.albedo;

        // Add emission at this hit
        radiance += throughput * mat.emission;

        // Direct lighting at bounce point
        let bounce_n_dot_l = max(dot(normal, sun_dir), 0.0);
        if bounce_n_dot_l > 0.0 {
            let shadow = trace_shadow_ray(pos + normal * 0.01, sun_dir);
            let direct = params.sun_color * params.sun_intensity * bounce_n_dot_l * shadow;
            radiance += throughput * albedo * direct * 0.5; // Attenuate bounced light
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

    // Read G-buffer
    let pos_data = textureLoad(t_position, vec2<i32>(pixel), 0);
    let normal_data = textureLoad(t_normal, vec2<i32>(pixel), 0);
    let albedo_data = textureLoad(t_albedo, vec2<i32>(pixel), 0);

    // Check if this pixel hit geometry
    if pos_data.w == 0.0 {
        // Sky - output background color
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
    let material_id = u32(normal_data.w);
    let tex_albedo = albedo_data.rgb;

    // Use texture albedo combined with material albedo
    let mat = materials[material_id];
    let albedo = tex_albedo;

    // Path trace
    let color = path_trace(world_pos, world_normal, albedo, material_id);

    // Output
    textureStore(output, pixel, vec4<f32>(color, 1.0));
}

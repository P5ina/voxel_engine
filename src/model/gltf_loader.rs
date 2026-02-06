//! GLTF/GLB file loader

use std::path::Path;

use glam::{Mat4, Quat, Vec3};

use crate::model::animation::{
    AnimationChannel, AnimationClip, Interpolation, Keyframe, KeyframeValue,
};
use crate::model::mesh::{LoadedModel, LoadedTexture, MeshVertex, PolygonMesh, TextureFormat};
use crate::model::skeleton::{Joint, JointTransform, Skeleton};

/// Error type for model loading
#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Gltf(gltf::Error),
    InvalidData(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "IO error: {}", e),
            LoadError::Gltf(e) => write!(f, "GLTF error: {}", e),
            LoadError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl From<gltf::Error> for LoadError {
    fn from(e: gltf::Error) -> Self {
        LoadError::Gltf(e)
    }
}

/// Load a GLB (binary GLTF) file
pub fn load_glb<P: AsRef<Path>>(path: P) -> Result<LoadedModel, LoadError> {
    let path = path.as_ref();
    let (document, buffers, images) = gltf::import(path)?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    load_from_gltf(document, buffers, images, name)
}

/// Load model from parsed GLTF data
fn load_from_gltf(
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
    _name: String,
) -> Result<LoadedModel, LoadError> {
    let mut model = LoadedModel::new();

    // Load textures
    for image in images {
        let texture = LoadedTexture {
            width: image.width,
            height: image.height,
            format: match image.format {
                gltf::image::Format::R8G8B8A8 => TextureFormat::Rgba8,
                gltf::image::Format::R8G8B8 => TextureFormat::Rgb8,
                _ => {
                    // Convert to RGBA8
                    let rgba = convert_to_rgba(&image);
                    model.textures.push(LoadedTexture {
                        data: rgba,
                        width: image.width,
                        height: image.height,
                        format: TextureFormat::Rgba8,
                    });
                    continue;
                }
            },
            data: image.pixels,
        };
        model.textures.push(texture);
    }

    // Load skeleton from skins (standard GLTF skinning)
    let mut skin_joint_map: Vec<usize> = Vec::new();
    let mut use_node_skeleton = false;

    if let Some(skin) = document.skins().next() {
        let skeleton = load_skeleton(&skin, &buffers)?;
        skin_joint_map = skin.joints().map(|j| j.index()).collect();
        model.skeleton = Some(skeleton);
    } else {
        // Blockbench-style: create skeleton from node hierarchy
        let (skeleton, node_to_joint) = create_skeleton_from_nodes(&document);
        if !skeleton.joints.is_empty() {
            log::info!(
                "Created skeleton from node hierarchy with {} joints",
                skeleton.joints.len()
            );
            skin_joint_map = node_to_joint.iter().map(|(&k, _)| k).collect();
            model.skeleton = Some(skeleton);
            use_node_skeleton = true;
        }
    }

    // Build node world transforms
    let node_transforms = compute_node_world_transforms(&document);

    // Load meshes by traversing nodes
    for node in document.nodes() {
        if let Some(mesh) = node.mesh() {
            let world_transform = node_transforms
                .get(&node.index())
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            let node_name = node.name().unwrap_or("").to_string();

            for primitive in mesh.primitives() {
                let mut polygon_mesh = load_primitive(&primitive, &buffers, &skin_joint_map)?;

                if use_node_skeleton {
                    // For Blockbench: find joint index for this node and assign to all vertices
                    if let Some(skeleton) = &model.skeleton {
                        let joint_idx = skeleton
                            .joints
                            .iter()
                            .position(|j| j.name == node_name)
                            .unwrap_or(0) as u32;

                        for vertex in &mut polygon_mesh.vertices {
                            vertex.joint_indices = [joint_idx, 0, 0, 0];
                            vertex.joint_weights = [1.0, 0.0, 0.0, 0.0];
                        }
                    }
                    // Don't apply static transform - let skeleton handle it
                } else if skin_joint_map.is_empty() {
                    // No skeleton at all - apply transform statically
                    apply_transform_to_mesh(&mut polygon_mesh, world_transform);
                }

                // Use node name for the mesh
                if polygon_mesh.name.starts_with("mesh_") && !node_name.is_empty() {
                    polygon_mesh.name = node_name.clone();
                }

                model.meshes.push(polygon_mesh);
            }
        }
    }

    // Build node-to-joint map for animations
    let _node_to_joint_map: Vec<usize> = if use_node_skeleton {
        document
            .nodes()
            .filter(|n| n.mesh().is_some())
            .map(|n| {
                model
                    .skeleton
                    .as_ref()
                    .and_then(|s| {
                        s.joints
                            .iter()
                            .position(|j| j.name == n.name().unwrap_or(""))
                    })
                    .unwrap_or(0)
            })
            .collect()
    } else {
        skin_joint_map.clone()
    };

    // Load animations
    for animation in document.animations() {
        let clip =
            load_animation_for_nodes(&animation, &buffers, &document, model.skeleton.as_ref())?;
        model.animations.push(clip);
    }

    Ok(model)
}

/// Compute world transforms for all nodes by traversing the hierarchy
fn compute_node_world_transforms(
    document: &gltf::Document,
) -> std::collections::HashMap<usize, Mat4> {
    let mut transforms = std::collections::HashMap::new();

    // Find root nodes (nodes without parents)
    let all_children: std::collections::HashSet<usize> = document
        .nodes()
        .flat_map(|n| n.children().map(|c| c.index()))
        .collect();

    let root_nodes: Vec<_> = document
        .nodes()
        .filter(|n| !all_children.contains(&n.index()))
        .collect();

    // Recursively compute transforms
    fn compute_recursive(
        node: &gltf::Node,
        parent_transform: Mat4,
        transforms: &mut std::collections::HashMap<usize, Mat4>,
    ) {
        let local = node_to_mat4(node);
        let world = parent_transform * local;
        transforms.insert(node.index(), world);

        for child in node.children() {
            compute_recursive(&child, world, transforms);
        }
    }

    for root in root_nodes {
        compute_recursive(&root, Mat4::IDENTITY, &mut transforms);
    }

    transforms
}

/// Convert node transform to Mat4
fn node_to_mat4(node: &gltf::Node) -> Mat4 {
    let (translation, rotation, scale) = node.transform().decomposed();
    Mat4::from_scale_rotation_translation(
        Vec3::from(scale),
        Quat::from_array(rotation),
        Vec3::from(translation),
    )
}

/// Create skeleton from node hierarchy (for Blockbench-style models without GLTF skin)
fn create_skeleton_from_nodes(
    document: &gltf::Document,
) -> (Skeleton, std::collections::HashMap<usize, usize>) {
    let mut skeleton = Skeleton::new();
    let mut node_to_joint: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();

    // Find parent relationships
    let mut parent_map: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for node in document.nodes() {
        for child in node.children() {
            parent_map.insert(child.index(), node.index());
        }
    }

    // First pass: add ALL nodes as joints (to preserve hierarchy)
    // We need to add them in order so parents are added before children
    fn add_node_recursive(
        node: &gltf::Node,
        parent_joint_idx: Option<usize>,
        skeleton: &mut Skeleton,
        node_to_joint: &mut std::collections::HashMap<usize, usize>,
        _document: &gltf::Document,
    ) {
        let name = node.name().unwrap_or("").to_string();

        let mut joint = Joint::new(name, parent_joint_idx);

        // Set local transform
        let (translation, rotation, scale) = node.transform().decomposed();
        joint.local_transform = JointTransform::new(
            Vec3::from(translation),
            Quat::from_array(rotation),
            Vec3::from(scale),
        );

        // For Blockbench node hierarchy: vertices are in local space, bound to same-name joint
        // The inverse_bind should be IDENTITY so skin_matrix = global (transforms local to world)
        joint.inverse_bind_matrix = Mat4::IDENTITY;

        let joint_idx = skeleton.joints.len();
        node_to_joint.insert(node.index(), joint_idx);
        skeleton.add_joint(joint);

        // Recurse to children
        for child in node.children() {
            add_node_recursive(&child, Some(joint_idx), skeleton, node_to_joint, _document);
        }
    }

    // Find root nodes and start recursion
    let all_children: std::collections::HashSet<usize> = document
        .nodes()
        .flat_map(|n| n.children().map(|c| c.index()))
        .collect();

    for node in document.nodes() {
        if !all_children.contains(&node.index()) {
            add_node_recursive(&node, None, &mut skeleton, &mut node_to_joint, document);
        }
    }

    (skeleton, node_to_joint)
}

/// Load animation with node-based joint mapping (for Blockbench models)
fn load_animation_for_nodes(
    animation: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    _document: &gltf::Document,
    skeleton: Option<&Skeleton>,
) -> Result<AnimationClip, LoadError> {
    let name = animation.name().unwrap_or("animation").to_string();
    let mut clip = AnimationClip::new(name);

    // Check extras for loop information (Blockbench exports this)
    if let Some(extras) = animation.extras().as_ref() {
        let extras_str = extras.get();
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(extras_str)
            && let Some(loop_val) = json.get("loop")
        {
            clip.looping = match loop_val {
                serde_json::Value::Bool(b) => *b,
                serde_json::Value::String(s) => s == "loop" || s == "true",
                _ => clip.looping,
            };
        }
    }

    let Some(skeleton) = skeleton else {
        return Ok(clip);
    };

    log::debug!(
        "Loading animation '{}' (looping: {})",
        clip.name,
        clip.looping
    );

    for channel in animation.channels() {
        let target = channel.target();
        let node_name = target.node().name().unwrap_or("");

        // Find joint by node name
        let joint_index = skeleton
            .joints
            .iter()
            .position(|j| j.name == node_name)
            .unwrap_or(0);

        let sampler = channel.sampler();
        let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));

        // Read input (times)
        let times: Vec<f32> = match reader.read_inputs() {
            Some(iter) => iter.collect(),
            None => continue,
        };

        // Read outputs based on property
        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };

        let keyframes: Vec<Keyframe> = match target.property() {
            gltf::animation::Property::Translation => {
                let Some(outputs) = reader.read_outputs() else {
                    continue;
                };
                if let gltf::animation::util::ReadOutputs::Translations(iter) = outputs {
                    times
                        .iter()
                        .zip(iter)
                        .map(|(&time, translation)| Keyframe {
                            time,
                            value: KeyframeValue::Translation(Vec3::from(translation)),
                        })
                        .collect()
                } else {
                    continue;
                }
            }
            gltf::animation::Property::Rotation => {
                let Some(outputs) = reader.read_outputs() else {
                    continue;
                };
                if let gltf::animation::util::ReadOutputs::Rotations(iter) = outputs {
                    times
                        .iter()
                        .zip(iter.into_f32())
                        .map(|(&time, rotation)| Keyframe {
                            time,
                            value: KeyframeValue::Rotation(Quat::from_array(rotation)),
                        })
                        .collect()
                } else {
                    continue;
                }
            }
            gltf::animation::Property::Scale => {
                let Some(outputs) = reader.read_outputs() else {
                    continue;
                };
                if let gltf::animation::util::ReadOutputs::Scales(iter) = outputs {
                    times
                        .iter()
                        .zip(iter)
                        .map(|(&time, scale)| Keyframe {
                            time,
                            value: KeyframeValue::Scale(Vec3::from(scale)),
                        })
                        .collect()
                } else {
                    continue;
                }
            }
            gltf::animation::Property::MorphTargetWeights => continue,
        };

        clip.channels.push(AnimationChannel {
            joint_index,
            keyframes,
            interpolation,
        });
    }

    clip.calculate_duration();
    Ok(clip)
}

/// Apply a transform matrix to all vertices in a mesh
fn apply_transform_to_mesh(mesh: &mut PolygonMesh, transform: Mat4) {
    let normal_matrix = transform.inverse().transpose();

    for vertex in &mut mesh.vertices {
        // Transform position
        let pos = Vec3::from(vertex.position);
        let transformed_pos = transform.transform_point3(pos);
        vertex.position = transformed_pos.into();

        // Transform normal
        let normal = Vec3::from(vertex.normal);
        let transformed_normal = normal_matrix.transform_vector3(normal).normalize();
        vertex.normal = transformed_normal.into();
    }
}

/// Convert image data to RGBA8 format
fn convert_to_rgba(image: &gltf::image::Data) -> Vec<u8> {
    match image.format {
        gltf::image::Format::R8 => image.pixels.iter().flat_map(|&r| [r, r, r, 255]).collect(),
        gltf::image::Format::R8G8 => image
            .pixels
            .chunks(2)
            .flat_map(|rg| [rg[0], rg[0], rg[0], rg.get(1).copied().unwrap_or(255)])
            .collect(),
        gltf::image::Format::R8G8B8 => image
            .pixels
            .chunks(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
        gltf::image::Format::R8G8B8A8 => image.pixels.clone(),
        _ => {
            // Fallback: just use as-is and hope for the best
            image.pixels.clone()
        }
    }
}

/// Load skeleton from GLTF skin
fn load_skeleton(skin: &gltf::Skin, buffers: &[gltf::buffer::Data]) -> Result<Skeleton, LoadError> {
    let mut skeleton = Skeleton::new();

    // Get inverse bind matrices
    let inverse_bind_matrices: Vec<Mat4> = if let Some(accessor) = skin.inverse_bind_matrices() {
        read_accessor_mat4(&accessor, buffers)?
    } else {
        vec![Mat4::IDENTITY; skin.joints().count()]
    };

    // Build joint hierarchy
    // First pass: create all joints and collect their node indices
    let joints: Vec<_> = skin.joints().collect();
    let joint_node_indices: Vec<usize> = joints.iter().map(|j| j.index()).collect();

    // Build parent map by checking children of each joint
    let mut parent_map: Vec<Option<usize>> = vec![None; joints.len()];
    for (i, joint_node) in joints.iter().enumerate() {
        for child in joint_node.children() {
            // Find if this child is in our joint list
            if let Some(child_idx) = joint_node_indices
                .iter()
                .position(|&idx| idx == child.index())
            {
                parent_map[child_idx] = Some(i);
            }
        }
    }

    for (i, joint_node) in joints.iter().enumerate() {
        let default_name = format!("joint_{}", i);
        let name = joint_node.name().unwrap_or(&default_name).to_string();

        let mut joint = Joint::new(name, parent_map[i]);

        // Set inverse bind matrix
        if i < inverse_bind_matrices.len() {
            joint.inverse_bind_matrix = inverse_bind_matrices[i];
        }

        // Set local transform from node
        let (translation, rotation, scale) = joint_node.transform().decomposed();
        joint.local_transform = JointTransform::new(
            Vec3::from(translation),
            Quat::from_array(rotation),
            Vec3::from(scale),
        );

        skeleton.add_joint(joint);
    }

    Ok(skeleton)
}

/// Load a mesh primitive
fn load_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    skin_joint_map: &[usize],
) -> Result<PolygonMesh, LoadError> {
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    // Read positions (required)
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| LoadError::InvalidData("Missing positions".to_string()))?
        .collect();

    // Read normals (optional, generate if missing)
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|iter| iter.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

    // Read UVs (optional)
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|iter| iter.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

    // Read joint indices (optional)
    let joint_indices: Vec<[u32; 4]> = reader
        .read_joints(0)
        .map(|iter| {
            iter.into_u16()
                .map(|[a, b, c, d]| {
                    // Remap joint indices using skin_joint_map
                    let remap = |idx: u16| -> u32 {
                        let idx_usize = idx as usize;
                        if idx_usize < skin_joint_map.len() {
                            skin_joint_map[idx_usize] as u32
                        } else {
                            0
                        }
                    };
                    [remap(a), remap(b), remap(c), remap(d)]
                })
                .collect()
        })
        .unwrap_or_else(|| vec![[0, 0, 0, 0]; positions.len()]);

    // Read joint weights (optional)
    let joint_weights: Vec<[f32; 4]> = reader
        .read_weights(0)
        .map(|iter| iter.into_f32().collect())
        .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 0.0]; positions.len()]);

    // Read indices
    let indices: Option<Vec<u32>> = reader.read_indices().map(|iter| iter.into_u32().collect());

    // Build vertices
    let vertices: Vec<MeshVertex> = positions
        .iter()
        .enumerate()
        .map(|(i, &pos)| {
            MeshVertex::new(
                pos,
                normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                joint_indices.get(i).copied().unwrap_or([0, 0, 0, 0]),
                joint_weights
                    .get(i)
                    .copied()
                    .unwrap_or([1.0, 0.0, 0.0, 0.0]),
            )
        })
        .collect();

    // Get material/texture reference
    let material = primitive.material();
    let texture_index = material
        .pbr_metallic_roughness()
        .base_color_texture()
        .map(|tex| tex.texture().source().index());

    let mut mesh = PolygonMesh::new(format!("mesh_{}", primitive.index()));
    mesh.vertices = vertices;
    mesh.indices = indices;
    mesh.texture_index = texture_index;
    mesh.material_index = material.index();

    Ok(mesh)
}

/// Read Mat4 values from an accessor
fn read_accessor_mat4(
    accessor: &gltf::Accessor,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<Mat4>, LoadError> {
    let view = accessor
        .view()
        .ok_or_else(|| LoadError::InvalidData("Missing buffer view for accessor".to_string()))?;

    let buffer = &buffers[view.buffer().index()];
    let offset = view.offset() + accessor.offset();
    let stride = view.stride().unwrap_or(64); // Mat4 is 64 bytes

    let count = accessor.count();
    let mut matrices = Vec::with_capacity(count);

    for i in 0..count {
        let start = offset + i * stride;
        let end = start + 64;

        if end > buffer.len() {
            return Err(LoadError::InvalidData(
                "Buffer overflow reading matrices".to_string(),
            ));
        }

        let slice = &buffer[start..end];
        let floats: [f32; 16] = bytemuck::cast_slice(slice)[..16]
            .try_into()
            .map_err(|_| LoadError::InvalidData("Failed to read matrix data".to_string()))?;

        matrices.push(Mat4::from_cols_array(&floats));
    }

    Ok(matrices)
}

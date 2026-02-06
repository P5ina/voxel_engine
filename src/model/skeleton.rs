//! Skeleton structures for skeletal animation

use glam::{Mat4, Quat, Vec3};

/// A single joint in the skeleton hierarchy
#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    pub parent_index: Option<usize>,
    pub children: Vec<usize>,
    pub inverse_bind_matrix: Mat4,
    pub local_transform: JointTransform,
}

impl Joint {
    pub fn new(name: impl Into<String>, parent_index: Option<usize>) -> Self {
        Self {
            name: name.into(),
            parent_index,
            children: Vec::new(),
            inverse_bind_matrix: Mat4::IDENTITY,
            local_transform: JointTransform::default(),
        }
    }
}

/// Transform for a single joint (TRS decomposition)
#[derive(Debug, Clone, Copy)]
pub struct JointTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for JointTransform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl JointTransform {
    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Convert to a 4x4 transformation matrix
    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// The skeleton hierarchy
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub joints: Vec<Joint>,
    pub root_joints: Vec<usize>,
}

impl Skeleton {
    pub fn new() -> Self {
        Self {
            joints: Vec::new(),
            root_joints: Vec::new(),
        }
    }

    /// Add a joint to the skeleton
    pub fn add_joint(&mut self, mut joint: Joint) -> usize {
        let index = self.joints.len();

        if let Some(parent_idx) = joint.parent_index {
            self.joints[parent_idx].children.push(index);
        } else {
            self.root_joints.push(index);
        }

        // Ensure joint has valid parent reference
        if joint.parent_index.is_some() && joint.parent_index.unwrap() >= self.joints.len() {
            joint.parent_index = None;
        }

        self.joints.push(joint);
        index
    }

    /// Find joint index by name
    pub fn find_joint(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime state of a skeleton (current pose)
#[derive(Debug, Clone)]
pub struct SkeletonState {
    /// Current local transforms for each joint
    pub local_transforms: Vec<JointTransform>,
    /// Computed skin matrices (joint_matrix * inverse_bind_matrix)
    pub skin_matrices: Vec<Mat4>,
    /// Cached global transforms
    global_transforms: Vec<Mat4>,
}

impl SkeletonState {
    /// Create a new skeleton state from a skeleton (uses bind pose)
    pub fn from_skeleton(skeleton: &Skeleton) -> Self {
        let joint_count = skeleton.joints.len();
        let local_transforms: Vec<_> = skeleton.joints.iter().map(|j| j.local_transform).collect();

        let mut state = Self {
            local_transforms,
            skin_matrices: vec![Mat4::IDENTITY; joint_count],
            global_transforms: vec![Mat4::IDENTITY; joint_count],
        };

        state.compute_skin_matrices(skeleton);
        state
    }

    /// Update skin matrices from current local transforms
    pub fn compute_skin_matrices(&mut self, skeleton: &Skeleton) {
        // Compute global transforms by traversing from roots
        for &root_idx in &skeleton.root_joints {
            self.compute_global_transform_recursive(skeleton, root_idx, Mat4::IDENTITY);
        }

        // Compute skin matrices: global * inverse_bind
        for (i, joint) in skeleton.joints.iter().enumerate() {
            self.skin_matrices[i] = self.global_transforms[i] * joint.inverse_bind_matrix;
        }
    }

    fn compute_global_transform_recursive(
        &mut self,
        skeleton: &Skeleton,
        joint_idx: usize,
        parent_global: Mat4,
    ) {
        let local_matrix = self.local_transforms[joint_idx].to_matrix();
        let global = parent_global * local_matrix;
        self.global_transforms[joint_idx] = global;

        // Recurse to children
        for &child_idx in &skeleton.joints[joint_idx].children {
            self.compute_global_transform_recursive(skeleton, child_idx, global);
        }
    }

    /// Reset to bind pose (default pose from skeleton)
    pub fn reset_to_bind_pose(&mut self, skeleton: &Skeleton) {
        for (i, joint) in skeleton.joints.iter().enumerate() {
            self.local_transforms[i] = joint.local_transform;
        }
    }

    /// Transform a vertex position using skinning
    pub fn transform_vertex(
        &self,
        position: Vec3,
        joint_indices: [u32; 4],
        joint_weights: [f32; 4],
    ) -> Vec3 {
        let mut result = Vec3::ZERO;

        for i in 0..4 {
            let weight = joint_weights[i];
            if weight > 0.0 {
                let joint_idx = joint_indices[i] as usize;
                if joint_idx < self.skin_matrices.len() {
                    result += self.skin_matrices[joint_idx].transform_point3(position) * weight;
                }
            }
        }

        result
    }
}

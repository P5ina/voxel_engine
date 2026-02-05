//! Animation structures and playback

use glam::{Quat, Vec3};

use crate::model::skeleton::{JointTransform, Skeleton, SkeletonState};

/// Interpolation method for keyframes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
    CubicSpline,
}

/// A single keyframe in an animation channel
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time: f32,
    pub value: KeyframeValue,
}

/// Value stored in a keyframe
#[derive(Debug, Clone)]
pub enum KeyframeValue {
    Translation(Vec3),
    Rotation(Quat),
    Scale(Vec3),
}

/// An animation channel targeting a specific joint property
#[derive(Debug, Clone)]
pub struct AnimationChannel {
    pub joint_index: usize,
    pub keyframes: Vec<Keyframe>,
    pub interpolation: Interpolation,
}

impl AnimationChannel {
    /// Sample the channel at a given time
    pub fn sample(&self, time: f32) -> Option<KeyframeValue> {
        if self.keyframes.is_empty() {
            return None;
        }

        // Find the two keyframes to interpolate between
        let mut prev_idx = 0;
        let mut next_idx = 0;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= time {
                prev_idx = i;
            }
            if kf.time >= time {
                next_idx = i;
                break;
            }
            next_idx = i;
        }

        let prev_kf = &self.keyframes[prev_idx];
        let next_kf = &self.keyframes[next_idx];

        // Calculate interpolation factor
        let t = if prev_idx == next_idx || prev_kf.time >= next_kf.time {
            0.0
        } else {
            (time - prev_kf.time) / (next_kf.time - prev_kf.time)
        };

        // Interpolate based on method
        match self.interpolation {
            Interpolation::Step => Some(prev_kf.value.clone()),
            Interpolation::Linear | Interpolation::CubicSpline => {
                // For now, treat cubic spline as linear
                match (&prev_kf.value, &next_kf.value) {
                    (KeyframeValue::Translation(a), KeyframeValue::Translation(b)) => {
                        Some(KeyframeValue::Translation(a.lerp(*b, t)))
                    }
                    (KeyframeValue::Rotation(a), KeyframeValue::Rotation(b)) => {
                        Some(KeyframeValue::Rotation(a.slerp(*b, t)))
                    }
                    (KeyframeValue::Scale(a), KeyframeValue::Scale(b)) => {
                        Some(KeyframeValue::Scale(a.lerp(*b, t)))
                    }
                    _ => None,
                }
            }
        }
    }
}

/// A complete animation clip
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: String,
    pub channels: Vec<AnimationChannel>,
    pub duration: f32,
}

impl AnimationClip {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channels: Vec::new(),
            duration: 0.0,
        }
    }

    /// Calculate duration from keyframes
    pub fn calculate_duration(&mut self) {
        self.duration = self
            .channels
            .iter()
            .flat_map(|c| c.keyframes.iter())
            .map(|kf| kf.time)
            .fold(0.0f32, |a, b| a.max(b));
    }

    /// Sample the animation at a given time and apply to skeleton state
    pub fn sample(&self, time: f32, skeleton: &Skeleton, state: &mut SkeletonState) {
        for channel in &self.channels {
            if let Some(value) = channel.sample(time) {
                let joint_idx = channel.joint_index;
                if joint_idx < state.local_transforms.len() {
                    let transform = &mut state.local_transforms[joint_idx];
                    match value {
                        KeyframeValue::Translation(t) => transform.translation = t,
                        KeyframeValue::Rotation(r) => transform.rotation = r,
                        KeyframeValue::Scale(s) => transform.scale = s,
                    }
                }
            }
        }
        state.compute_skin_matrices(skeleton);
    }
}

/// Animation player for managing animation playback
#[derive(Debug)]
pub struct AnimationPlayer {
    pub clips: Vec<AnimationClip>,
    pub current_clip: Option<usize>,
    pub time: f32,
    pub speed: f32,
    pub looping: bool,
    pub playing: bool,
}

impl AnimationPlayer {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            current_clip: None,
            time: 0.0,
            speed: 1.0,
            looping: true,
            playing: false,
        }
    }

    /// Add an animation clip
    pub fn add_clip(&mut self, clip: AnimationClip) -> usize {
        let index = self.clips.len();
        self.clips.push(clip);
        index
    }

    /// Play a specific clip by index
    pub fn play(&mut self, clip_index: usize) {
        if clip_index < self.clips.len() {
            self.current_clip = Some(clip_index);
            self.time = 0.0;
            self.playing = true;
        }
    }

    /// Play a clip by name, returns true if found
    pub fn play_by_name(&mut self, name: &str) -> bool {
        if let Some(idx) = self.clips.iter().position(|c| c.name == name) {
            self.play(idx);
            true
        } else {
            false
        }
    }

    /// Stop playback
    pub fn stop(&mut self) {
        self.playing = false;
        self.time = 0.0;
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Resume playback
    pub fn resume(&mut self) {
        self.playing = true;
    }

    /// Update the animation player
    pub fn update(&mut self, dt: f32, skeleton: &Skeleton, state: &mut SkeletonState) {
        if !self.playing {
            return;
        }

        let Some(clip_idx) = self.current_clip else {
            return;
        };

        let clip = &self.clips[clip_idx];

        // Advance time
        self.time += dt * self.speed;

        // Handle looping or clamping
        if self.time >= clip.duration {
            if self.looping {
                self.time = self.time % clip.duration;
            } else {
                self.time = clip.duration;
                self.playing = false;
            }
        }

        // Sample and apply animation
        clip.sample(self.time, skeleton, state);
    }

    /// Get the current animation name
    pub fn current_animation_name(&self) -> Option<&str> {
        self.current_clip
            .and_then(|idx| self.clips.get(idx))
            .map(|c| c.name.as_str())
    }

    /// Get normalized progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        self.current_clip
            .and_then(|idx| self.clips.get(idx))
            .map(|c| {
                if c.duration > 0.0 {
                    self.time / c.duration
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0)
    }
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self::new()
    }
}

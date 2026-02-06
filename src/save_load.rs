use std::sync::mpsc;

use glam::Vec3;

use crate::ui::{LoadingState, UiScreen};
use crate::world::{self, ChunkManager, load_big_world_fast, prepare_save, write_prepared_save};
use crate::{AppState, BigWorldGenMessage, BigWorldGenResult};

impl AppState {
    pub(crate) fn scan_worlds(&self) -> Vec<crate::ui::WorldInfo> {
        let mut worlds = Vec::new();
        let maps_dir = std::path::Path::new("maps");
        if !maps_dir.is_dir() {
            return worlds;
        }
        let Ok(entries) = std::fs::read_dir(maps_dir) else {
            return worlds;
        };
        for entry in entries.flatten() {
            let path = entry.path();

            // Region-based world: directory with world.meta
            let is_region_dir = path.is_dir() && path.join("world.meta").exists();
            // Legacy: .world file
            let is_legacy = path.extension().and_then(|e| e.to_str()) == Some("world");

            if !is_region_dir && !is_legacy {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let (size_mb, modified) = if is_region_dir {
                // Sum up all file sizes in the directory
                Self::scan_dir_stats(&path)
            } else {
                Self::scan_file_stats(&path)
            };

            worlds.push(crate::ui::WorldInfo {
                name,
                path: path.to_string_lossy().to_string(),
                size_mb,
                modified,
            });
        }
        worlds.sort_by(|a, b| a.name.cmp(&b.name));
        worlds
    }

    fn scan_file_stats(path: &std::path::Path) -> (f64, String) {
        match std::fs::metadata(path) {
            Ok(meta) => {
                let size = meta.len() as f64 / (1024.0 * 1024.0);
                let mod_time = Self::format_modified(&meta);
                (size, mod_time)
            }
            Err(_) => (0.0, "unknown".to_string()),
        }
    }

    fn scan_dir_stats(dir: &std::path::Path) -> (f64, String) {
        let mut total_size = 0u64;
        let mut latest_modified = None;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        total_size += meta.len();
                        if let Ok(mod_time) = meta.modified() {
                            latest_modified = Some(match latest_modified {
                                Some(prev) => std::cmp::max(prev, mod_time),
                                None => mod_time,
                            });
                        }
                    }
                } else if path.is_dir() {
                    // Recurse into regions/ subdirectory
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            if let Ok(meta) = std::fs::metadata(sub_entry.path()) {
                                total_size += meta.len();
                                if let Ok(mod_time) = meta.modified() {
                                    latest_modified = Some(match latest_modified {
                                        Some(prev) => std::cmp::max(prev, mod_time),
                                        None => mod_time,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        let size_mb = total_size as f64 / (1024.0 * 1024.0);
        let modified = latest_modified
            .and_then(|t| {
                let elapsed = t.elapsed().ok()?;
                let secs = elapsed.as_secs();
                if secs < 60 {
                    Some("just now".to_string())
                } else if secs < 3600 {
                    Some(format!("{}m ago", secs / 60))
                } else if secs < 86400 {
                    Some(format!("{}h ago", secs / 3600))
                } else {
                    Some(format!("{}d ago", secs / 86400))
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        (size_mb, modified)
    }

    fn format_modified(meta: &std::fs::Metadata) -> String {
        meta.modified()
            .ok()
            .and_then(|t| {
                let elapsed = t.elapsed().ok()?;
                let secs = elapsed.as_secs();
                if secs < 60 {
                    Some("just now".to_string())
                } else if secs < 3600 {
                    Some(format!("{}m ago", secs / 60))
                } else if secs < 86400 {
                    Some(format!("{}h ago", secs / 3600))
                } else {
                    Some(format!("{}d ago", secs / 86400))
                }
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub(crate) fn save_big_world_to_file(&mut self) {
        // Don't start a second save while one is already running
        if self.save_world_receiver.is_some() {
            log::warn!("[BigWorld] Save already in progress");
            return;
        }

        let Some(ref octree) = self.octree else {
            log::error!("[BigWorld] No octree to save");
            return;
        };

        let spawn = self.player.position.to_array();

        // Region-based save: write dirty regions + world.meta
        if let Some(ref mut region_mgr) = self.region_manager {
            log::info!("[BigWorld] Saving dirty regions...");

            // Save dirty regions synchronously (they're already compressed per-region)
            region_mgr.save_dirty_regions(&self.world);

            // Clone octree for background meta save
            let octree_clone = octree.clone();
            let world_dir = region_mgr.world_dir().to_path_buf();

            // Show loading screen
            self.loading_state = LoadingState::new("Saving world...", 0);
            self.prev_screen = self.ui_screen;
            self.ui_screen = UiScreen::Loading;
            self.grab_mouse(false);

            let (tx, rx) = mpsc::channel();
            self.save_world_receiver = Some(rx);

            std::thread::spawn(move || {
                log::info!("[BigWorld] Background save thread: writing world.meta");
                let result = world::region::save_world_meta(&world_dir, &octree_clone, spawn);
                let _ = tx.send((None, result));
            });
        } else {
            // Legacy monolithic save path (for worlds not yet converted)
            let octree = self.octree.take().unwrap();

            log::info!("[BigWorld] Compressing chunks for legacy save...");
            let prepared = prepare_save(&self.world);

            self.loading_state = LoadingState::new("Saving world...", 0);
            self.prev_screen = self.ui_screen;
            self.ui_screen = UiScreen::Loading;
            self.grab_mouse(false);

            let (tx, rx) = mpsc::channel();
            self.save_world_receiver = Some(rx);

            let path = format!("maps/{}.world", self.editor_state.level_name);
            std::thread::spawn(move || {
                log::info!("[BigWorld] Background save thread started (legacy)");
                let result = write_prepared_save(&path, prepared, &octree, 8, spawn);
                let _ = tx.send((Some(octree), result));
            });
        }
    }

    pub(crate) fn load_big_world_from_file(&mut self, path: &str) {
        let path_obj = std::path::Path::new(path);
        let is_region_dir = path_obj.is_dir() && path_obj.join("world.meta").exists();
        let is_legacy_file = path_obj.is_file();

        if !is_region_dir && !is_legacy_file {
            log::error!("[BigWorld] Not found: {}", path);
            return;
        }

        // Derive level name from directory/file name
        let level_name = path_obj
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Clear state
        self.pending_chunks.clear();
        self.pending_set.clear();
        self.octree = None;
        self.chunk_streamer = None;
        self.use_streaming = false;
        self.region_manager = None;
        self.streaming_mesh_rx = None;
        self.streaming_mesh_tx = None;
        self.streaming_inflight.clear();
        self.lod_mesh_rx = None;
        self.lod_mesh_tx = None;
        self.chunk_meshes.clear();
        self.world = ChunkManager::new();

        self.editor_state.level_name = level_name;

        // Setup loading state and switch to loading screen
        self.loading_state = LoadingState::new("Loading world...", 100);
        self.ui_screen = UiScreen::Loading;
        self.grab_mouse(false);

        // Spawn background thread for file loading + octree building
        let (tx, rx) = mpsc::channel();
        self.big_world_gen_receiver = Some(rx);

        let path = path.to_string();
        if is_region_dir {
            std::thread::spawn(move || {
                Self::load_region_world_background(path, tx);
            });
        } else {
            std::thread::spawn(move || {
                Self::load_big_world_background(path, tx);
            });
        }

        log::info!("[BigWorld] Background loading thread started");
    }

    /// Background thread: load chunks + octree from file, compute mesh tasks
    fn load_big_world_background(path: String, tx: mpsc::Sender<BigWorldGenMessage>) {
        let _ = tx.send(BigWorldGenMessage::Progress("Reading file...".into(), 0, 1));

        let loaded = match load_big_world_fast(&path) {
            Ok(loaded) => loaded,
            Err(e) => {
                log::error!("[BigWorld] Failed to load: {}", e);
                return; // Sender drops, receiver sees Disconnected
            }
        };

        let spawn_pos = Vec3::from_array(loaded.spawn_position);

        log::info!(
            "[BigWorld] File loaded: {} chunks, octree: {} nodes. Computing mesh tasks...",
            loaded.world.chunk_count(),
            loaded.octree.node_count(),
        );

        let _ = tx.send(BigWorldGenMessage::Progress(
            "Computing mesh tasks...".into(),
            0,
            1,
        ));
        let (mesh_tasks, lod0_count) =
            Self::compute_mesh_tasks(&loaded.world, &loaded.octree, spawn_pos);
        let octree = loaded.octree;

        let _ = tx.send(BigWorldGenMessage::Done(Box::new(BigWorldGenResult {
            world: loaded.world,
            octree,
            mesh_tasks,
            spawn_pos,
            lod0_count,
        })));
    }

    /// Background thread: load world.meta (octree only, no chunks) for region-based worlds.
    /// Chunks will stream from region files on demand.
    fn load_region_world_background(path: String, tx: mpsc::Sender<BigWorldGenMessage>) {
        let world_dir = std::path::PathBuf::from(&path);

        let _ = tx.send(BigWorldGenMessage::Progress(
            "Reading world.meta...".into(),
            0,
            1,
        ));

        let loaded_meta = match world::region::load_world_meta(&world_dir) {
            Ok(meta) => meta,
            Err(e) => {
                log::error!("[Region] Failed to load world.meta: {}", e);
                return;
            }
        };

        let spawn_pos = Vec3::from_array(loaded_meta.meta.spawn_position);
        let octree = loaded_meta.octree;

        log::info!(
            "[Region] world.meta loaded: octree {} nodes. Computing LOD mesh tasks...",
            octree.node_count(),
        );

        // No chunks loaded — empty ChunkManager. Chunks stream from region files.
        let world = ChunkManager::new();

        let _ = tx.send(BigWorldGenMessage::Progress(
            "Computing mesh tasks...".into(),
            0,
            1,
        ));

        // Only compute LOD mesh tasks (no LOD0 chunks loaded yet)
        let (mesh_tasks, lod0_count) = Self::compute_mesh_tasks(&world, &octree, spawn_pos);

        let _ = tx.send(BigWorldGenMessage::Done(Box::new(BigWorldGenResult {
            world,
            octree,
            mesh_tasks,
            spawn_pos,
            lod0_count,
        })));
    }
}

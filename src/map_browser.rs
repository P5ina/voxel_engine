use crate::AppState;
use crate::ui::PlayableWorld;

impl AppState {
    pub(crate) fn scan_playable_worlds(&self) -> Vec<PlayableWorld> {
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
            let is_region_dir = path.is_dir() && path.join("world.meta").exists();
            let is_legacy = path.extension().and_then(|e| e.to_str()) == Some("world");
            if !is_region_dir && !is_legacy {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            worlds.push(PlayableWorld {
                name,
                path: path.to_string_lossy().to_string(),
            });
        }

        worlds.sort_by(|a, b| a.name.cmp(&b.name));
        worlds
    }
}

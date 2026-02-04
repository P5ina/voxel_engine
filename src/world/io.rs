use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use super::chunk_manager::{ChunkManager, WorldData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Binary,
    Json,
}

#[derive(Debug)]
pub enum WorldError {
    Io(std::io::Error),
    Bincode(bincode::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldError::Io(e) => write!(f, "IO error: {}", e),
            WorldError::Bincode(e) => write!(f, "Bincode error: {}", e),
            WorldError::Json(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for WorldError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WorldError::Io(e) => Some(e),
            WorldError::Bincode(e) => Some(e),
            WorldError::Json(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for WorldError {
    fn from(e: std::io::Error) -> Self {
        WorldError::Io(e)
    }
}

impl From<bincode::Error> for WorldError {
    fn from(e: bincode::Error) -> Self {
        WorldError::Bincode(e)
    }
}

impl From<serde_json::Error> for WorldError {
    fn from(e: serde_json::Error) -> Self {
        WorldError::Json(e)
    }
}

impl ChunkManager {
    /// Save the world to a file
    pub fn save(&self, path: impl AsRef<Path>, format: SaveFormat) -> Result<(), WorldError> {
        let path = path.as_ref();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = self.to_world_data();

        match format {
            SaveFormat::Binary => {
                let file = File::create(path)?;
                let writer = BufWriter::new(file);
                bincode::serialize_into(writer, &data)?;
            }
            SaveFormat::Json => {
                let file = File::create(path)?;
                let writer = BufWriter::new(file);
                serde_json::to_writer_pretty(writer, &data)?;
            }
        }

        Ok(())
    }

    /// Load a world from a file (auto-detects format from extension)
    pub fn load(path: impl AsRef<Path>) -> Result<ChunkManager, WorldError> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let data: WorldData = if path.extension().map_or(false, |ext| ext == "json") {
            serde_json::from_reader(reader)?
        } else {
            bincode::deserialize_from(reader)?
        };

        Ok(ChunkManager::from_world_data(data))
    }

    /// Load a world from a file with explicit format
    pub fn load_with_format(
        path: impl AsRef<Path>,
        format: SaveFormat,
    ) -> Result<ChunkManager, WorldError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let data: WorldData = match format {
            SaveFormat::Binary => bincode::deserialize_from(reader)?,
            SaveFormat::Json => serde_json::from_reader(reader)?,
        };

        Ok(ChunkManager::from_world_data(data))
    }
}

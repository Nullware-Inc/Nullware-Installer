//! What the installer has installed, so it can update and uninstall later.

use crate::platform::Format;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub app: String,
    pub version: String,
    pub format: Format,
    /// Files/dirs we created ourselves (portable, AppImage, shortcuts…).
    #[serde(default)]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub installed: Vec<Installed>,
}

pub fn dir() -> PathBuf {
    let d = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Nullware")
        .join("installer");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn file() -> PathBuf {
    dir().join("installed.json")
}

impl State {
    pub fn load() -> Self {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::write(file(), serde_json::to_string_pretty(self).unwrap())
    }

    pub fn get(&self, app: &str) -> Option<&Installed> {
        self.installed.iter().find(|i| i.app == app)
    }

    pub fn put(&mut self, entry: Installed) {
        self.installed.retain(|i| i.app != entry.app);
        self.installed.push(entry);
    }

    pub fn remove(&mut self, app: &str) {
        self.installed.retain(|i| i.app != app);
    }
}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ── Runtime management types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedRuntime {
    pub build: u32,
    pub tag_name: String,
    pub backend_id: String,
    pub backend_label: String,
    pub asset_name: String,
    /// Subdirectory name under the runtimes base dir
    pub dir_name: String,
    pub installed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRuntime {
    pub label: String,
    pub binary_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ActiveRuntime {
    Managed { build: u32 },
    Custom { index: usize },
    #[default]
    None,
}

// ── App config ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    // ── Legacy runtime fields (migrated on load, not serialized) ──
    #[serde(default, skip_serializing)]
    pub runtime_dir: Option<PathBuf>,
    #[serde(default, skip_serializing)]
    pub runtime_build: Option<u32>,
    #[serde(default, skip_serializing)]
    pub runtime_backend: Option<String>,

    // ── New runtime fields ──
    #[serde(default)]
    pub managed_runtimes: Vec<ManagedRuntime>,
    #[serde(default)]
    pub custom_runtimes: Vec<CustomRuntime>,
    #[serde(default)]
    pub active_runtime: ActiveRuntime,
    #[serde(default)]
    pub auto_delete_old_runtimes: bool,

    // ── Models ──
    #[serde(default, skip_serializing)]
    pub models_dir: Option<PathBuf>,
    #[serde(default)]
    pub model_dirs: Vec<PathBuf>,
    pub download_dir: Option<PathBuf>,

    // ── Updates ──
    pub last_update_check: Option<i64>,
    pub latest_known_build: Option<u32>,
    pub auto_check_updates: bool,

    // ── Preferences ──
    #[serde(default)]
    pub favorite_models: Vec<String>,
    pub selected_model: Option<String>,
    #[serde(default)]
    pub wizard_completed: bool,
    /// Maps model file path → last-used preset name for that model
    #[serde(default)]
    pub model_presets: HashMap<String, String>,
}

impl AppConfig {
    pub fn config_path() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        Ok(data_dir.join("catapult").join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            let default_dir = Self::default_models_dir()?;
            return Ok(Self {
                auto_check_updates: true,
                model_dirs: vec![default_dir.clone()],
                download_dir: Some(default_dir),
                ..Default::default()
            });
        }
        let content = std::fs::read_to_string(&path)?;
        let mut config: Self = serde_json::from_str(&content)?;

        // ── Migrate legacy runtime fields ──
        if config.managed_runtimes.is_empty() && config.custom_runtimes.is_empty() {
            if let Some(ref dir) = config.runtime_dir {
                if config.runtime_backend.as_deref() == Some("custom") {
                    // Was a custom runtime
                    config.custom_runtimes.push(CustomRuntime {
                        label: dir.display().to_string(),
                        binary_path: crate::runtime::find_server_binary(dir)
                            .unwrap_or_else(|| dir.join("llama-server")),
                    });
                    config.active_runtime = ActiveRuntime::Custom { index: 0 };
                } else if let Some(build) = config.runtime_build {
                    // Was a managed runtime — keep its directory as-is
                    let dir_name = dir.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("b{}", build));
                    config.managed_runtimes.push(ManagedRuntime {
                        build,
                        tag_name: format!("b{}", build),
                        backend_id: config.runtime_backend.clone().unwrap_or_default(),
                        backend_label: config.runtime_backend.clone().unwrap_or_default().to_uppercase(),
                        asset_name: String::new(),
                        dir_name,
                        installed_at: 0,
                    });
                    config.active_runtime = ActiveRuntime::Managed { build };
                }
            }
            config.runtime_dir = None;
            config.runtime_build = None;
            config.runtime_backend = None;
        }

        // ── Migrate legacy models_dir ──
        if let Some(legacy) = config.models_dir.take() {
            if config.model_dirs.is_empty() {
                config.model_dirs.push(legacy.clone());
            }
            if config.download_dir.is_none() {
                config.download_dir = Some(legacy);
            }
        }
        if config.model_dirs.is_empty() {
            let default_dir = Self::default_models_dir()?;
            config.model_dirs.push(default_dir.clone());
            if config.download_dir.is_none() {
                config.download_dir = Some(default_dir);
            }
        }
        if config.download_dir.is_none() {
            config.download_dir = Some(Self::default_models_dir()?);
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    // ── Path helpers ─────────────────────────────────────────────────────────

    pub fn default_models_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        Ok(data_dir.join("catapult").join("models"))
    }

    pub fn default_runtime_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        Ok(data_dir.join("catapult").join("runtime"))
    }

    pub fn runtimes_base_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        Ok(data_dir.join("catapult").join("runtimes"))
    }

    pub fn models_dir(&self) -> Result<PathBuf> {
        match &self.download_dir {
            Some(p) => Ok(p.clone()),
            None => Self::default_models_dir(),
        }
    }

    pub fn all_model_dirs(&self) -> Vec<PathBuf> {
        if self.model_dirs.is_empty() {
            Self::default_models_dir().into_iter().collect()
        } else {
            self.model_dirs.clone()
        }
    }

    /// Returns the directory of the active runtime (for find_server_binary).
    pub fn runtime_dir(&self) -> Result<PathBuf> {
        match &self.active_runtime {
            ActiveRuntime::Managed { build } => {
                if let Some(mr) = self.managed_runtimes.iter().find(|r| r.build == *build) {
                    // Check both new runtimes/ dir and legacy runtime/ dir
                    let new_dir = Self::runtimes_base_dir()?.join(&mr.dir_name);
                    if new_dir.exists() {
                        return Ok(new_dir);
                    }
                    // Fallback to legacy dir
                    let legacy_dir = Self::default_runtime_dir()?;
                    if legacy_dir.exists() {
                        return Ok(legacy_dir);
                    }
                    Ok(new_dir) // return new dir even if it doesn't exist yet
                } else {
                    Self::default_runtime_dir()
                }
            }
            ActiveRuntime::Custom { index } => {
                if let Some(cr) = self.custom_runtimes.get(*index) {
                    Ok(cr.binary_path.parent()
                        .unwrap_or(&cr.binary_path)
                        .to_path_buf())
                } else {
                    anyhow::bail!("Custom runtime index {} not found", index)
                }
            }
            ActiveRuntime::None => Self::default_runtime_dir(),
        }
    }

    pub fn presets_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        Ok(data_dir.join("catapult").join("presets"))
    }

    // ── Runtime helpers ──────────────────────────────────────────────────────

    pub fn active_build(&self) -> Option<u32> {
        match &self.active_runtime {
            ActiveRuntime::Managed { build } => Some(*build),
            _ => None,
        }
    }

    pub fn is_managed_runtime(&self) -> bool {
        matches!(self.active_runtime, ActiveRuntime::Managed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_presets_defaults_empty() {
        let cfg = AppConfig::default();
        assert!(cfg.model_presets.is_empty());
    }

    #[test]
    fn model_presets_round_trips_through_json() {
        let mut cfg = AppConfig::default();
        cfg.model_presets.insert("/home/user/models/foo.gguf".to_string(), "mypreset".to_string());
        cfg.model_presets.insert("/home/user/models/bar.gguf".to_string(), "another".to_string());

        let json = serde_json::to_string(&cfg).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.model_presets.get("/home/user/models/foo.gguf").map(|s| s.as_str()), Some("mypreset"));
        assert_eq!(restored.model_presets.get("/home/user/models/bar.gguf").map(|s| s.as_str()), Some("another"));
    }

    #[test]
    fn model_presets_missing_from_json_defaults_to_empty() {
        // Old config.json without the model_presets key should deserialize cleanly
        let json = r#"{"auto_check_updates":false,"wizard_completed":true,"favorite_models":[]}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.model_presets.is_empty());
    }
}

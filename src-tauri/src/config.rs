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
    Managed {
        build: u32,
        #[serde(default)]
        backend_id: String,
    },
    Custom { index: usize },
    #[default]
    None,
}

fn default_true() -> bool { true }

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
    #[serde(default = "default_true")]
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
    /// Preferred GGUF source owners on HuggingFace, in priority order.
    #[serde(default)]
    pub preferred_owners: Vec<String>,
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
                    config.active_runtime = ActiveRuntime::Managed { build, backend_id: config.runtime_backend.clone().unwrap_or_default() };
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
            ActiveRuntime::Managed { build, backend_id } => {
                // Match on both build and backend_id; fall back to build-only for legacy configs
                let mr = if backend_id.is_empty() {
                    self.managed_runtimes.iter().find(|r| r.build == *build)
                } else {
                    self.managed_runtimes.iter().find(|r| r.build == *build && r.backend_id == *backend_id)
                };
                if let Some(mr) = mr {
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
            ActiveRuntime::Managed { build, .. } => Some(*build),
            _ => None,
        }
    }

    pub fn active_backend_id(&self) -> Option<&str> {
        match &self.active_runtime {
            ActiveRuntime::Managed { backend_id, .. } => Some(backend_id.as_str()),
            _ => None,
        }
    }

    pub fn is_managed_runtime(&self) -> bool {
        matches!(self.active_runtime, ActiveRuntime::Managed { .. })
    }

    /// Returns the effective preferred owners list, falling back to defaults if empty.
    pub fn effective_owners(&self) -> Vec<String> {
        if self.preferred_owners.is_empty() {
            crate::huggingface::DEFAULT_PREFERRED_OWNERS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            self.preferred_owners.clone()
        }
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

    #[test]
    fn preferred_owners_defaults_empty() {
        let cfg = AppConfig::default();
        assert!(cfg.preferred_owners.is_empty());
    }

    #[test]
    fn preferred_owners_round_trips_through_json() {
        let mut cfg = AppConfig::default();
        cfg.preferred_owners = vec!["bartowski".to_string(), "unsloth".to_string()];

        let json = serde_json::to_string(&cfg).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.preferred_owners, vec!["bartowski", "unsloth"]);
    }

    #[test]
    fn preferred_owners_missing_from_json_defaults_to_empty() {
        let json = r#"{"auto_check_updates":false,"wizard_completed":true,"favorite_models":[]}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.preferred_owners.is_empty());
    }

    #[test]
    fn effective_owners_uses_defaults_when_empty() {
        let cfg = AppConfig::default();
        let effective = cfg.effective_owners();
        assert_eq!(
            effective,
            crate::huggingface::DEFAULT_PREFERRED_OWNERS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn effective_owners_uses_custom_when_set() {
        let mut cfg = AppConfig::default();
        cfg.preferred_owners = vec!["myorg".to_string(), "another".to_string()];
        assert_eq!(cfg.effective_owners(), vec!["myorg", "another"]);
    }

    // ── Config round-trip integrity tests ──
    //
    // These ensure that serialize → deserialize preserves every user-facing
    // field. A regression here (e.g. accidental skip_serializing) would cause
    // config erasure on the next app restart.

    /// Build a config with every field set to a distinguishable non-default
    /// value so we can detect if any field silently disappears.
    fn fully_populated_config() -> AppConfig {
        AppConfig {
            // Legacy fields are skip_serializing — intentionally omitted.
            managed_runtimes: vec![ManagedRuntime {
                build: 8000,
                tag_name: "b8000".to_string(),
                backend_id: "cuda".to_string(),
                backend_label: "CUDA 12.4".to_string(),
                asset_name: "llama-b8000-cuda.zip".to_string(),
                dir_name: "b8000-cuda".to_string(),
                installed_at: 1700000000,
            }],
            custom_runtimes: vec![CustomRuntime {
                label: "my-build".to_string(),
                binary_path: PathBuf::from("/opt/llama/llama-server"),
            }],
            active_runtime: ActiveRuntime::Managed {
                build: 8000,
                backend_id: "cuda".to_string(),
            },
            auto_delete_old_runtimes: true,
            model_dirs: vec![
                PathBuf::from("/data/models"),
                PathBuf::from("/extra/models"),
            ],
            download_dir: Some(PathBuf::from("/data/models")),
            last_update_check: Some(1700000000),
            latest_known_build: Some(9000),
            auto_check_updates: false, // non-default (default is true)
            favorite_models: vec![
                "model-alpha".to_string(),
                "model-beta".to_string(),
                "model-gamma".to_string(),
            ],
            selected_model: Some("/data/models/foo.gguf".to_string()),
            wizard_completed: true,
            model_presets: {
                let mut m = HashMap::new();
                m.insert("/data/models/foo.gguf".to_string(), "fast".to_string());
                m.insert("/data/models/bar.gguf".to_string(), "quality".to_string());
                m
            },
            preferred_owners: vec!["bartowski".to_string(), "unsloth".to_string()],
            ..Default::default()
        }
    }

    /// Serialize → deserialize must preserve every non-legacy field.
    /// If a new field is added to AppConfig without #[serde(default)] or with
    /// accidental skip_serializing, this test will catch it.
    #[test]
    fn config_round_trip_preserves_all_fields() {
        let original = fully_populated_config();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();

        // ── Runtime fields ──
        assert_eq!(restored.managed_runtimes.len(), 1);
        assert_eq!(restored.managed_runtimes[0].build, 8000);
        assert_eq!(restored.managed_runtimes[0].backend_id, "cuda");
        assert_eq!(restored.managed_runtimes[0].backend_label, "CUDA 12.4");
        assert_eq!(restored.managed_runtimes[0].asset_name, "llama-b8000-cuda.zip");
        assert_eq!(restored.managed_runtimes[0].dir_name, "b8000-cuda");
        assert_eq!(restored.managed_runtimes[0].installed_at, 1700000000);

        assert_eq!(restored.custom_runtimes.len(), 1);
        assert_eq!(restored.custom_runtimes[0].label, "my-build");
        assert_eq!(restored.custom_runtimes[0].binary_path, PathBuf::from("/opt/llama/llama-server"));

        assert_eq!(restored.active_runtime, ActiveRuntime::Managed {
            build: 8000,
            backend_id: "cuda".to_string(),
        });
        assert!(restored.auto_delete_old_runtimes);

        // ── Model fields ──
        assert_eq!(restored.model_dirs, vec![
            PathBuf::from("/data/models"),
            PathBuf::from("/extra/models"),
        ]);
        assert_eq!(restored.download_dir, Some(PathBuf::from("/data/models")));

        // ── Update fields ──
        assert_eq!(restored.last_update_check, Some(1700000000));
        assert_eq!(restored.latest_known_build, Some(9000));
        assert!(!restored.auto_check_updates, "auto_check_updates should be false, not reset to default true");

        // ── Preference fields ──
        assert_eq!(restored.favorite_models, vec!["model-alpha", "model-beta", "model-gamma"]);
        assert_eq!(restored.selected_model.as_deref(), Some("/data/models/foo.gguf"));
        assert!(restored.wizard_completed);
        assert_eq!(restored.model_presets.len(), 2);
        assert_eq!(restored.model_presets.get("/data/models/foo.gguf").map(|s| s.as_str()), Some("fast"));
        assert_eq!(restored.model_presets.get("/data/models/bar.gguf").map(|s| s.as_str()), Some("quality"));
        assert_eq!(restored.preferred_owners, vec!["bartowski", "unsloth"]);
    }

    /// Legacy fields with skip_serializing must NOT appear in JSON output.
    /// They only exist for migration on load.
    #[test]
    fn config_legacy_fields_not_serialized() {
        let mut cfg = fully_populated_config();
        cfg.runtime_dir = Some(PathBuf::from("/old/runtime"));
        cfg.runtime_build = Some(3000);
        cfg.runtime_backend = Some("cuda".to_string());
        cfg.models_dir = Some(PathBuf::from("/old/models"));

        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("runtime_dir"), "legacy runtime_dir should not be serialized");
        assert!(!json.contains("runtime_build"), "legacy runtime_build should not be serialized");
        assert!(!json.contains("runtime_backend"), "legacy runtime_backend should not be serialized");
        assert!(!json.contains("models_dir"), "legacy models_dir should not be serialized");
    }

    /// Simulates the exact scenario that caused the config erasure bug:
    /// a "stale" config (snapshot before changes) is serialized and then
    /// deserialized — verify that deserializing a stale snapshot cannot
    /// silently produce an empty/default config.
    #[test]
    fn config_stale_snapshot_still_deserializes_correctly() {
        let populated = fully_populated_config();
        let snapshot_json = serde_json::to_string(&populated).unwrap();

        // Simulate: "live" config gets new favorites added
        let mut live = fully_populated_config();
        live.favorite_models.push("model-delta".to_string());
        live.wizard_completed = true;

        // The stale snapshot is deserialized (this is what the old bug did)
        let stale: AppConfig = serde_json::from_str(&snapshot_json).unwrap();

        // The stale config should still have ALL original data —
        // it should not be empty/default
        assert!(stale.wizard_completed, "stale snapshot lost wizard_completed");
        assert_eq!(stale.favorite_models.len(), 3, "stale snapshot lost favorites");
        assert_eq!(stale.managed_runtimes.len(), 1, "stale snapshot lost runtimes");
        assert!(!stale.auto_check_updates, "stale snapshot reset auto_check_updates to default");
    }

    /// Canary: verify that the real config file is not modified by this test
    /// suite. If this fails, a test is accidentally calling config.save() on
    /// a default/test AppConfig, overwriting the user's real data.
    ///
    /// Run this test LAST by giving it a name that sorts after all others.
    #[test]
    fn zzz_canary_tests_did_not_corrupt_real_config() {
        let path = match AppConfig::config_path() {
            Ok(p) => p,
            Err(_) => return, // can't determine path, skip
        };
        if !path.exists() {
            return; // no config file, nothing to check
        }
        let content = std::fs::read_to_string(&path).unwrap();
        let config: AppConfig = serde_json::from_str(&content).unwrap();

        // The test suite uses build numbers like 3000-5000 and installed_at=1000.
        // If we find those exact values in the real config, a test wrote to it.
        for rt in &config.managed_runtimes {
            assert!(rt.installed_at != 1000 || rt.build > 6000,
                "REAL CONFIG CORRUPTED BY TESTS: found test fixture data \
                 (build={}, installed_at=1000) in {}. A test is calling \
                 config.save() on a test AppConfig.",
                rt.build, path.display());
        }
    }
}

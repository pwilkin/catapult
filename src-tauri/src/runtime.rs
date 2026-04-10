use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub installed: bool,
    pub build: Option<u32>,
    pub backend: Option<String>,
    pub path: Option<PathBuf>,
    pub server_binary: Option<PathBuf>,
    pub runtime_type: String, // "managed", "custom", "none"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub name: String,
    pub published_at: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub build: u32,
    pub published_at: String,
    pub available_assets: Vec<AssetOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetOption {
    pub name: String,
    pub backend_id: String,
    pub backend_label: String,
    pub platform: String,
    pub download_url: String,
    pub size_mb: u64,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub status: String,
}

pub fn get_runtime_info(config: &AppConfig) -> Result<RuntimeInfo> {
    use crate::config::ActiveRuntime;

    let runtime_type = match &config.active_runtime {
        ActiveRuntime::Managed { .. } => "managed",
        ActiveRuntime::Custom { .. } => "custom",
        ActiveRuntime::None => "none",
    };

    let (build, backend) = match &config.active_runtime {
        ActiveRuntime::Managed { build, backend_id } => {
            let mr = if backend_id.is_empty() {
                config.managed_runtimes.iter().find(|r| r.build == *build)
            } else {
                config.managed_runtimes.iter().find(|r| r.build == *build && r.backend_id == *backend_id)
            };
            (Some(*build), mr.map(|r| r.backend_label.clone()))
        }
        ActiveRuntime::Custom { index } => {
            let label = config.custom_runtimes.get(*index).map(|c| c.label.clone());
            (None, label)
        }
        ActiveRuntime::None => (None, None),
    };

    let runtime_dir = match config.runtime_dir() {
        Ok(d) => d,
        Err(_) => {
            return Ok(RuntimeInfo {
                installed: false,
                build,
                backend,
                path: None,
                server_binary: None,
                runtime_type: runtime_type.to_string(),
            });
        }
    };

    if !runtime_dir.exists() {
        return Ok(RuntimeInfo {
            installed: false,
            build,
            backend,
            path: None,
            server_binary: None,
            runtime_type: runtime_type.to_string(),
        });
    }

    let server_binary = find_server_binary(&runtime_dir);

    Ok(RuntimeInfo {
        installed: server_binary.is_some(),
        build,
        backend,
        path: Some(runtime_dir),
        server_binary,
        runtime_type: runtime_type.to_string(),
    })
}

pub fn find_server_binary(runtime_dir: &Path) -> Option<PathBuf> {
    let target = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    // Recursive search — archives often extract into a nested directory
    find_file_recursive(runtime_dir, target, 3)
}

pub fn find_chat_binary(runtime_dir: &Path) -> Option<PathBuf> {
    let target = if cfg!(target_os = "windows") {
        "llama-cli.exe"
    } else {
        "llama-cli"
    };
    find_file_recursive(runtime_dir, target, 3)
}

pub fn find_file_recursive(dir: &Path, name: &str, max_depth: u32) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
    }
    // Recurse into subdirectories
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, name, max_depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

pub async fn fetch_latest_release(
    client: &reqwest::Client,
    available_backend_ids: &[String],
) -> Result<ReleaseInfo> {
    let url = "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest";
    let response = client
        .get(url)
        .header("User-Agent", "catapult-launcher/0.1")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .context("Failed to fetch GitHub release")?;

    if !response.status().is_success() {
        anyhow::bail!("GitHub API returned status {}", response.status());
    }

    let release: GithubRelease = response.json().await.context("Failed to parse GitHub release JSON")?;
    parse_release(release, available_backend_ids)
}

fn parse_release(release: GithubRelease, available_backend_ids: &[String]) -> Result<ReleaseInfo> {
    let build = release
        .tag_name
        .trim_start_matches('b')
        .parse::<u32>()
        .context("Cannot parse build number from tag")?;

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let mut assets: Vec<AssetOption> = release
        .assets
        .iter()
        .filter(|a| a.name.ends_with(".zip") || a.name.ends_with(".tar.gz"))
        .filter_map(|a| score_asset(&a.name, os, arch, a.browser_download_url.clone(), a.size, available_backend_ids))
        .collect();

    // Sort by score descending
    assets.sort_by(|a, b| b.score.cmp(&a.score));

    Ok(ReleaseInfo {
        tag_name: release.tag_name,
        build,
        published_at: release.published_at,
        available_assets: assets,
    })
}

/// Score an asset filename for the current platform/hardware.
/// Returns None if the asset is clearly for a different platform.
fn score_asset(
    name: &str,
    os: &str,
    arch: &str,
    url: String,
    size: u64,
    available_backend_ids: &[String],
) -> Option<AssetOption> {
    let lower = name.to_lowercase();

    // Skip non-binary assets (SHA checksums, source archives, etc.)
    if lower.contains("sha256") || lower.contains(".tar.gz.sig") || lower.contains("source") {
        return None;
    }

    // Platform matching
    let platform_match = match os {
        "linux" => lower.contains("linux") || lower.contains("ubuntu"),
        "windows" => lower.contains("win"),
        "macos" => lower.contains("macos") || lower.contains("darwin"),
        _ => false,
    };
    if !platform_match {
        return None;
    }

    // Architecture matching
    let arch_match = match arch {
        "x86_64" => lower.contains("x64") || lower.contains("amd64") || lower.contains("x86_64"),
        "aarch64" => lower.contains("arm64") || lower.contains("aarch64"),
        _ => true, // allow if unknown
    };
    if !arch_match {
        return None;
    }

    // Detect backend from asset name
    let (backend_id, backend_label, base_score) = detect_asset_backend(&lower, os);

    // Penalize backends that are not available on this system.
    // CPU variants are always usable; accelerated backends need a match.
    let backend_available = backend_id.starts_with("cpu")
        || available_backend_ids.iter().any(|b| backend_id.starts_with(b.as_str()));
    let score = if backend_available { base_score } else { base_score - 200 };

    let size_mb = size / (1024 * 1024);

    Some(AssetOption {
        name: name.to_string(),
        backend_id,
        backend_label,
        platform: os.to_string(),
        download_url: url,
        size_mb,
        score,
    })
}

fn detect_asset_backend(lower: &str, os: &str) -> (String, String, i32) {
    if lower.contains("cuda") {
        // Extract CUDA version if present
        let version = extract_cuda_version(lower).unwrap_or_default();
        (
            "cuda".to_string(),
            format!("CUDA{}", version),
            100,
        )
    } else if lower.contains("rocm") || lower.contains("hip") {
        ("rocm".to_string(), "ROCm/HIP".to_string(), 90)
    } else if lower.contains("metal") {
        ("metal".to_string(), "Metal".to_string(), 95)
    } else if lower.contains("vulkan") {
        ("vulkan".to_string(), "Vulkan".to_string(), 70)
    } else if lower.contains("sycl") {
        ("sycl".to_string(), "SYCL".to_string(), 60)
    } else if lower.contains("openvino") {
        ("openvino".to_string(), "OpenVINO".to_string(), 50)
    } else if lower.contains("noavx") || lower.contains("no-avx") {
        ("cpu-noavx".to_string(), "CPU (no AVX)".to_string(), 10)
    } else if lower.contains("avx512") {
        ("cpu-avx512".to_string(), "CPU (AVX-512)".to_string(), 30)
    } else if lower.contains("avx2") {
        ("cpu-avx2".to_string(), "CPU (AVX2)".to_string(), 25)
    } else if lower.contains("avx") {
        ("cpu-avx".to_string(), "CPU (AVX)".to_string(), 20)
    } else if os == "macos" {
        // macOS builds without explicit backend use Metal
        ("metal".to_string(), "Metal".to_string(), 95)
    } else {
        // Generic CPU build
        ("cpu".to_string(), "CPU".to_string(), 20)
    }
}

fn extract_cuda_version(lower: &str) -> Option<String> {
    // Match patterns like "cu12.4", "cu124", "cu11.8"
    let re = regex::Regex::new(r"cu(\d+)\.?(\d*)").ok()?;
    let caps = re.captures(lower)?;
    let major = caps.get(1)?.as_str();
    let minor = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    if minor.is_empty() {
        Some(format!(" {}", major))
    } else {
        Some(format!(" {}.{}", major, minor))
    }
}

/// Result of a successful runtime download, ready to be registered into config.
#[derive(Debug, Clone)]
pub struct DownloadedRuntime {
    pub managed_runtime: crate::config::ManagedRuntime,
}

pub async fn download_runtime(
    client: &reqwest::Client,
    asset: &AssetOption,
    tag_name: &str,
    progress_cb: impl Fn(DownloadProgress),
) -> Result<DownloadedRuntime> {
    // Parse build number from tag
    let build = tag_name
        .trim_start_matches('b')
        .parse::<u32>()
        .context("Cannot parse build number from tag")?;

    // Create versioned subdirectory
    let dir_name = format!("b{}-{}", build, asset.backend_id);
    let base_dir = AppConfig::runtimes_base_dir()?;
    let runtime_dir = base_dir.join(&dir_name);
    std::fs::create_dir_all(&runtime_dir)?;

    let download_id = "runtime".to_string();
    let tmp_path = runtime_dir.join(format!("__download__{}", &asset.name));

    let response = client
        .get(&asset.download_url)
        .header("User-Agent", "catapult-launcher/0.1")
        .send()
        .await
        .context("Failed to start download")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status {}", response.status());
    }

    let total_bytes = response.content_length().unwrap_or(asset.size_mb * 1024 * 1024);
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut downloaded: u64 = 0;

    use futures::StreamExt;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download stream error")?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        let percent = if total_bytes > 0 {
            (downloaded as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };

        progress_cb(DownloadProgress {
            id: download_id.clone(),
            bytes_downloaded: downloaded,
            total_bytes,
            percent,
            status: "downloading".to_string(),
        });
    }

    file.flush().await?;
    drop(file);

    progress_cb(DownloadProgress {
        id: download_id.clone(),
        bytes_downloaded: downloaded,
        total_bytes,
        percent: 100.0,
        status: "extracting".to_string(),
    });

    let name_lower = asset.name.to_lowercase();
    if name_lower.ends_with(".zip") {
        extract_zip(&tmp_path, &runtime_dir)?;
    } else if name_lower.ends_with(".tar.gz") {
        extract_tar_gz(&tmp_path, &runtime_dir)?;
    } else {
        anyhow::bail!("Unknown archive format: {}", asset.name);
    }

    let _ = std::fs::remove_file(&tmp_path);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    progress_cb(DownloadProgress {
        id: download_id,
        bytes_downloaded: downloaded,
        total_bytes,
        percent: 100.0,
        status: "done".to_string(),
    });

    Ok(DownloadedRuntime {
        managed_runtime: crate::config::ManagedRuntime {
            build,
            tag_name: tag_name.to_string(),
            backend_id: asset.backend_id.clone(),
            backend_label: asset.backend_label.clone(),
            asset_name: asset.name.clone(),
            dir_name,
            installed_at: now,
        },
    })
}

/// Register a downloaded runtime into the config: add it, set as active,
/// optionally auto-delete old runtimes of the same backend.
///
/// This only mutates the in-memory config. The caller is responsible for
/// calling `config.save()` afterwards (typically under a lock so that
/// concurrent changes are not lost).
pub fn register_downloaded_runtime(config: &mut AppConfig, downloaded: DownloadedRuntime) -> Result<()> {
    let rt = downloaded.managed_runtime;
    let build = rt.build;
    let backend_id = rt.backend_id.clone();

    // Add to managed runtimes (replace if same build+backend exists)
    config.managed_runtimes.retain(|r| !(r.build == build && r.backend_id == backend_id));
    config.managed_runtimes.push(rt);

    // Set as active
    config.active_runtime = crate::config::ActiveRuntime::Managed {
        build,
        backend_id: backend_id.clone(),
    };

    // Auto-delete old runtimes if configured (only same backend)
    if config.auto_delete_old_runtimes {
        let base_dir = AppConfig::runtimes_base_dir()?;
        let old_runtimes: Vec<_> = config.managed_runtimes.iter()
            .filter(|r| r.backend_id == backend_id && r.build != build)
            .map(|r| (r.build, r.backend_id.clone(), r.dir_name.clone()))
            .collect();

        for (old_build, old_backend, old_dir_name) in &old_runtimes {
            let old_dir = base_dir.join(old_dir_name);
            if old_dir.exists() {
                let _ = std::fs::remove_dir_all(&old_dir);
            }
            config.managed_runtimes.retain(|r| !(r.build == *old_build && r.backend_id == *old_backend));
        }
    }

    // Sort by build descending
    config.managed_runtimes.sort_by(|a, b| b.build.cmp(&a.build));

    Ok(())
}

pub fn delete_managed_runtime(build: u32, backend_id: &str, config: &mut AppConfig) -> Result<()> {
    let is_active = config.active_build() == Some(build)
        && config.active_backend_id().map_or(true, |id| id == backend_id || backend_id.is_empty());
    if is_active {
        anyhow::bail!("Cannot delete the active runtime. Switch to another first.");
    }
    if let Some(mr) = config.managed_runtimes.iter().find(|r| r.build == build && (backend_id.is_empty() || r.backend_id == backend_id)) {
        let dir = AppConfig::runtimes_base_dir()?.join(&mr.dir_name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        // Also try legacy dir if dir_name matches
        let legacy = AppConfig::default_runtime_dir()?;
        if legacy.exists() && mr.dir_name == legacy.file_name().unwrap_or_default().to_string_lossy() {
            let _ = std::fs::remove_dir_all(&legacy);
        }
    }
    config.managed_runtimes.retain(|r| !(r.build == build && (backend_id.is_empty() || r.backend_id == backend_id)));
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut zip_file = archive.by_index(i)?;
        let out_path = match zip_file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };

        if zip_file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut zip_file, &mut out_file)?;

            // Preserve executable permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = zip_file.unix_mode() {
                    std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode))?;
                }
            }
        }
    }

    Ok(())
}

fn extract_tar_gz(tar_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(tar_path)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest)?;
    Ok(())
}

pub fn set_custom_runtime(path: &Path, config: &mut AppConfig) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    let server = find_server_binary(path);
    if server.is_none() {
        anyhow::bail!("No llama-server binary found at {}", path.display());
    }

    let binary = server.unwrap();
    let label = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom".to_string());

    // Add to custom runtimes if not already there
    let index = if let Some(idx) = config.custom_runtimes.iter().position(|c| c.binary_path == binary) {
        idx
    } else {
        let idx = config.custom_runtimes.len();
        config.custom_runtimes.push(crate::config::CustomRuntime {
            label,
            binary_path: binary,
        });
        idx
    };
    config.active_runtime = crate::config::ActiveRuntime::Custom { index };
    Ok(())
}

/// A llama-server binary discovered within a custom directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomBuild {
    pub binary_path: PathBuf,
    /// Human-readable label, e.g. "build/bin/llama-server"
    pub label: String,
}

/// Result of scanning a directory for llama-server binaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub builds: Vec<CustomBuild>,
    /// True if the scanned directory is a llama.cpp source distribution root.
    pub is_source_distribution: bool,
}

/// Check whether a directory is a llama.cpp source distribution root
/// by looking for a CMakeLists.txt containing `project("llama.cpp"`.
fn is_llamacpp_source_dir(root: &Path) -> bool {
    let cmake = root.join("CMakeLists.txt");
    if let Ok(content) = std::fs::read_to_string(&cmake) {
        content.contains("project(\"llama.cpp\"")
    } else {
        false
    }
}

/// Scan a directory tree for all llama-server binaries (up to depth 5).
pub fn scan_for_builds(root: &Path) -> Result<ScanResult> {
    if !root.exists() {
        anyhow::bail!("Path does not exist: {}", root.display());
    }

    let target = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    let mut results = Vec::new();
    find_all_binaries_recursive(root, target, 5, root, &mut results);
    results.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(ScanResult {
        is_source_distribution: is_llamacpp_source_dir(root),
        builds: results,
    })
}

fn find_all_binaries_recursive(
    dir: &Path,
    name: &str,
    max_depth: u32,
    root: &Path,
    results: &mut Vec<CustomBuild>,
) {
    if max_depth == 0 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            // Label: "<root_dir_name> - <first_subdir>" e.g. "llama.cpp - build"
            let label = if let Ok(rel) = path.strip_prefix(root) {
                let root_name = root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let first_subdir = rel.components().next()
                    .map(|c| c.as_os_str().to_string_lossy().to_string());
                match first_subdir {
                    Some(sub) => format!("{} - {}", root_name, sub),
                    None => root_name,
                }
            } else {
                path.display().to_string()
            };
            results.push(CustomBuild {
                binary_path: path,
                label,
            });
        } else if path.is_dir() {
            // Skip hidden dirs (.git, .cache, etc.) and known-irrelevant dirs
            // to avoid scanning thousands of directories in source trees
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name.starts_with('.') || dir_name == "node_modules" || dir_name == "__pycache__" {
                    continue;
                }
            }
            find_all_binaries_recursive(&path, name, max_depth - 1, root, results);
        }
    }
}

/// Set a specific binary as the custom runtime.
#[allow(dead_code)]
pub fn set_custom_runtime_binary(binary_path: &Path, config: &mut AppConfig) -> Result<()> {
    if !binary_path.exists() {
        anyhow::bail!("Binary does not exist: {}", binary_path.display());
    }
    let label = binary_path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom".to_string());

    let index = if let Some(idx) = config.custom_runtimes.iter().position(|c| c.binary_path == binary_path) {
        idx
    } else {
        let idx = config.custom_runtimes.len();
        config.custom_runtimes.push(crate::config::CustomRuntime {
            label,
            binary_path: binary_path.to_path_buf(),
        });
        idx
    };
    config.active_runtime = crate::config::ActiveRuntime::Custom { index };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_asset_cuda_linux_x64() {
        let result = score_asset(
            "llama-b5000-bin-linux-x64-cuda-cu12.4.zip",
            "linux", "x86_64",
            "https://example.com/asset.zip".to_string(),
            100 * 1024 * 1024,
            &["cuda".to_string()],
        );
        let opt = result.expect("should match linux x64 cuda");
        assert_eq!(opt.backend_id, "cuda");
        assert!(opt.backend_label.contains("12.4"));
        assert_eq!(opt.score, 100);
        assert_eq!(opt.platform, "linux");
    }

    #[test]
    fn score_asset_rejects_wrong_os() {
        // Windows asset on Linux
        assert!(score_asset(
            "llama-b5000-bin-win-x64-cuda.zip",
            "linux", "x86_64", String::new(), 0, &[],
        ).is_none());

        // Linux asset on macOS
        assert!(score_asset(
            "llama-b5000-bin-linux-x64-cuda.zip",
            "macos", "aarch64", String::new(), 0, &[],
        ).is_none());
    }

    #[test]
    fn score_asset_rejects_wrong_arch() {
        assert!(score_asset(
            "llama-b5000-bin-linux-arm64-cuda.zip",
            "linux", "x86_64", String::new(), 0, &[],
        ).is_none());
    }

    #[test]
    fn score_asset_penalizes_unavailable_backend() {
        let result = score_asset(
            "llama-b5000-bin-linux-x64-cuda-cu12.4.zip",
            "linux", "x86_64", String::new(), 0,
            &["cpu".to_string()], // no CUDA available
        );
        let opt = result.expect("should still return, just penalized");
        assert!(opt.score < 0, "score should be negative: {}", opt.score);
    }

    #[test]
    fn score_asset_cpu_always_usable() {
        let result = score_asset(
            "llama-b5000-bin-linux-x64-avx2.zip",
            "linux", "x86_64", String::new(), 0,
            &[], // no backends available
        );
        let opt = result.expect("CPU assets should always be accepted");
        assert!(opt.score > 0);
        assert!(opt.backend_id.starts_with("cpu"));
    }

    #[test]
    fn score_asset_skips_sha_and_source() {
        assert!(score_asset(
            "llama-b5000-sha256sums.txt",
            "linux", "x86_64", String::new(), 0, &[],
        ).is_none());
        assert!(score_asset(
            "llama-b5000-source.tar.gz",
            "linux", "x86_64", String::new(), 0, &[],
        ).is_none());
    }

    #[test]
    fn detect_backend_all_variants() {
        let cases = vec![
            ("llama-cuda-cu12", "linux", "cuda", 100),
            ("llama-rocm", "linux", "rocm", 90),
            ("llama-hip", "linux", "rocm", 90),
            ("llama-metal", "macos", "metal", 95),
            ("llama-vulkan", "linux", "vulkan", 70),
            ("llama-sycl", "linux", "sycl", 60),
            ("llama-openvino", "linux", "openvino", 50),
            ("llama-avx512", "linux", "cpu-avx512", 30),
            ("llama-avx2", "linux", "cpu-avx2", 25),
            ("llama-avx", "linux", "cpu-avx", 20),
            ("llama-noavx", "linux", "cpu-noavx", 10),
            ("llama-generic", "linux", "cpu", 20),
            ("llama-generic", "macos", "metal", 95), // macOS default = Metal
        ];
        for (name, os, expected_id, expected_score) in cases {
            let (id, _label, score) = detect_asset_backend(name, os);
            assert_eq!(id, expected_id, "backend for '{}' on {}", name, os);
            assert_eq!(score, expected_score, "score for '{}' on {}", name, os);
        }
    }

    #[test]
    fn extract_cuda_version_patterns() {
        assert_eq!(extract_cuda_version("llama-cu12.4-x64"), Some(" 12.4".to_string()));
        assert_eq!(extract_cuda_version("llama-cu11.8"), Some(" 11.8".to_string()));
        assert_eq!(extract_cuda_version("llama-cu124"), Some(" 124".to_string()));
        assert_eq!(extract_cuda_version("llama-nocuda"), None);
        assert_eq!(extract_cuda_version("llama-generic"), None);
    }

    // ── Per-backend runtime management tests ──

    fn make_managed(build: u32, backend_id: &str) -> crate::config::ManagedRuntime {
        crate::config::ManagedRuntime {
            build,
            tag_name: format!("b{}", build),
            backend_id: backend_id.to_string(),
            backend_label: backend_id.to_uppercase(),
            asset_name: format!("llama-b{}-{}.zip", build, backend_id),
            dir_name: format!("b{}-{}", build, backend_id),
            installed_at: 1000,
        }
    }

    #[test]
    fn active_runtime_matches_build_and_backend() {
        let mut config = AppConfig::default();
        config.managed_runtimes = vec![
            make_managed(5000, "cuda"),
            make_managed(5000, "vulkan"),
        ];

        // Activate CUDA
        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "cuda".to_string(),
        };
        assert_eq!(config.active_build(), Some(5000));
        assert_eq!(config.active_backend_id(), Some("cuda"));

        // Activate Vulkan (same build, different backend)
        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "vulkan".to_string(),
        };
        assert_eq!(config.active_build(), Some(5000));
        assert_eq!(config.active_backend_id(), Some("vulkan"));
    }

    #[test]
    fn runtime_dir_distinguishes_backends() {
        // Create actual directories so runtime_dir() returns the new-style path
        let base = AppConfig::runtimes_base_dir().unwrap();
        let cuda_dir = base.join("b5000-cuda");
        let vulkan_dir = base.join("b5000-vulkan");
        std::fs::create_dir_all(&cuda_dir).unwrap();
        std::fs::create_dir_all(&vulkan_dir).unwrap();

        let mut config = AppConfig::default();
        config.managed_runtimes = vec![
            make_managed(5000, "cuda"),
            make_managed(5000, "vulkan"),
        ];

        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "cuda".to_string(),
        };
        let dir = config.runtime_dir().unwrap();
        assert!(dir.to_string_lossy().contains("b5000-cuda"),
            "expected b5000-cuda in {:?}", dir);

        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "vulkan".to_string(),
        };
        let dir = config.runtime_dir().unwrap();
        assert!(dir.to_string_lossy().contains("b5000-vulkan"),
            "expected b5000-vulkan in {:?}", dir);

        // Cleanup
        let _ = std::fs::remove_dir_all(&cuda_dir);
        let _ = std::fs::remove_dir_all(&vulkan_dir);
    }

    #[test]
    fn delete_runtime_targets_specific_backend() {
        let tmp = std::env::temp_dir().join("catapult_test_delete_backend");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut config = AppConfig::default();
        config.managed_runtimes = vec![
            make_managed(5000, "cuda"),
            make_managed(5000, "vulkan"),
        ];

        // Set active to Vulkan so we can delete CUDA
        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "vulkan".to_string(),
        };

        // delete_managed_runtime needs runtimes_base_dir to exist; we can't easily
        // test file deletion without mocking dirs, but we can verify config changes
        let result = delete_managed_runtime(5000, "cuda", &mut config);
        // May error on non-existent dir, but config should be updated
        let _ = result;

        // Vulkan should still be there, CUDA should be gone
        assert!(config.managed_runtimes.iter().any(|r| r.backend_id == "vulkan"));
        assert!(!config.managed_runtimes.iter().any(|r| r.backend_id == "cuda"));
        assert_eq!(config.managed_runtimes.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn delete_active_runtime_is_rejected() {
        let mut config = AppConfig::default();
        config.managed_runtimes = vec![
            make_managed(5000, "cuda"),
        ];
        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "cuda".to_string(),
        };

        let result = delete_managed_runtime(5000, "cuda", &mut config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot delete"));
        assert_eq!(config.managed_runtimes.len(), 1);
    }

    #[test]
    fn get_runtime_info_uses_backend_id() {
        let mut config = AppConfig::default();
        config.managed_runtimes = vec![
            make_managed(5000, "cuda"),
            make_managed(5000, "vulkan"),
        ];

        config.active_runtime = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "vulkan".to_string(),
        };

        let info = get_runtime_info(&config).unwrap();
        assert_eq!(info.build, Some(5000));
        assert_eq!(info.backend.as_deref(), Some("VULKAN"));
    }

    #[test]
    fn active_runtime_serializes_with_backend_id() {
        let rt = crate::config::ActiveRuntime::Managed {
            build: 5000,
            backend_id: "cuda".to_string(),
        };
        let json = serde_json::to_string(&rt).unwrap();
        assert!(json.contains("\"backend_id\":\"cuda\""));
        assert!(json.contains("\"build\":5000"));
    }

    #[test]
    fn active_runtime_deserializes_legacy_without_backend_id() {
        // Old config.json won't have backend_id
        let json = r#"{"type":"managed","build":5000}"#;
        let rt: crate::config::ActiveRuntime = serde_json::from_str(json).unwrap();
        match rt {
            crate::config::ActiveRuntime::Managed { build, backend_id } => {
                assert_eq!(build, 5000);
                assert_eq!(backend_id, ""); // default empty string
            }
            _ => panic!("Expected Managed variant"),
        }
    }

    // ── register_downloaded_runtime tests ──

    fn make_downloaded(build: u32, backend_id: &str) -> DownloadedRuntime {
        DownloadedRuntime {
            managed_runtime: make_managed(build, backend_id),
        }
    }

    /// A populated config with every user-facing field set to a non-default
    /// value. Used to verify that register_downloaded_runtime does not clobber
    /// anything outside managed_runtimes / active_runtime.
    fn config_with_user_data() -> AppConfig {
        AppConfig {
            managed_runtimes: vec![make_managed(4000, "vulkan")],
            custom_runtimes: vec![crate::config::CustomRuntime {
                label: "my-build".to_string(),
                binary_path: PathBuf::from("/opt/llama/llama-server"),
            }],
            active_runtime: crate::config::ActiveRuntime::Managed {
                build: 4000,
                backend_id: "vulkan".to_string(),
            },
            auto_delete_old_runtimes: false,
            model_dirs: vec![
                PathBuf::from("/data/models"),
                PathBuf::from("/extra/models"),
            ],
            download_dir: Some(PathBuf::from("/data/models")),
            last_update_check: Some(1700000000),
            latest_known_build: Some(5000),
            auto_check_updates: false,
            favorite_models: vec![
                "model-alpha".to_string(),
                "model-beta".to_string(),
            ],
            selected_model: Some("/data/models/foo.gguf".to_string()),
            wizard_completed: true,
            model_presets: {
                let mut m = std::collections::HashMap::new();
                m.insert("/data/models/foo.gguf".to_string(), "fast".to_string());
                m
            },
            preferred_owners: vec!["bartowski".to_string(), "unsloth".to_string()],
            ..Default::default()
        }
    }

    /// The critical regression test: registering a downloaded runtime must
    /// never erase unrelated config fields (favorites, model dirs, wizard
    /// status, presets, etc.).
    #[test]
    fn register_runtime_preserves_all_unrelated_config_fields() {
        let mut config = config_with_user_data();
        let downloaded = make_downloaded(5000, "cuda");

        register_downloaded_runtime(&mut config, downloaded).unwrap();

        // ── Runtime fields SHOULD have changed ──
        assert_eq!(config.active_build(), Some(5000));
        assert_eq!(config.active_backend_id(), Some("cuda"));
        assert!(config.managed_runtimes.iter().any(|r| r.build == 5000 && r.backend_id == "cuda"));

        // ── Everything below MUST be preserved ──
        assert!(config.wizard_completed,
            "wizard_completed was erased!");
        assert_eq!(config.favorite_models, vec!["model-alpha", "model-beta"],
            "favorite_models were erased!");
        assert_eq!(config.selected_model.as_deref(), Some("/data/models/foo.gguf"),
            "selected_model was erased!");
        assert_eq!(config.model_dirs, vec![
            PathBuf::from("/data/models"),
            PathBuf::from("/extra/models"),
        ], "model_dirs were erased!");
        assert_eq!(config.download_dir, Some(PathBuf::from("/data/models")),
            "download_dir was erased!");
        assert_eq!(config.last_update_check, Some(1700000000),
            "last_update_check was erased!");
        assert_eq!(config.latest_known_build, Some(5000),
            "latest_known_build was erased!");
        assert!(!config.auto_check_updates,
            "auto_check_updates was erased (reset to default true)!");
        assert_eq!(config.model_presets.get("/data/models/foo.gguf").map(|s| s.as_str()),
            Some("fast"), "model_presets were erased!");
        assert_eq!(config.preferred_owners, vec!["bartowski", "unsloth"],
            "preferred_owners were erased!");
        assert_eq!(config.custom_runtimes.len(), 1,
            "custom_runtimes were erased!");
        assert_eq!(config.custom_runtimes[0].label, "my-build",
            "custom_runtimes content was erased!");
    }

    /// The old b4000-vulkan runtime should survive when we register a new
    /// b5000-cuda (different backend).
    #[test]
    fn register_runtime_keeps_other_backends() {
        let mut config = config_with_user_data();
        assert_eq!(config.managed_runtimes.len(), 1); // b4000-vulkan

        let downloaded = make_downloaded(5000, "cuda");
        register_downloaded_runtime(&mut config, downloaded).unwrap();

        assert_eq!(config.managed_runtimes.len(), 2);
        assert!(config.managed_runtimes.iter().any(|r| r.build == 4000 && r.backend_id == "vulkan"));
        assert!(config.managed_runtimes.iter().any(|r| r.build == 5000 && r.backend_id == "cuda"));
    }

    /// Registering the same build+backend twice replaces the old entry (no
    /// duplicates).
    #[test]
    fn register_runtime_replaces_same_build_and_backend() {
        let mut config = AppConfig::default();
        config.managed_runtimes.push(make_managed(5000, "cuda"));

        let downloaded = make_downloaded(5000, "cuda");
        register_downloaded_runtime(&mut config, downloaded).unwrap();

        let cuda_count = config.managed_runtimes.iter()
            .filter(|r| r.build == 5000 && r.backend_id == "cuda")
            .count();
        assert_eq!(cuda_count, 1, "should have exactly one entry, not a duplicate");
    }

    #[test]
    fn register_runtime_sets_active() {
        let mut config = config_with_user_data();
        // Starts with b4000-vulkan active
        assert_eq!(config.active_build(), Some(4000));

        let downloaded = make_downloaded(5000, "cuda");
        register_downloaded_runtime(&mut config, downloaded).unwrap();

        assert_eq!(config.active_build(), Some(5000));
        assert_eq!(config.active_backend_id(), Some("cuda"));
    }

    #[test]
    fn register_runtime_sorts_by_build_descending() {
        let mut config = AppConfig::default();
        config.managed_runtimes.push(make_managed(3000, "cpu"));
        config.managed_runtimes.push(make_managed(5000, "vulkan"));

        let downloaded = make_downloaded(4000, "cuda");
        register_downloaded_runtime(&mut config, downloaded).unwrap();

        let builds: Vec<u32> = config.managed_runtimes.iter().map(|r| r.build).collect();
        assert_eq!(builds, vec![5000, 4000, 3000]);
    }

    #[test]
    fn register_runtime_auto_deletes_same_backend_only() {
        let mut config = AppConfig::default();
        config.auto_delete_old_runtimes = true;
        config.managed_runtimes.push(make_managed(3000, "cuda"));
        config.managed_runtimes.push(make_managed(3000, "vulkan"));

        let downloaded = make_downloaded(5000, "cuda");
        register_downloaded_runtime(&mut config, downloaded).unwrap();

        // Old cuda should be removed, vulkan should remain
        assert!(!config.managed_runtimes.iter().any(|r| r.build == 3000 && r.backend_id == "cuda"),
            "old cuda b3000 should have been auto-deleted");
        assert!(config.managed_runtimes.iter().any(|r| r.build == 3000 && r.backend_id == "vulkan"),
            "vulkan b3000 should NOT be auto-deleted");
        assert!(config.managed_runtimes.iter().any(|r| r.build == 5000 && r.backend_id == "cuda"),
            "new cuda b5000 should be present");
    }
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use crate::config::AppConfig;
use crate::huggingface::{self, HfFile, RECOMMENDED_MODELS};
use crate::runtime::DownloadProgress;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub repo_id: String,
    pub filename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub quant: Option<String>,
    pub params_b: Option<String>,
    pub context_length: Option<u64>,
    pub is_vision: bool,
    pub mmproj_path: Option<PathBuf>,
    /// Paths to all parts for split GGUF models (empty for single-file models).
    #[serde(default)]
    pub split_files: Vec<PathBuf>,
}

/// Metadata extracted from a GGUF file header.
#[derive(Debug, Default)]
struct GgufMeta {
    name: Option<String>,
    architecture: Option<String>,
    size_label: Option<String>,
    context_length: Option<u64>,
    tags: Vec<String>,
}

fn read_gguf_metadata(path: &Path) -> Option<GgufMeta> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf4 = [0u8; 4];
    let mut buf8 = [0u8; 8];

    // Magic: "GGUF"
    f.read_exact(&mut buf4).ok()?;
    if &buf4 != b"GGUF" {
        return None;
    }

    // Version (u32)
    f.read_exact(&mut buf4).ok()?;
    let _version = u32::from_le_bytes(buf4);

    // Tensor count (u64)
    f.read_exact(&mut buf8).ok()?;

    // Metadata KV count (u64)
    f.read_exact(&mut buf8).ok()?;
    let kv_count = u64::from_le_bytes(buf8);

    let mut meta = GgufMeta::default();

    for _ in 0..kv_count.min(128) {
        let key = match read_gguf_string(&mut f) {
            Some(s) => s,
            None => break,
        };

        f.read_exact(&mut buf4).ok()?;
        let vtype = u32::from_le_bytes(buf4);

        match vtype {
            8 => {
                // String
                let val = match read_gguf_string(&mut f) {
                    Some(s) => s,
                    None => break,
                };
                if key == "general.name" {
                    meta.name = Some(val);
                } else if key == "general.architecture" {
                    meta.architecture = Some(val);
                } else if key == "general.size_label" {
                    meta.size_label = Some(val);
                }
            }
            4 | 5 => {
                // u32 / i32
                f.read_exact(&mut buf4).ok()?;
                let val = u32::from_le_bytes(buf4);
                if key.ends_with(".context_length") {
                    meta.context_length = Some(val as u64);
                }
            }
            10 | 11 => {
                // u64 / i64
                f.read_exact(&mut buf8).ok()?;
                let val = u64::from_le_bytes(buf8);
                if key.ends_with(".context_length") {
                    meta.context_length = Some(val);
                }
            }
            0 | 1 => { let mut b = [0u8; 1]; f.read_exact(&mut b).ok()?; }
            2 | 3 => { let mut b = [0u8; 2]; f.read_exact(&mut b).ok()?; }
            6 => { f.read_exact(&mut buf4).ok()?; }
            7 => { let mut b = [0u8; 1]; f.read_exact(&mut b).ok()?; }
            12 => { f.read_exact(&mut buf8).ok()?; }
            9 => {
                // Array
                f.read_exact(&mut buf4).ok()?;
                let atype = u32::from_le_bytes(buf4);
                f.read_exact(&mut buf8).ok()?;
                let alen = u64::from_le_bytes(buf8);
                match atype {
                    0 | 1 | 7 => { let mut b = vec![0u8; alen as usize]; f.read_exact(&mut b).ok()?; }
                    2 | 3 => { let mut b = vec![0u8; alen as usize * 2]; f.read_exact(&mut b).ok()?; }
                    4 | 5 | 6 => { let mut b = vec![0u8; alen as usize * 4]; f.read_exact(&mut b).ok()?; }
                    10 | 11 | 12 => { let mut b = vec![0u8; alen as usize * 8]; f.read_exact(&mut b).ok()?; }
                    8 => {
                        let mut strings = Vec::new();
                        for _ in 0..alen {
                            if let Some(s) = read_gguf_string(&mut f) {
                                strings.push(s);
                            }
                        }
                        if key == "general.tags" {
                            meta.tags = strings;
                        }
                    }
                    _ => break,
                }
            }
            _ => break,
        }
    }

    Some(meta)
}

fn read_gguf_string(f: &mut std::fs::File) -> Option<String> {
    let mut buf8 = [0u8; 8];
    f.read_exact(&mut buf8).ok()?;
    let len = u64::from_le_bytes(buf8) as usize;
    if len > 1_000_000 {
        return None; // sanity check
    }
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedModel {
    pub repo_id: String,
    pub filename: String,
    pub name: String,
    pub description: String,
    pub params_b: u32,
    pub family: String,
    pub quant: String,
    pub context: Option<u32>,
    pub estimated_size_mb: u64,
    pub installed: bool,
    pub installed_path: Option<PathBuf>,
}

// ── GGUF metadata cache ──────────────────────────────────────────────────────

/// Cached GGUF metadata keyed by file path. Invalidated when size or mtime changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GgufCacheEntry {
    size_bytes: u64,
    mtime_secs: i64,
    // Cached metadata fields
    meta_name: Option<String>,
    meta_size_label: Option<String>,
    meta_context_length: Option<u64>,
    #[serde(default)]
    is_vision: bool,
}

type GgufCache = HashMap<String, GgufCacheEntry>;

fn cache_path() -> Option<PathBuf> {
    let data_dir = dirs::data_dir()?;
    Some(data_dir.join("catapult").join("gguf_cache.json"))
}

fn load_cache() -> GgufCache {
    let path = match cache_path() {
        Some(p) => p,
        None => return HashMap::new(),
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_cache(cache: &GgufCache) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string(cache) {
            let _ = std::fs::write(path, content);
        }
    }
}

fn file_mtime_secs(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Model scanning ───────────────────────────────────────────────────────────

pub fn list_installed_models(config: &AppConfig) -> Result<Vec<ModelInfo>> {
    let mut models = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    let mut cache = load_cache();
    let mut cache_dirty = false;

    for dir in config.all_model_dirs() {
        if dir.exists() {
            scan_gguf_recursive(&dir, &mut models, &mut seen_paths, &mut cache, &mut cache_dirty, 5);
        }
    }

    if cache_dirty {
        save_cache(&cache);
    }

    // Consolidate split GGUF files into single entries
    models = consolidate_split_models(models);

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

/// Group split GGUF parts (e.g. model-00001-of-00003.gguf) into single ModelInfo entries.
/// Only consolidates when ALL expected parts are present.
fn consolidate_split_models(models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    use std::collections::BTreeMap;

    let mut singles = Vec::new();
    let mut split_groups: BTreeMap<String, Vec<ModelInfo>> = BTreeMap::new();

    for model in models {
        if let Some((base, _part, total)) = huggingface::parse_split_filename(&model.filename) {
            let dir = model.path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            let key = format!("{}|{}-{:05}", dir, base, total);
            split_groups.entry(key).or_default().push(model);
        } else {
            singles.push(model);
        }
    }

    for (_key, mut parts) in split_groups {
        let expected_total = huggingface::parse_split_filename(&parts[0].filename)
            .map(|(_, _, t)| t)
            .unwrap_or(0) as usize;

        if parts.len() != expected_total {
            // Not all parts present — show individually
            singles.extend(parts);
            continue;
        }

        parts.sort_by_key(|m| {
            huggingface::parse_split_filename(&m.filename).map(|(_, n, _)| n).unwrap_or(0)
        });

        let total_size: u64 = parts.iter().map(|p| p.size_bytes).sum();
        let split_files: Vec<PathBuf> = parts.iter().map(|p| p.path.clone()).collect();
        let first = parts.into_iter().next().unwrap();

        singles.push(ModelInfo {
            size_bytes: total_size,
            split_files,
            ..first
        });
    }

    singles
}

fn scan_gguf_recursive(
    dir: &std::path::Path,
    models: &mut Vec<ModelInfo>,
    seen: &mut std::collections::HashSet<PathBuf>,
    cache: &mut GgufCache,
    cache_dirty: &mut bool,
    max_depth: u32,
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
        if path.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("gguf")
            && !path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with("__downloading__"))
                .unwrap_or(false)
        {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }

            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let file_meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size_bytes = file_meta.len();
            let mtime_secs = file_mtime_secs(&file_meta);
            let quant = huggingface::extract_quant(&filename);
            let id = sanitize_id(&filename);
            let (repo_id, fallback_name) = guess_repo_from_filename(&filename);

            let cache_key = path.to_string_lossy().to_string();

            // Check cache: hit if size and mtime match
            let cached_meta = if let Some(cached) = cache.get(&cache_key) {
                if cached.size_bytes == size_bytes && cached.mtime_secs == mtime_secs {
                    CachedMeta {
                        name: cached.meta_name.clone(),
                        size_label: cached.meta_size_label.clone(),
                        context_length: cached.meta_context_length,
                        is_vision: cached.is_vision,
                    }
                } else {
                    *cache_dirty = true;
                    read_and_cache(&path, size_bytes, mtime_secs, &cache_key, cache)
                }
            } else {
                *cache_dirty = true;
                read_and_cache(&path, size_bytes, mtime_secs, &cache_key, cache)
            };

            let name = cached_meta.name
                .map(|n| n.rsplit('/').next().unwrap_or(&n).to_string())
                .unwrap_or(fallback_name);
            let params_b = cached_meta.size_label
                .or_else(|| extract_params_from_filename(&filename).map(|p| format!("{}B", p)));

            // Find compatible mmproj for vision models
            let mmproj_path = if cached_meta.is_vision {
                find_mmproj(&path, &filename)
            } else {
                None
            };

            models.push(ModelInfo {
                id,
                name,
                repo_id,
                filename: filename.clone(),
                path,
                size_bytes,
                quant,
                params_b,
                context_length: cached_meta.context_length,
                is_vision: cached_meta.is_vision,
                mmproj_path,
                split_files: vec![],
            });
        } else if path.is_dir() {
            scan_gguf_recursive(&path, models, seen, cache, cache_dirty, max_depth - 1);
        }
    }
}

/// Find a compatible mmproj file in the same directory as the model.
/// The mmproj must contain "mmproj" and share a reasonable common substring
/// (model name + param count) with the main model.
fn find_mmproj(model_path: &Path, model_filename: &str) -> Option<PathBuf> {
    let dir = model_path.parent()?;
    let stem = model_filename.trim_end_matches(".gguf");

    // Extract the "model name + params" prefix, e.g. "Qwen3.5-4B" from "Qwen3.5-4B-Q4_K_M"
    // Strip trailing quant pattern to get the base name
    let re = regex::Regex::new(r"[-_](?:MXFP\d|IQ\d[_A-Z]*|Q\d[_KM0-9A-Z]+|F16|F32|BF16)$").unwrap();
    let base = re.replace(stem, "").to_string();

    // Split base into segments for matching
    // e.g. "Qwen3.5-4B" → ["qwen3.5", "4b"]
    let base_lower = base.to_lowercase();
    let segments: Vec<&str> = base_lower.split(&['-', '_', '.'][..])
        .filter(|s| !s.is_empty())
        .collect();

    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(PathBuf, usize)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }
        let fname = path.file_name()?.to_string_lossy().to_string();
        let fname_lower = fname.to_lowercase();

        // Must contain "mmproj" and be a .gguf file
        if !fname_lower.contains("mmproj") { continue; }
        if !fname_lower.ends_with(".gguf") { continue; }
        // Must not be the model itself
        if fname == model_filename { continue; }

        // Count how many base segments appear in the mmproj filename
        let matches = segments.iter()
            .filter(|seg| fname_lower.contains(*seg))
            .count();

        // Require at least 2 matching segments (name + params typically)
        if matches >= 2 {
            if best.as_ref().map_or(true, |(_, best_m)| matches > *best_m) {
                best = Some((path, matches));
            }
        }
    }

    best.map(|(p, _)| p)
}

struct CachedMeta {
    name: Option<String>,
    size_label: Option<String>,
    context_length: Option<u64>,
    is_vision: bool,
}

fn is_vision_model(tags: &[String]) -> bool {
    tags.iter().any(|t| {
        let lower = t.to_lowercase();
        lower == "image-to-text" || lower == "image-text-to-text"
    })
}

fn read_and_cache(
    path: &Path,
    size_bytes: u64,
    mtime_secs: i64,
    cache_key: &str,
    cache: &mut GgufCache,
) -> CachedMeta {
    let meta = read_gguf_metadata(path).unwrap_or_default();
    let is_vision = is_vision_model(&meta.tags);
    cache.insert(
        cache_key.to_string(),
        GgufCacheEntry {
            size_bytes,
            mtime_secs,
            meta_name: meta.name.clone(),
            meta_size_label: meta.size_label.clone(),
            meta_context_length: meta.context_length,
            is_vision,
        },
    );
    CachedMeta {
        name: meta.name,
        size_label: meta.size_label,
        context_length: meta.context_length,
        is_vision,
    }
}

pub fn get_recommended_models(config: &AppConfig) -> Result<Vec<RecommendedModel>> {
    let installed = list_installed_models(config)?;
    let _models_dir = config.models_dir()?;

    let models = RECOMMENDED_MODELS
        .iter()
        .map(|def| {
            let estimated_size_mb = estimate_size_mb(def.params_b, def.quant);
            let installed_model = installed.iter().find(|m| m.filename == def.filename);

            RecommendedModel {
                repo_id: def.repo_id.to_string(),
                filename: def.filename.to_string(),
                name: def.name.to_string(),
                description: def.description.to_string(),
                params_b: def.params_b,
                family: def.family.to_string(),
                quant: def.quant.to_string(),
                context: def.context,
                estimated_size_mb,
                installed: installed_model.is_some(),
                installed_path: installed_model.map(|m| m.path.clone()),
            }
        })
        .collect();

    Ok(models)
}

pub async fn download_model(
    client: &reqwest::Client,
    _repo_id: &str,
    file: &HfFile,
    config: &AppConfig,
    progress_cb: impl Fn(DownloadProgress) + Send + Sync,
) -> Result<PathBuf> {
    let models_dir = config.models_dir()?;
    std::fs::create_dir_all(&models_dir)?;

    if !file.split_parts.is_empty() {
        return download_split_model(client, file, &models_dir, &progress_cb).await;
    }

    download_single_file(client, &file.filename, &file.download_url, file.size_bytes, &models_dir, &progress_cb).await
}

/// Download all parts of a split GGUF model, reporting combined progress.
async fn download_split_model(
    client: &reqwest::Client,
    file: &HfFile,
    models_dir: &Path,
    progress_cb: &(dyn Fn(DownloadProgress) + Send + Sync),
) -> Result<PathBuf> {
    let total_bytes = file.size_bytes;
    let display_id = file.filename.clone();
    let mut completed_bytes: u64 = 0;

    let first_dest = models_dir.join(&file.split_parts[0].filename);

    for part in &file.split_parts {
        let dest_path = models_dir.join(&part.filename);

        // Skip already completed parts
        if dest_path.exists() {
            if let Ok(meta) = std::fs::metadata(&dest_path) {
                if part.size_bytes > 0 && meta.len() == part.size_bytes {
                    completed_bytes += part.size_bytes;
                    continue;
                }
            }
        }

        let base = completed_bytes;
        let id = display_id.clone();
        let total = total_bytes;

        let part_cb = move |p: DownloadProgress| {
            if p.status == "done" {
                return; // Don't forward individual part "done"
            }
            progress_cb(DownloadProgress {
                id: id.clone(),
                bytes_downloaded: base + p.bytes_downloaded,
                total_bytes: total,
                percent: if total > 0 { ((base + p.bytes_downloaded) as f64 / total as f64) * 100.0 } else { 0.0 },
                status: p.status,
            });
        };

        download_single_file(client, &part.filename, &part.download_url, part.size_bytes, models_dir, &part_cb).await?;

        completed_bytes += part.size_bytes;
    }

    // All parts done
    progress_cb(DownloadProgress {
        id: display_id,
        bytes_downloaded: total_bytes,
        total_bytes,
        percent: 100.0,
        status: "done".to_string(),
    });

    Ok(first_dest)
}

/// Download a single file with resume support and exponential backoff.
async fn download_single_file(
    client: &reqwest::Client,
    filename: &str,
    download_url: &str,
    size_bytes: u64,
    models_dir: &Path,
    progress_cb: &(dyn Fn(DownloadProgress) + Send + Sync),
) -> Result<PathBuf> {
    let dest_path = models_dir.join(filename);
    let tmp_path = models_dir.join(format!("__downloading__{}", filename));

    // Check if already downloaded
    if dest_path.exists() {
        let existing_size = std::fs::metadata(&dest_path)?.len();
        if size_bytes > 0 && existing_size == size_bytes {
            return Ok(dest_path);
        }
    }

    let total_bytes = size_bytes;
    let max_retries = 5u32;
    let backoff_secs: [u64; 6] = [0, 1, 2, 4, 8, 8];
    let mut consecutive_failures = 0u32;

    loop {
        // Check how much we already have from a previous partial download
        let resume_from = if tmp_path.exists() {
            std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        if consecutive_failures > 0 {
            let delay = backoff_secs[consecutive_failures.min(5) as usize];
            progress_cb(DownloadProgress {
                id: filename.to_string(),
                bytes_downloaded: resume_from,
                total_bytes,
                percent: if total_bytes > 0 { (resume_from as f64 / total_bytes as f64) * 100.0 } else { 0.0 },
                status: format!("retrying ({}/{})", consecutive_failures, max_retries),
            });
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
        }

        // Build request with Range header for resume
        let mut req = client
            .get(download_url)
            .header("User-Agent", "catapult-launcher/0.1");

        if resume_from > 0 {
            req = req.header("Range", format!("bytes={}-", resume_from));
            log::info!("Resuming download of {} from byte {}", filename, resume_from);
        }

        let response = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                consecutive_failures += 1;
                log::warn!("Download attempt failed for {} (failure {}/{}): {}", filename, consecutive_failures, max_retries, e);
                if consecutive_failures >= max_retries {
                    progress_cb(DownloadProgress {
                        id: filename.to_string(),
                        bytes_downloaded: resume_from,
                        total_bytes,
                        percent: if total_bytes > 0 { (resume_from as f64 / total_bytes as f64) * 100.0 } else { 0.0 },
                        status: "paused".to_string(),
                    });
                    anyhow::bail!("Download failed after {} consecutive retries: {}", max_retries, e);
                }
                continue;
            }
        };

        let status_code = response.status();
        if !status_code.is_success() && status_code.as_u16() != 206 {
            consecutive_failures += 1;
            log::warn!("HTTP {} for {} (failure {}/{})", status_code, filename, consecutive_failures, max_retries);
            if consecutive_failures >= max_retries {
                progress_cb(DownloadProgress {
                    id: filename.to_string(),
                    bytes_downloaded: resume_from,
                    total_bytes,
                    percent: if total_bytes > 0 { (resume_from as f64 / total_bytes as f64) * 100.0 } else { 0.0 },
                    status: "paused".to_string(),
                });
                anyhow::bail!("Download failed: HTTP {}", status_code);
            }
            continue;
        }

        // If server returned 200 (not 206), it doesn't support Range — start from scratch
        let (mut downloaded, mut out_file) = if status_code.as_u16() == 206 && resume_from > 0 {
            let f = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&tmp_path)
                .await
                .context("Failed to open temp file for resume")?;
            (resume_from, f)
        } else {
            let f = tokio::fs::File::create(&tmp_path)
                .await
                .context("Failed to create temp file")?;
            (0u64, f)
        };

        use futures::StreamExt;
        let mut stream = response.bytes_stream();
        let mut stream_error = false;
        let downloaded_at_start = downloaded;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Err(e) = out_file.write_all(&chunk).await {
                        log::warn!("Write error during download of {}: {}", filename, e);
                        stream_error = true;
                        break;
                    }
                    downloaded += chunk.len() as u64;

                    let percent = if total_bytes > 0 {
                        (downloaded as f64 / total_bytes as f64) * 100.0
                    } else {
                        0.0
                    };

                    progress_cb(DownloadProgress {
                        id: filename.to_string(),
                        bytes_downloaded: downloaded,
                        total_bytes,
                        percent,
                        status: "downloading".to_string(),
                    });
                }
                Err(e) => {
                    log::warn!("Stream error during download of {}: {}", filename, e);
                    stream_error = true;
                    break;
                }
            }
        }

        let _ = out_file.flush().await;
        drop(out_file);

        if stream_error {
            // If we received new data before the error, reset the failure counter
            if downloaded > downloaded_at_start {
                log::info!("Download of {} made progress ({} -> {} bytes), resetting retry counter",
                    filename, downloaded_at_start, downloaded);
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
            }

            if consecutive_failures >= max_retries {
                progress_cb(DownloadProgress {
                    id: filename.to_string(),
                    bytes_downloaded: downloaded,
                    total_bytes,
                    percent: if total_bytes > 0 { (downloaded as f64 / total_bytes as f64) * 100.0 } else { 0.0 },
                    status: "paused".to_string(),
                });
                anyhow::bail!("Download of {} failed after {} consecutive retries (stream error)", filename, max_retries);
            }
            continue;
        }

        // Success — move to final destination
        tokio::fs::rename(&tmp_path, &dest_path).await?;

        progress_cb(DownloadProgress {
            id: filename.to_string(),
            bytes_downloaded: downloaded,
            total_bytes,
            percent: 100.0,
            status: "done".to_string(),
        });

        return Ok(dest_path);
    }
}

pub fn abort_download(filename: &str, config: &AppConfig) -> Result<()> {
    let models_dir = config.models_dir()?;
    let tmp_path = models_dir.join(format!("__downloading__{}", filename));
    if tmp_path.exists() {
        std::fs::remove_file(&tmp_path)?;
    }

    // For split models: also clean up temp files for other parts
    if let Some((base, _, total)) = huggingface::parse_split_filename(filename) {
        for i in 1..=total {
            let part_name = format!("{}-{:05}-of-{:05}.gguf", base, i, total);
            let tmp = models_dir.join(format!("__downloading__{}", part_name));
            if tmp.exists() {
                std::fs::remove_file(&tmp)?;
            }
        }
    }

    Ok(())
}

pub fn delete_model(path: &Path) -> Result<()> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if let Some((base, _, total)) = huggingface::parse_split_filename(filename) {
        // Delete all parts of the split model
        if let Some(parent) = path.parent() {
            for i in 1..=total {
                let part_name = format!("{}-{:05}-of-{:05}.gguf", base, i, total);
                let part_path = parent.join(part_name);
                if part_path.exists() {
                    std::fs::remove_file(&part_path)?;
                }
            }
        }
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

fn sanitize_id(filename: &str) -> String {
    filename
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn guess_repo_from_filename(filename: &str) -> (String, String) {
    // Try to extract a clean name by stripping the .gguf extension
    // and quant suffix like -Q4_K_M
    let stem = filename.trim_end_matches(".gguf");

    // Strip trailing quant patterns
    let re = regex::Regex::new(r"-(?:MXFP\d|IQ\d[_A-Z]*|Q\d[_KM0-9A-Z]+|F16|F32|BF16)$").unwrap();
    let name = re.replace(stem, "").to_string();

    (String::new(), name)
}

fn extract_params_from_filename(filename: &str) -> Option<u32> {
    // Match patterns like 7B, 8B, 70B, 1.5B, etc.
    let re = regex::Regex::new(r"(?i)[_\-.](\d+(?:\.\d+)?)b[_\-.]").ok()?;
    let caps = re.captures(filename)?;
    let val: f64 = caps.get(1)?.as_str().parse().ok()?;
    Some(val as u32)
}

pub fn estimate_size_mb(params_b: u32, quant: &str) -> u64 {
    // Bits per weight for each quant type
    let bits_per_weight = match quant.to_uppercase().as_str() {
        q if q.starts_with("Q2") => 2.5_f64,
        q if q.starts_with("Q3") => 3.5,
        q if q.starts_with("Q4") || q.starts_with("MXFP4") => 4.5,
        q if q.starts_with("Q5") => 5.5,
        q if q.starts_with("Q6") => 6.6,
        q if q.starts_with("Q8") => 8.5,
        "F16" | "BF16" => 16.0,
        "F32" => 32.0,
        q if q.starts_with("IQ2") => 2.3,
        q if q.starts_with("IQ3") => 3.3,
        q if q.starts_with("IQ4") => 4.3,
        _ => 4.5,
    };

    // params_b * 1e9 * bits_per_weight / 8 bytes, converted to MB
    let bytes = (params_b as f64) * 1_000_000_000.0 * bits_per_weight / 8.0;
    (bytes / (1024.0 * 1024.0)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_size_q4_7b() {
        let mb = estimate_size_mb(7, "Q4_K_M");
        // 7B * 4.5 bits/weight * 1e9 / 8 / 1024^2 ≈ 3755 MB
        assert!(mb > 3500 && mb < 4000, "7B Q4_K_M should be ~3750 MB, got {}", mb);
    }

    #[test]
    fn estimate_size_f16_7b() {
        let mb = estimate_size_mb(7, "F16");
        // 7B * 16 bits * 1e9 / 8 / 1024^2 ≈ 13351 MB
        assert!(mb > 13000 && mb < 14000, "7B F16 should be ~13351 MB, got {}", mb);
    }

    #[test]
    fn estimate_size_q4_70b() {
        let mb = estimate_size_mb(70, "Q4_K_M");
        assert!(mb > 35000 && mb < 40000, "70B Q4_K_M ~37500 MB, got {}", mb);
    }

    #[test]
    fn estimate_size_iq2() {
        let mb = estimate_size_mb(7, "IQ2_XXS");
        // 7B * 2.3 bits * 1e9 / 8 / 1024^2 ≈ 1920 MB
        assert!(mb > 1800 && mb < 2100, "7B IQ2_XXS ~1920 MB, got {}", mb);
    }

    #[test]
    fn guess_repo_strips_quant_and_extension() {
        let (_, name) = guess_repo_from_filename("Qwen2.5-7B-Instruct-Q4_K_M.gguf");
        assert_eq!(name, "Qwen2.5-7B-Instruct");

        let (_, name) = guess_repo_from_filename("Meta-Llama-3.1-8B-Q8_0.gguf");
        assert_eq!(name, "Meta-Llama-3.1-8B");

        let (_, name) = guess_repo_from_filename("model-F16.gguf");
        assert_eq!(name, "model");

        let (_, name) = guess_repo_from_filename("model-IQ4_XS.gguf");
        assert_eq!(name, "model");
    }

    #[test]
    fn extract_params_from_filename_patterns() {
        assert_eq!(extract_params_from_filename("Llama-3.1-8B-Q4_K_M.gguf"), Some(8));
        assert_eq!(extract_params_from_filename("model-70B-Q5_K.gguf"), Some(70));
        assert_eq!(extract_params_from_filename("qwen2.5-1.5b-instruct-Q4_K_M.gguf"), Some(1));
        assert_eq!(extract_params_from_filename("model.gguf"), None);
    }

    #[test]
    fn sanitize_id_replaces_special_chars() {
        assert_eq!(sanitize_id("model (v2.0).gguf"), "model__v2_0__gguf");
        assert_eq!(sanitize_id("simple-name_v1"), "simple-name_v1");
        assert_eq!(sanitize_id("a/b\\c:d"), "a_b_c_d");
    }

    #[test]
    fn gguf_parser_handles_missing_file() {
        let result = read_gguf_metadata(std::path::Path::new("/nonexistent/file.gguf"));
        assert!(result.is_none());
    }

    #[test]
    fn gguf_parser_handles_non_gguf_file() {
        // Create a temp file with non-GGUF content
        let dir = std::env::temp_dir().join("catapult_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not_gguf.gguf");
        std::fs::write(&path, b"This is not a GGUF file").unwrap();

        let result = read_gguf_metadata(&path);
        assert!(result.is_none());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn gguf_parser_reads_real_file() {
        // Test with an actual GGUF file if available
        let test_paths = [
            "/mnt/win/k/models/GLM-4.6V-Flash-Q4_K_M.gguf",
            "/mnt/win/h/models/Falcon-H1R-7B-Q8_0.gguf",
        ];
        for path_str in &test_paths {
            let path = std::path::Path::new(path_str);
            if !path.exists() {
                continue;
            }
            let meta = read_gguf_metadata(path).expect("should parse valid GGUF");
            assert!(meta.architecture.is_some(), "should have architecture");
            assert!(meta.size_label.is_some() || meta.name.is_some(),
                "should have size_label or name for {}", path_str);
            if meta.architecture.is_some() {
                assert!(meta.context_length.is_some(),
                    "should have context_length for {}", path_str);
            }
        }
    }
}

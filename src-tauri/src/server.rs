use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::hardware::{suggest_config, SystemInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub model_path: String,
    #[serde(default)]
    pub mmproj_path: Option<String>,
    pub host: String,
    pub port: u16,
    // Context and memory
    pub n_ctx: u32,
    pub n_gpu_layers: i32,
    pub n_threads: Option<i32>,
    // Attention
    pub flash_attn: String,
    pub cache_type_k: String,
    pub cache_type_v: String,
    // Sampling
    pub temperature: f32,
    pub top_k: i32,
    pub min_p: f32,
    pub top_p: f32,
    pub n_predict: i32,
    // Batching
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub cont_batching: bool,
    // Memory
    pub mlock: bool,
    pub no_mmap: bool,
    // Misc
    pub seed: Option<u64>,
    pub rope_freq_scale: Option<f32>,
    pub rope_freq_base: Option<f32>,
    pub grp_attn_n: Option<u32>,
    pub grp_attn_w: Option<u32>,
    // Slots
    pub parallel: u32,
    // Additional CLI parameters: key = flag name (without --), value = argument (empty for boolean flags)
    #[serde(default)]
    pub extra_params: HashMap<String, String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            mmproj_path: None,
            host: "127.0.0.1".to_string(),
            port: 8080,
            n_ctx: 0,
            n_gpu_layers: -1,
            n_threads: None,
            flash_attn: "auto".to_string(),
            cache_type_k: "f16".to_string(),
            cache_type_v: "f16".to_string(),
            temperature: 0.8,
            top_k: 40,
            min_p: 0.05,
            top_p: 0.95,
            n_predict: -1,
            n_batch: 512,
            n_ubatch: 512,
            cont_batching: true,
            mlock: false,
            no_mmap: false,
            seed: None,
            rope_freq_scale: None,
            rope_freq_base: None,
            grp_attn_n: None,
            grp_attn_w: None,
            parallel: 1,
            extra_params: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running { port: u16, pid: u32 },
    Error { message: String },
}

pub struct ServerState {
    pub process: Option<Child>,
    pub status: ServerStatus,
    pub log_lines: Vec<String>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            process: None,
            status: ServerStatus::Stopped,
            log_lines: Vec::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, ServerStatus::Running { .. } | ServerStatus::Starting)
    }
}

pub type SharedServerState = Arc<Mutex<ServerState>>;

pub fn new_server_state() -> SharedServerState {
    Arc::new(Mutex::new(ServerState::new()))
}

/// Apply `HfPresetParams` fields to an existing `ServerConfig`.
/// Only overwrites fields that are present in the preset.
pub fn apply_hf_preset_params(params: &crate::huggingface::HfPresetParams, config: &mut ServerConfig) {
    if let Some(v) = params.temperature { config.temperature = v; }
    if let Some(v) = params.top_k { config.top_k = v; }
    if let Some(v) = params.top_p { config.top_p = v; }
    if let Some(v) = params.min_p { config.min_p = v; }
    if let Some(v) = params.n_predict { config.n_predict = v; }
    if let Some(v) = params.seed { config.seed = Some(v); }
    if let Some(v) = params.repeat_penalty {
        config.extra_params.insert("repeat-penalty".to_string(), format!("{:.4}", v));
    }
    if let Some(v) = params.repeat_last_n {
        config.extra_params.insert("repeat-last-n".to_string(), v.to_string());
    }
}

/// Derive a safe preset name from a HuggingFace repo_id (e.g. "unsloth/Foo" → "unsloth__Foo").
pub fn preset_name_from_repo(repo_id: &str) -> String {
    repo_id.replace('/', "__")
}

pub async fn start_server(
    server_binary: &PathBuf,
    config: &ServerConfig,
    state: SharedServerState,
    log_cb: impl Fn(String) + Send + Sync + 'static,
) -> Result<()> {
    // Check not already running
    {
        let s = state.lock().unwrap();
        if s.is_running() {
            anyhow::bail!("Server is already running");
        }
    }

    let args = build_args(config);

    let cmdline = format!("{} {}", server_binary.display(), args.join(" "));
    log::info!("Starting llama-server: {}", cmdline);

    let mut child = Command::new(server_binary)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to spawn llama-server")?;

    let pid = child.id().unwrap_or(0);
    let port = config.port;

    // Take stdout/stderr before storing child in state
    let stdout = child.stdout.take().expect("stdout not piped");
    let stderr = child.stderr.take().expect("stderr not piped");

    {
        let mut s = state.lock().unwrap();
        s.process = Some(child);
        s.status = ServerStatus::Starting;
        s.log_lines.clear();
        // Add commandline as first log entry (after clear)
        s.log_lines.push(format!("$ {}", cmdline));
    }

    // Emit commandline as first log event
    log_cb(format!("$ {}", cmdline));

    // Read stdout/stderr in background tasks
    let state_clone = state.clone();
    let log_cb = Arc::new(log_cb);
    let log_cb_clone = log_cb.clone();
    let log_cb_exit = log_cb.clone();

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    let mut s = state_clone.lock().unwrap();
                    s.log_lines.push(line.clone());
                    if s.log_lines.len() > 500 {
                        s.log_lines.drain(0..100);
                    }
                    // Detect server ready
                    if matches!(s.status, ServerStatus::Starting)
                        && (line.contains("HTTP server listening")
                            || line.contains("server is listening"))
                    {
                        s.status = ServerStatus::Running { port, pid };
                        log::info!("Server ready on port {}", port);
                    }
                    drop(s);
                    log_cb(line);
                }
                Err(e) => {
                    log::warn!("Error reading server stdout: {}", e);
                    break;
                }
            }
        }
    });

    let state_clone2 = state.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    let mut s = state_clone2.lock().unwrap();
                    s.log_lines.push(format!("[stderr] {}", line));
                    if s.log_lines.len() > 500 {
                        s.log_lines.drain(0..100);
                    }
                    if matches!(s.status, ServerStatus::Starting)
                        && (line.contains("HTTP server listening")
                            || line.contains("server is listening"))
                    {
                        s.status = ServerStatus::Running { port, pid };
                    }
                    drop(s);
                    log_cb_clone(format!("[stderr] {}", line));
                }
                Err(e) => {
                    log::warn!("Error reading server stderr: {}", e);
                    break;
                }
            }
        }
    });

    // Monitor process exit in background via polling
    let state_clone3 = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            let exit_status = {
                let mut s = state_clone3.lock().unwrap();
                match s.process.as_mut() {
                    Some(child) => child.try_wait(),
                    None => break, // Child was taken by stop_server
                }
            };

            match exit_status {
                Ok(Some(status)) => {
                    let mut s = state_clone3.lock().unwrap();
                    s.process = None;
                    if s.status == ServerStatus::Stopped {
                        // Already marked stopped by stop_server
                    } else if status.success() {
                        s.status = ServerStatus::Stopped;
                    } else {
                        let msg = format!("Server exited with code {}", status);
                        s.log_lines.push(format!("[error] {}", msg));
                        s.status = ServerStatus::Error { message: msg.clone() };
                        drop(s);
                        log_cb_exit(format!("[error] {}", msg));
                    }
                    break;
                }
                Ok(None) => continue, // Still running
                Err(e) => {
                    let mut s = state_clone3.lock().unwrap();
                    s.process = None;
                    let msg = format!("Server process error: {}", e);
                    s.log_lines.push(format!("[error] {}", msg));
                    s.status = ServerStatus::Error { message: msg.clone() };
                    drop(s);
                    log_cb_exit(format!("[error] {}", msg));
                    break;
                }
            }
        }
    });

    Ok(())
}

pub async fn stop_server(state: &SharedServerState) -> Result<()> {
    let mut child = {
        let mut s = state.lock().unwrap();
        s.status = ServerStatus::Stopped;
        s.process.take()
    };

    if let Some(ref mut child) = child {
        // Send SIGTERM for graceful shutdown on Unix
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            log::info!("Sent SIGTERM to server (pid {})", pid);
        }

        // On Windows, start_kill sends TerminateProcess immediately
        #[cfg(not(unix))]
        {
            let _ = child.start_kill();
        }

        // Wait up to 30 seconds for graceful shutdown
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            child.wait(),
        )
        .await
        {
            Ok(Ok(status)) => {
                log::info!("Server exited: {}", status);
            }
            Ok(Err(e)) => {
                log::warn!("Error waiting for server exit: {}", e);
            }
            Err(_) => {
                // Timed out — force kill
                log::warn!("Server did not stop within 30 seconds, force killing");
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
    }

    Ok(())
}

pub fn build_args(config: &ServerConfig) -> Vec<String> {
    let mut args = Vec::new();

    args.push("--model".to_string());
    args.push(config.model_path.clone());

    if let Some(ref mmproj) = config.mmproj_path {
        if !mmproj.is_empty() {
            args.push("--mmproj".to_string());
            args.push(mmproj.clone());
        }
    }

    args.push("--host".to_string());
    args.push(config.host.clone());

    args.push("--port".to_string());
    args.push(config.port.to_string());

    args.push("--ctx-size".to_string());
    args.push(config.n_ctx.to_string());

    args.push("--n-gpu-layers".to_string());
    args.push(config.n_gpu_layers.to_string());

    if let Some(threads) = config.n_threads {
        args.push("--threads".to_string());
        args.push(threads.to_string());
    }

    args.push("--flash-attn".to_string());
    args.push(config.flash_attn.clone());

    args.push("--cache-type-k".to_string());
    args.push(config.cache_type_k.clone());

    args.push("--cache-type-v".to_string());
    args.push(config.cache_type_v.clone());

    args.push("--temp".to_string());
    args.push(format!("{:.2}", config.temperature));

    args.push("--top-k".to_string());
    args.push(config.top_k.to_string());

    args.push("--min-p".to_string());
    args.push(format!("{:.4}", config.min_p));

    args.push("--top-p".to_string());
    args.push(format!("{:.4}", config.top_p));

    if config.n_predict != -1 {
        args.push("--n-predict".to_string());
        args.push(config.n_predict.to_string());
    }

    args.push("--batch-size".to_string());
    args.push(config.n_batch.to_string());

    args.push("--ubatch-size".to_string());
    args.push(config.n_ubatch.to_string());

    if config.cont_batching {
        args.push("--cont-batching".to_string());
    }

    if config.mlock {
        args.push("--mlock".to_string());
    }

    if config.no_mmap {
        args.push("--no-mmap".to_string());
    }

    if let Some(seed) = config.seed {
        args.push("--seed".to_string());
        args.push(seed.to_string());
    }

    if let Some(scale) = config.rope_freq_scale {
        args.push("--rope-freq-scale".to_string());
        args.push(format!("{:.6}", scale));
    }

    if let Some(base) = config.rope_freq_base {
        args.push("--rope-freq-base".to_string());
        args.push(format!("{:.1}", base));
    }

    if let Some(n) = config.grp_attn_n {
        args.push("--grp-attn-n".to_string());
        args.push(n.to_string());
    }

    if let Some(w) = config.grp_attn_w {
        args.push("--grp-attn-w".to_string());
        args.push(w.to_string());
    }

    if config.parallel > 1 {
        args.push("--parallel".to_string());
        args.push(config.parallel.to_string());
    }

    // Extra parameters from the UI
    let mut sorted_params: Vec<_> = config.extra_params.iter()
        .filter(|(k, _)| k.as_str() != "__raw__" && k.as_str() != "mmproj")
        .collect();
    sorted_params.sort_by_key(|(k, _)| (*k).clone());
    for (key, value) in sorted_params {
        args.push(format!("--{}", key));
        if !value.is_empty() {
            args.push(value.clone());
        }
    }

    // Raw extra arguments (free-form text from the UI)
    if let Some(raw) = config.extra_params.get("__raw__") {
        for part in raw.split_whitespace() {
            args.push(part.to_string());
        }
    }

    args
}

/// Build a suggested config based on system info and model size
pub fn suggest_server_config(
    model_path: &str,
    model_size_mb: u64,
    system: &SystemInfo,
) -> ServerConfig {
    let suggestion = suggest_config(model_size_mb, system);

    let cache_type_k = if suggestion.can_fit_fully_in_vram || suggestion.total_usable_mb > 8192 {
        "f16".to_string()
    } else {
        "q8_0".to_string() // Save memory
    };

    ServerConfig {
        model_path: model_path.to_string(),
        n_ctx: suggestion.n_ctx,
        n_gpu_layers: suggestion.n_gpu_layers,
        flash_attn: "auto".to_string(),
        cache_type_k,
        cache_type_v: "f16".to_string(),
        ..ServerConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_default_config() {
        let config = ServerConfig {
            model_path: "/path/to/model.gguf".to_string(),
            ..Default::default()
        };
        let args = build_args(&config);

        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"/path/to/model.gguf".to_string()));
        assert!(args.contains(&"--host".to_string()));
        assert!(args.contains(&"127.0.0.1".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"8080".to_string()));
        assert!(args.contains(&"--ctx-size".to_string()));
        assert!(args.contains(&"0".to_string()));
        assert!(args.contains(&"--flash-attn".to_string()));
        assert!(args.contains(&"auto".to_string()));
    }

    #[test]
    fn build_args_optional_fields() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            n_threads: Some(8),
            seed: Some(42),
            parallel: 4,
            ..Default::default()
        };
        let args = build_args(&config);

        assert!(args.contains(&"--threads".to_string()));
        assert!(args.contains(&"8".to_string()));
        assert!(args.contains(&"--seed".to_string()));
        assert!(args.contains(&"42".to_string()));
        assert!(args.contains(&"--parallel".to_string()));
        assert!(args.contains(&"4".to_string()));
    }

    #[test]
    fn build_args_extra_params() {
        let mut extra = HashMap::new();
        extra.insert("api-key".to_string(), "secret".to_string());
        extra.insert("metrics".to_string(), String::new()); // boolean flag
        extra.insert("__raw__".to_string(), "--verbose --log-timestamps".to_string());

        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            extra_params: extra,
            ..Default::default()
        };
        let args = build_args(&config);

        // Named extra params (sorted alphabetically)
        let api_key_idx = args.iter().position(|a| a == "--api-key").unwrap();
        assert_eq!(args[api_key_idx + 1], "secret");

        assert!(args.contains(&"--metrics".to_string()));

        // Raw args split and appended at the end
        let verbose_idx = args.iter().position(|a| a == "--verbose").unwrap();
        let timestamps_idx = args.iter().position(|a| a == "--log-timestamps").unwrap();
        assert!(verbose_idx > api_key_idx);
        assert!(timestamps_idx > verbose_idx);
    }

    #[test]
    fn build_args_no_parallel_when_one() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            parallel: 1,
            ..Default::default()
        };
        let args = build_args(&config);
        assert!(!args.contains(&"--parallel".to_string()));
    }

    #[test]
    fn build_args_omits_none_threads() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            n_threads: None,
            ..Default::default()
        };
        let args = build_args(&config);
        assert!(!args.contains(&"--threads".to_string()));
    }

    // ── apply_hf_preset_params ───────────────────────────────────────────────

    fn make_hf_params() -> crate::huggingface::HfPresetParams {
        crate::huggingface::HfPresetParams {
            temperature: Some(0.6),
            top_k: Some(30),
            top_p: Some(0.85),
            min_p: Some(0.02),
            n_predict: Some(1024),
            seed: Some(123),
            repeat_penalty: Some(1.15),
            repeat_last_n: Some(64),
        }
    }

    #[test]
    fn apply_hf_preset_updates_sampling_fields() {
        let mut cfg = ServerConfig::default();
        apply_hf_preset_params(&make_hf_params(), &mut cfg);

        assert!((cfg.temperature - 0.6).abs() < 1e-5);
        assert_eq!(cfg.top_k, 30);
        assert!((cfg.top_p - 0.85).abs() < 1e-5);
        assert!((cfg.min_p - 0.02).abs() < 1e-5);
        assert_eq!(cfg.n_predict, 1024);
        assert_eq!(cfg.seed, Some(123));
    }

    #[test]
    fn apply_hf_preset_puts_repeat_in_extra_params() {
        let mut cfg = ServerConfig::default();
        apply_hf_preset_params(&make_hf_params(), &mut cfg);

        assert!(cfg.extra_params.contains_key("repeat-penalty"),
            "repeat_penalty should be stored in extra_params");
        assert!(cfg.extra_params.contains_key("repeat-last-n"),
            "repeat_last_n should be stored in extra_params");
        let rp: f32 = cfg.extra_params["repeat-penalty"].parse().unwrap();
        assert!((rp - 1.15).abs() < 1e-3);
        assert_eq!(cfg.extra_params["repeat-last-n"], "64");
    }

    #[test]
    fn apply_hf_preset_none_fields_preserve_defaults() {
        let mut cfg = ServerConfig::default();
        let default_temp = cfg.temperature;
        let params = crate::huggingface::HfPresetParams::default(); // all None
        apply_hf_preset_params(&params, &mut cfg);

        // Nothing should have changed
        assert!((cfg.temperature - default_temp).abs() < 1e-5);
        assert!(cfg.extra_params.is_empty());
        assert_eq!(cfg.seed, None);
    }

    #[test]
    fn apply_hf_preset_does_not_touch_hardware_fields() {
        let mut cfg = ServerConfig {
            n_gpu_layers: 99,
            n_ctx: 4096,
            n_threads: Some(8),
            ..Default::default()
        };
        apply_hf_preset_params(&make_hf_params(), &mut cfg);
        // Hardware fields must be untouched
        assert_eq!(cfg.n_gpu_layers, 99);
        assert_eq!(cfg.n_ctx, 4096);
        assert_eq!(cfg.n_threads, Some(8));
    }

    // ── preset_name_from_repo ────────────────────────────────────────────────

    #[test]
    fn preset_name_from_repo_replaces_slash() {
        assert_eq!(preset_name_from_repo("unsloth/Qwen3.5-4B-GGUF"), "unsloth__Qwen3.5-4B-GGUF");
    }

    #[test]
    fn preset_name_from_repo_no_slash() {
        assert_eq!(preset_name_from_repo("plain-name"), "plain-name");
    }
}

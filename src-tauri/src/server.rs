use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt;

use crate::config::AppConfig;
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
    /// Whether this server was started by this app
    pub started_by_us: bool,
    /// For external servers: the PID we detected
    pub external_pid: Option<u32>,
    /// Whether we're in the process of stopping (to prevent immediate re-detection)
    pub stopping: bool,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            process: None,
            status: ServerStatus::Stopped,
            log_lines: Vec::new(),
            started_by_us: false,
            external_pid: None,
            stopping: false,
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

/// Rename/drop flag keys that were removed or renamed in newer llama.cpp builds.
/// Idempotent and safe to call on any `extra_params` map. Returns true if any
/// changes were made.
pub fn migrate_extra_params(extra: &mut HashMap<String, String>) -> bool {
    // Removed entirely (no automatic equivalent — meaning depended on --spec-type
    // which would now need user attention). Drop them so the server doesn't
    // refuse to start with an "argument has been removed" error.
    const REMOVED_DROP: &[&str] = &[
        "spec-ngram-size-n",
        "spec-ngram-size-m",
        "spec-ngram-min-hits",
    ];
    // Old → canonical rename. Some are still recognized by llama.cpp as aliases,
    // but we normalise to the canonical form so the UI and saved presets stay
    // consistent.
    const RENAMES: &[(&str, &str)] = &[
        // Removed entirely — must be migrated
        ("draft", "spec-draft-n-max"),
        ("draft-max", "spec-draft-n-max"),
        ("draft-n-max", "spec-draft-n-max"),
        ("draft-min", "spec-draft-n-min"),
        ("draft-n-min", "spec-draft-n-min"),
        // Still accepted as aliases — normalise to canonical
        ("model-draft", "spec-draft-model"),
        ("ctx-size-draft", "spec-draft-ctx-size"),
        ("n-gpu-layers-draft", "spec-draft-ngl"),
        ("gpu-layers-draft", "spec-draft-ngl"),
        ("device-draft", "spec-draft-device"),
        ("threads-draft", "spec-draft-threads"),
        ("threads-batch-draft", "spec-draft-threads-batch"),
        ("cpu-moe-draft", "spec-draft-cpu-moe"),
        ("draft-cpu-moe", "spec-draft-cpu-moe"),
        ("n-cpu-moe-draft", "spec-draft-n-cpu-moe"),
        ("override-tensor-draft", "spec-draft-override-tensor"),
        ("draft-p-min", "spec-draft-p-min"),
        ("draft-p-split", "spec-draft-p-split"),
        ("hf-repo-draft", "spec-draft-hf"),
        ("cache-type-k-draft", "spec-draft-type-k"),
        ("cache-type-v-draft", "spec-draft-type-v"),
    ];

    let mut changed = false;
    for k in REMOVED_DROP {
        if extra.remove(*k).is_some() {
            changed = true;
        }
    }
    for (old, new) in RENAMES {
        if let Some(v) = extra.remove(*old) {
            // Don't clobber an explicitly-set canonical value
            extra.entry((*new).to_string()).or_insert(v);
            changed = true;
        }
    }
    changed
}

/// Helper function to get log file path
fn log_file_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("catapult").join("server.log"))
}

pub async fn start_server(
    server_binary: &PathBuf,
    config: &ServerConfig,
    state: SharedServerState,
    log_cb: impl Fn(String) + Send + Sync + 'static,
) -> Result<()> {
    // Check not already running, and clear stopping flag
    {
        let mut s = state.lock().unwrap();
        if s.is_running() {
            anyhow::bail!("Server is already running");
        }
        s.stopping = false; // 允许重新启动，清除停止标志
    }

    let args = build_args(config);

    let cmdline = format!("{} {}", server_binary.display(), args.join(" "));
    log::info!("Starting llama-server: {}", cmdline);

    // Open log file for writing (append mode)
    let log_file = if let Some(log_path) = log_file_path() {
        // Ensure parent directory exists
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Write command header
        let _ = std::fs::write(&log_path, format!("# {}\n", cmdline));
        // Open in append mode
        std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok()
    } else {
        None
    };

    let mut cmd = Command::new(server_binary);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn()
        .context("Failed to spawn llama-server")?;

    let pid = child.id().unwrap_or(0);
    let port = config.port;

    // Write PID file like TUI does
    if let Some(pid_path) = pid_file_path() {
        let _ = std::fs::write(&pid_path, format!("{}\n{}", pid, server_binary.display()));
    }

    // Take stdout/stderr before storing child in state
    let stdout = child.stdout.take().expect("stdout not piped");
    let stderr = child.stderr.take().expect("stderr not piped");

    {
        let mut s = state.lock().unwrap();
        s.process = Some(child);
        s.status = ServerStatus::Starting;
        s.log_lines.clear();
        s.started_by_us = true;
        s.external_pid = None;
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

    // Share the log file across both tasks
    let log_file_arc = Arc::new(Mutex::new(log_file));

    let log_file_for_stdout = log_file_arc.clone();
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

                    // Write to log file
                    if let Ok(mut f) = log_file_for_stdout.lock() {
                        if let Some(ref mut file) = *f {
                            use std::io::Write;
                            let _ = writeln!(file, "{}", line);
                            let _ = file.flush();
                        }
                    }

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
    let log_file_for_stderr = log_file_arc.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    let stderr_line = format!("[stderr] {}", line);
                    let mut s = state_clone2.lock().unwrap();
                    s.log_lines.push(stderr_line.clone());
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

                    // Write to log file
                    if let Ok(mut f) = log_file_for_stderr.lock() {
                        if let Some(ref mut file) = *f {
                            use std::io::Write;
                            let _ = writeln!(file, "{}", stderr_line);
                            let _ = file.flush();
                        }
                    }

                    log_cb_clone(stderr_line);
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

/// Stop a server by PID (for external processes)
fn stop_server_by_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        // Send SIGTERM
        let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if ret != 0 {
            anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
        }

        // Wait up to 30 seconds
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !process_alive(pid) {
                cleanup_pid_file();
                return Ok(());
            }
        }

        // Force kill
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        cleanup_pid_file();
        Ok(())
    }

    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut std::ffi::c_void;
            fn TerminateProcess(hProcess: *mut std::ffi::c_void, uExitCode: u32) -> i32;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }
        // PROCESS_TERMINATE = 0x0001
        let handle = unsafe { OpenProcess(0x0001, 0, pid) };
        if handle.is_null() {
            anyhow::bail!("Failed to open process {} for termination", pid);
        }
        unsafe {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        cleanup_pid_file();
        Ok(())
    }
}

fn cleanup_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

pub async fn stop_server(state: &SharedServerState) -> Result<()> {
    let (mut child, maybe_external_pid) = {
        let mut s = state.lock().unwrap();
        s.status = ServerStatus::Stopped;
        s.started_by_us = false;
        let external_pid = s.external_pid.take();
        s.stopping = true; // 标记为正在停止，防止立即重新检测
        (s.process.take(), external_pid)
    };

    if let Some(ref mut child) = child {
        // Stop process we started
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            log::info!("Sent SIGTERM to server (pid {})", pid);
        }

        #[cfg(not(unix))]
        {
            let _ = child.start_kill();
        }

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
                log::warn!("Server did not stop within 30 seconds, force killing");
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        }
        cleanup_pid_file();
    } else if let Some(pid) = maybe_external_pid {
        // Stop external process
        log::info!("Stopping external server (pid {})", pid);
        stop_server_by_pid(pid)?;
    }

    Ok(())
}

/// Synchronous kill for use during app exit — sends SIGTERM/TerminateProcess
/// and waits briefly for the process to exit, only if we started it.
pub fn kill_server_sync(state: &SharedServerState) {
    let mut child = {
        let mut s = state.lock().unwrap();
        if !s.started_by_us {
            // Don't kill external servers on app exit
            log::info!("Not killing server on exit (external or from TUI)");
            return;
        }
        s.status = ServerStatus::Stopped;
        s.process.take()
    };

    if let Some(ref mut child) = child {
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            log::info!("Sent SIGTERM to server on exit (pid {})", pid);
        }

        #[cfg(not(unix))]
        {
            let _ = child.start_kill();
        }

        // Block briefly to let the process clean up
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            match child.try_wait() {
                Ok(Some(status)) => {
                    log::info!("Server exited on shutdown: {}", status);
                    return;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => break,
            }
        }

        // Force kill if still alive
        let _ = child.start_kill();
        let _ = child.try_wait();
        log::warn!("Force-killed server on shutdown");
    }
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
    } else {
        args.push("--no-cont-batching".to_string());
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

    args.push("--parallel".to_string());
    args.push(config.parallel.to_string());

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

// ── External Server Detection ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DetectedServer {
    pub pid: u32,
    pub binary_path: PathBuf,
    pub port: u16,
    pub model_path: Option<String>,
    pub runtime_label: Option<String>,
    pub origin: ServerOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerOrigin {
    Tui,
    External,
    ExternalUnknown,
}

fn pid_file_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("catapult").join("server.pid"))
}

/// Detects any running llama-server process (from any source) - returns detailed info
pub fn detect_server(config: &AppConfig) -> Option<DetectedServer> {
    if let Some(tui_server) = check_pid_file(config) {
        return Some(tui_server);
    }
    scan_proc_for_servers(config)
}

/// Simple version for GUI: only returns pid and port
pub fn detect_server_simple(config: &AppConfig) -> Option<(u32, u16)> {
    detect_server(config).map(|s| (s.pid, s.port))
}

fn check_pid_file(config: &AppConfig) -> Option<DetectedServer> {
    let pid_path = pid_file_path()?;
    let content = std::fs::read_to_string(&pid_path).ok()?;
    let mut lines = content.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let binary_path_str = lines.next().unwrap_or("");

    if !process_alive(pid) {
        let _ = std::fs::remove_file(&pid_path);
        return None;
    }

    let binary_path = if binary_path_str.is_empty() {
        std::fs::read_link(format!("/proc/{}/exe", pid)).unwrap_or_default()
    } else {
        PathBuf::from(binary_path_str)
    };

    let port = read_port_from_proc(pid).unwrap_or(8080);
    let model_path = read_arg_from_proc(pid, "--model").or_else(|| read_arg_from_proc(pid, "-m"));
    let runtime_label = match_runtime(&binary_path, config);

    Some(DetectedServer {
        pid,
        binary_path,
        port,
        model_path,
        runtime_label,
        origin: ServerOrigin::Tui,
    })
}

#[cfg(target_os = "linux")]
fn scan_proc_for_servers(config: &AppConfig) -> Option<DetectedServer> {
    use std::path::Path;
    let proc_dir = std::fs::read_dir("/proc").ok()?;

    for entry in proc_dir.flatten() {
        let pid_str = entry.file_name();
        let pid_str = pid_str.to_str()?;
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = match std::fs::read(&cmdline_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        let args: Vec<&str> = cmdline
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::str::from_utf8(s).ok())
            .collect();

        if args.is_empty() {
            continue;
        }

        let exe_name = Path::new(args[0])
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if exe_name != "llama-server" && exe_name != "llama-server.exe" {
            continue;
        }

        let binary_path = std::fs::read_link(format!("/proc/{}/exe", pid))
            .unwrap_or_else(|_| PathBuf::from(args[0]));

        let port = args
            .windows(2)
            .find(|w| w[0] == "--port" || w[0] == "-p")
            .and_then(|w| w[1].parse().ok())
            .unwrap_or(8080);

        let model_path = args
            .windows(2)
            .find(|w| w[0] == "--model" || w[0] == "-m")
            .map(|w| w[1].to_string());

        let runtime_label = match_runtime(&binary_path, config);
        let origin = if runtime_label.is_some() {
            ServerOrigin::External
        } else {
            ServerOrigin::ExternalUnknown
        };

        return Some(DetectedServer {
            pid,
            binary_path,
            port,
            model_path,
            runtime_label,
            origin,
        });
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn scan_proc_for_servers(config: &AppConfig) -> Option<DetectedServer> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(UpdateKind::Always)
                .with_exe(UpdateKind::Always),
        ),
    );

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy();
        if name != "llama-server" && name != "llama-server.exe" {
            continue;
        }

        let cmd = process.cmd();
        let binary_path = process.exe().map(PathBuf::from).unwrap_or_default();

        let port = cmd
            .windows(2)
            .find(|w| w[0] == "--port" || w[0] == "-p")
            .and_then(|w| w[1].to_str()?.parse().ok())
            .unwrap_or(8080);

        let model_path = cmd
            .windows(2)
            .find(|w| w[0] == "--model" || w[0] == "-m")
            .map(|w| w[1].to_string_lossy().into_owned());

        let runtime_label = match_runtime(&binary_path, config);
        let origin = if runtime_label.is_some() {
            ServerOrigin::External
        } else {
            ServerOrigin::ExternalUnknown
        };

        return Some(DetectedServer {
            pid: pid.as_u32(),
            binary_path,
            port,
            model_path,
            runtime_label,
            origin,
        });
    }

    None
}

fn match_runtime(binary_path: &PathBuf, config: &AppConfig) -> Option<String> {
    let binary_str = binary_path.to_string_lossy();
    for rt in &config.managed_runtimes {
        if binary_str.contains(&rt.dir_name) {
            return Some(format!("b{} {}", rt.build, rt.backend_label));
        }
    }
    for rt in &config.custom_runtimes {
        if binary_path == &rt.binary_path {
            return Some(rt.label.clone());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn read_port_from_proc(pid: u32) -> Option<u16> {
    read_arg_from_proc(pid, "--port")
        .or_else(|| read_arg_from_proc(pid, "-p"))
        .and_then(|s| s.parse().ok())
}

#[cfg(not(target_os = "linux"))]
fn read_port_from_proc(pid: u32) -> Option<u16> {
    read_arg_from_proc(pid, "--port")
        .or_else(|| read_arg_from_proc(pid, "-p"))
        .and_then(|s| s.parse().ok())
}

#[cfg(target_os = "linux")]
fn read_arg_from_proc(pid: u32, flag: &str) -> Option<String> {
    let cmdline = std::fs::read(format!("/proc/{}/cmdline", pid)).ok()?;
    let args: Vec<&str> = cmdline
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok())
        .collect();
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].to_string())
}

#[cfg(not(target_os = "linux"))]
fn read_arg_from_proc(pid: u32, flag: &str) -> Option<String> {
    use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System, UpdateKind};
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cmd(UpdateKind::Always)),
    );
    let process = sys.process(Pid::from(pid as usize))?;
    process
        .cmd()
        .windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].to_string_lossy().into_owned())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    unsafe extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(hProcess: *mut std::ffi::c_void, lpExitCode: *mut u32) -> i32;
        fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x1000, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        exit_code == 259
    }
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
    fn build_args_parallel_always_emitted() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            parallel: 1,
            ..Default::default()
        };
        let args = build_args(&config);
        let idx = args.iter().position(|a| a == "--parallel").unwrap();
        assert_eq!(args[idx + 1], "1");
    }

    #[test]
    fn build_args_no_cont_batching() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            cont_batching: false,
            ..Default::default()
        };
        let args = build_args(&config);
        assert!(args.contains(&"--no-cont-batching".to_string()));
        assert!(!args.contains(&"--cont-batching".to_string()));
    }

    #[test]
    fn build_args_parallel_emitted_for_higher_values() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            parallel: 8,
            ..Default::default()
        };
        let args = build_args(&config);
        let idx = args.iter().position(|a| a == "--parallel").unwrap();
        assert_eq!(args[idx + 1], "8");
    }

    #[test]
    fn build_args_cont_batching_when_enabled() {
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            cont_batching: true,
            ..Default::default()
        };
        let args = build_args(&config);
        assert!(args.contains(&"--cont-batching".to_string()));
        assert!(!args.contains(&"--no-cont-batching".to_string()));
    }

    #[test]
    fn build_args_default_has_parallel_and_cont_batching() {
        // Default config: parallel=1, cont_batching=true
        let config = ServerConfig {
            model_path: "/m.gguf".to_string(),
            ..Default::default()
        };
        let args = build_args(&config);
        // parallel=1 must be emitted (not omitted)
        let idx = args.iter().position(|a| a == "--parallel").unwrap();
        assert_eq!(args[idx + 1], "1");
        // cont_batching=true emits --cont-batching
        assert!(args.contains(&"--cont-batching".to_string()));
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

    // ── kill_server_sync ─────────────────────────────────────────────────────

    #[test]
    fn kill_server_sync_no_process_is_noop() {
        let state = new_server_state();
        // Must not panic when no server is running
        kill_server_sync(&state);
        let s = state.lock().unwrap();
        assert!(matches!(s.status, ServerStatus::Stopped));
        assert!(s.process.is_none());
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

    // ── migrate_extra_params ─────────────────────────────────────────────────

    #[test]
    fn migrate_drops_removed_ngram_size_flags() {
        let mut ep = HashMap::new();
        ep.insert("spec-ngram-size-n".to_string(), "3".to_string());
        ep.insert("spec-ngram-size-m".to_string(), "5".to_string());
        ep.insert("spec-ngram-min-hits".to_string(), "1".to_string());
        ep.insert("kept".to_string(), "1".to_string());

        assert!(migrate_extra_params(&mut ep));
        assert!(!ep.contains_key("spec-ngram-size-n"));
        assert!(!ep.contains_key("spec-ngram-size-m"));
        assert!(!ep.contains_key("spec-ngram-min-hits"));
        assert_eq!(ep.get("kept"), Some(&"1".to_string()));
    }

    #[test]
    fn migrate_renames_removed_draft_flags_to_spec_draft() {
        let mut ep = HashMap::new();
        ep.insert("draft".to_string(), "16".to_string());
        ep.insert("draft-min".to_string(), "0".to_string());

        assert!(migrate_extra_params(&mut ep));
        assert_eq!(ep.get("spec-draft-n-max"), Some(&"16".to_string()));
        assert_eq!(ep.get("spec-draft-n-min"), Some(&"0".to_string()));
        assert!(!ep.contains_key("draft"));
        assert!(!ep.contains_key("draft-min"));
    }

    #[test]
    fn migrate_canonicalises_draft_aliases() {
        let mut ep = HashMap::new();
        ep.insert("model-draft".to_string(), "/p/d.gguf".to_string());
        ep.insert("ctx-size-draft".to_string(), "4096".to_string());
        ep.insert("n-gpu-layers-draft".to_string(), "99".to_string());
        ep.insert("threads-draft".to_string(), "4".to_string());
        ep.insert("device-draft".to_string(), "cuda0".to_string());
        ep.insert("cpu-moe-draft".to_string(), String::new());

        assert!(migrate_extra_params(&mut ep));
        assert_eq!(ep.get("spec-draft-model"), Some(&"/p/d.gguf".to_string()));
        assert_eq!(ep.get("spec-draft-ctx-size"), Some(&"4096".to_string()));
        assert_eq!(ep.get("spec-draft-ngl"), Some(&"99".to_string()));
        assert_eq!(ep.get("spec-draft-threads"), Some(&"4".to_string()));
        assert_eq!(ep.get("spec-draft-device"), Some(&"cuda0".to_string()));
        assert_eq!(ep.get("spec-draft-cpu-moe"), Some(&String::new()));
        assert!(!ep.contains_key("model-draft"));
    }

    #[test]
    fn migrate_does_not_clobber_explicit_canonical_value() {
        let mut ep = HashMap::new();
        ep.insert("spec-draft-n-max".to_string(), "32".to_string());
        ep.insert("draft".to_string(), "16".to_string());

        migrate_extra_params(&mut ep);
        // Canonical wins; the legacy key is removed.
        assert_eq!(ep.get("spec-draft-n-max"), Some(&"32".to_string()));
        assert!(!ep.contains_key("draft"));
    }

    #[test]
    fn migrate_idempotent_on_clean_map() {
        let mut ep = HashMap::new();
        ep.insert("spec-default".to_string(), String::new());
        ep.insert("temp".to_string(), "0.7".to_string());
        let before = ep.clone();
        assert!(!migrate_extra_params(&mut ep));
        assert_eq!(ep, before);
    }
}

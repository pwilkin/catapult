pub mod config;
pub mod hardware;
pub mod huggingface;
pub mod models;
pub mod runtime;
pub mod server;

use config::AppConfig;
use hardware::{suggest_config, BackendInfo, SystemInfo};
use huggingface::{HfFile, HfFilePart, HfModel, KNOWN_GGUF_OWNERS};
use models::{ModelInfo, RecommendedModel};
use runtime::{CustomBuild, ReleaseInfo, RuntimeInfo};
use server::{ServerConfig, ServerStatus, SharedServerState};

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder, WebviewUrl};

// ── App State ────────────────────────────────────────────────────────────────

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub server: SharedServerState,
    pub http_client: reqwest::Client,
    /// Active download cancellation flags: id -> cancelled
    pub downloads: Mutex<HashMap<String, bool>>,
}

// ── Hardware commands ─────────────────────────────────────────────────────────

#[tauri::command]
async fn get_system_info(_state: State<'_, AppState>) -> Result<SystemInfo, String> {
    hardware::get_system_info().map_err(|e| e.to_string())
}

#[tauri::command]
async fn suggest_model_config(
    model_size_mb: u64,
    _state: State<'_, AppState>,
) -> Result<hardware::SuggestedConfig, String> {
    let system = hardware::get_system_info().map_err(|e| e.to_string())?;
    Ok(suggest_config(model_size_mb, &system))
}

// ── Runtime commands ──────────────────────────────────────────────────────────

#[tauri::command]
async fn get_runtime_info(state: State<'_, AppState>) -> Result<RuntimeInfo, String> {
    let config = state.config.lock().unwrap().clone();
    runtime::get_runtime_info(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_latest_release(state: State<'_, AppState>) -> Result<ReleaseInfo, String> {
    let system = hardware::get_system_info().map_err(|e| e.to_string())?;
    let available_ids: Vec<String> = system
        .available_backends
        .iter()
        .filter(|b| b.available)
        .map(|b| b.id.clone())
        .collect();
    runtime::fetch_latest_release(&state.http_client, &available_ids)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_runtime(
    app: AppHandle,
    asset_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let system = hardware::get_system_info().map_err(|e| e.to_string())?;
    let available_ids: Vec<String> = system
        .available_backends
        .iter()
        .filter(|b| b.available)
        .map(|b| b.id.clone())
        .collect();
    let release = runtime::fetch_latest_release(&state.http_client, &available_ids)
        .await
        .map_err(|e| e.to_string())?;

    let asset = release
        .available_assets
        .iter()
        .find(|a| a.name == asset_name)
        .cloned()
        .ok_or_else(|| format!("Asset '{}' not found in release", asset_name))?;

    let tag_name = release.tag_name.clone();
    let mut config = state.config.lock().unwrap().clone();

    state.downloads.lock().unwrap().insert("runtime".to_string(), false);

    let result = runtime::download_runtime(
        &state.http_client,
        &asset,
        &tag_name,
        &mut config,
        move |progress| {
            let _ = app.emit("download_progress", &progress);
        },
    )
    .await;

    state.downloads.lock().unwrap().remove("runtime");

    match result {
        Ok(()) => {
            *state.config.lock().unwrap() = config;
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn set_custom_runtime(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    let mut config = state.config.lock().unwrap().clone();
    runtime::set_custom_runtime(&path, &mut config).map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
async fn scan_custom_runtime(path: String) -> Result<Vec<CustomBuild>, String> {
    let path = std::path::PathBuf::from(path);
    runtime::scan_for_builds(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_custom_runtime_binary(binary_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let binary_path = std::path::PathBuf::from(binary_path);
    let mut config = state.config.lock().unwrap().clone();
    // Add to custom runtimes list and activate
    let label = binary_path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom".to_string());
    let index = config.custom_runtimes.len();
    // Avoid duplicates
    if let Some(existing) = config.custom_runtimes.iter().position(|c| c.binary_path == binary_path) {
        config.active_runtime = config::ActiveRuntime::Custom { index: existing };
    } else {
        config.custom_runtimes.push(config::CustomRuntime {
            label,
            binary_path,
        });
        config.active_runtime = config::ActiveRuntime::Custom { index };
    }
    config.save().map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
async fn set_active_runtime(
    runtime_type: String,
    id: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.active_runtime = match runtime_type.as_str() {
        "managed" => {
            let build = id as u32;
            if !config.managed_runtimes.iter().any(|r| r.build == build) {
                return Err(format!("Managed runtime b{} not found", build));
            }
            config::ActiveRuntime::Managed { build }
        }
        "custom" => {
            if id >= config.custom_runtimes.len() {
                return Err(format!("Custom runtime index {} not found", id));
            }
            config::ActiveRuntime::Custom { index: id }
        }
        _ => return Err("Invalid runtime type".to_string()),
    };
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_managed_runtime(build: u32, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap().clone();
    runtime::delete_managed_runtime(build, &mut config).map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
async fn remove_custom_runtime(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    if index >= config.custom_runtimes.len() {
        return Err("Invalid index".to_string());
    }
    if config.active_runtime == (config::ActiveRuntime::Custom { index }) {
        return Err("Cannot remove the active runtime. Switch to another first.".to_string());
    }
    config.custom_runtimes.remove(index);
    // Fix active index if it shifted
    if let config::ActiveRuntime::Custom { index: ref mut active_idx } = config.active_runtime {
        if *active_idx > index {
            *active_idx -= 1;
        }
    }
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_auto_delete_runtimes(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.auto_delete_old_runtimes = enabled;
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_available_backends(_state: State<'_, AppState>) -> Result<Vec<BackendInfo>, String> {
    let system = hardware::get_system_info().map_err(|e| e.to_string())?;
    Ok(system.available_backends)
}

// ── Model commands ────────────────────────────────────────────────────────────

#[tauri::command]
async fn list_installed_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let config = state.config.lock().unwrap().clone();
    models::list_installed_models(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_recommended_models(state: State<'_, AppState>) -> Result<Vec<RecommendedModel>, String> {
    let config = state.config.lock().unwrap().clone();
    models::get_recommended_models(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_known_owners() -> Vec<serde_json::Value> {
    KNOWN_GGUF_OWNERS
        .iter()
        .map(|(id, desc)| {
            serde_json::json!({ "id": id, "description": desc })
        })
        .collect()
}

#[tauri::command]
async fn search_hf_models(
    query: String,
    owner: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<HfModel>, String> {
    huggingface::search_models(&state.http_client, &query, owner.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_hf_repo_files(
    repo_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<HfFile>, String> {
    huggingface::get_repo_files(&state.http_client, &repo_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_model(
    app: AppHandle,
    repo_id: String,
    filename: String,
    download_url: String,
    size_bytes: u64,
    split_parts: Option<Vec<HfFilePart>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();

    let (is_split, parts) = match split_parts {
        Some(parts) if !parts.is_empty() => (true, parts),
        _ => (false, vec![]),
    };

    let file = HfFile {
        filename: filename.clone(),
        size_bytes,
        quant: huggingface::extract_quant(&filename),
        download_url,
        is_split,
        split_parts: parts,
    };

    // Mark download active
    state.downloads.lock().unwrap().insert(filename.clone(), false);

    let result = models::download_model(
        &state.http_client,
        &repo_id,
        &file,
        &config,
        move |progress| {
            let _ = app.emit("download_progress", &progress);
        },
    )
    .await;

    state.downloads.lock().unwrap().remove(&filename);

    result
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_model(path: String, _state: State<'_, AppState>) -> Result<(), String> {
    models::delete_model(&std::path::PathBuf::from(path)).map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel_download(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.lock().unwrap().insert(id, true);
    Ok(())
}

#[tauri::command]
async fn abort_download(filename: String, state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();
    models::abort_download(&filename, &config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_models_dir(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.lock().unwrap().clone();
    config
        .models_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_model_dirs(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state.config.lock().unwrap().clone();
    let dirs: Vec<String> = config
        .all_model_dirs()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let download_dir = config
        .models_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(serde_json::json!({
        "dirs": dirs,
        "download_dir": download_dir,
    }))
}

#[tauri::command]
async fn add_model_dir(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = std::path::PathBuf::from(&path);
    if !path.exists() {
        return Err(format!("Directory does not exist: {}", path.display()));
    }
    let mut config = state.config.lock().unwrap();
    if !config.model_dirs.contains(&path) {
        config.model_dirs.push(path);
    }
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_model_dir(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = std::path::PathBuf::from(&path);
    let mut config = state.config.lock().unwrap();
    config.model_dirs.retain(|p| p != &path);
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_download_dir(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = std::path::PathBuf::from(&path);
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    let mut config = state.config.lock().unwrap();
    // Also add to model_dirs if not already there
    if !config.model_dirs.contains(&path) {
        config.model_dirs.push(path.clone());
    }
    config.download_dir = Some(path);
    config.save().map_err(|e| e.to_string())
}

// ── Server commands ────────────────────────────────────────────────────────────

#[tauri::command]
async fn start_server(
    app: AppHandle,
    config: ServerConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let app_config = state.config.lock().unwrap().clone();

    let runtime_info = runtime::get_runtime_info(&app_config).map_err(|e| e.to_string())?;
    let server_binary = runtime_info
        .server_binary
        .ok_or_else(|| "Runtime not installed. Please download the runtime first.".to_string())?;

    let server_state = state.server.clone();

    server::start_server(
        &server_binary,
        &config,
        server_state,
        move |line| {
            let _ = app.emit("server_log", &line);
        },
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_server(state: State<'_, AppState>) -> Result<(), String> {
    server::stop_server(&state.server)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_server_status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    Ok(state.server.lock().unwrap().status.clone())
}

#[tauri::command]
async fn get_server_logs(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.server.lock().unwrap().log_lines.clone())
}

#[tauri::command]
async fn suggest_server_config(
    model_path: String,
    model_size_mb: u64,
    _state: State<'_, AppState>,
) -> Result<ServerConfig, String> {
    let system = hardware::get_system_info().map_err(|e| e.to_string())?;
    Ok(server::suggest_server_config(&model_path, model_size_mb, &system))
}

// ── Chat window ───────────────────────────────────────────────────────────────

#[tauri::command]
async fn open_chat_window(app: AppHandle, port: u16) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{}", port);
    let webview_url = WebviewUrl::External(url.parse().map_err(|e: url::ParseError| e.to_string())?);

    // Reuse an existing chat window if already open
    if let Some(win) = app.get_webview_window("chat") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "chat", webview_url)
        .title("Chat")
        .inner_size(960.0, 760.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ── Config commands ───────────────────────────────────────────────────────────

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
async fn set_models_dir(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    let mut config = state.config.lock().unwrap();
    config.models_dir = Some(path);
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_favorite_model(model_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    if let Some(pos) = config.favorite_models.iter().position(|id| id == &model_id) {
        config.favorite_models.remove(pos);
    } else {
        config.favorite_models.push(model_id);
    }
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_selected_model(model_path: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.selected_model = model_path;
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_wizard_completed(completed: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.wizard_completed = completed;
    config.save().map_err(|e| e.to_string())
}

// ── Server config presets ────────────────────────────────────────────────────

#[tauri::command]
async fn list_server_presets() -> Result<Vec<String>, String> {
    let dir = AppConfig::presets_dir().map_err(|e| e.to_string())?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

#[tauri::command]
async fn save_server_preset(name: String, config: ServerConfig) -> Result<(), String> {
    let dir = AppConfig::presets_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_server_preset(name: String) -> Result<ServerConfig, String> {
    let dir = AppConfig::presets_dir().map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_server_preset(name: String) -> Result<(), String> {
    let dir = AppConfig::presets_dir().map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Tauri app setup ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut config = AppConfig::load().unwrap_or_default();

    // --force-wizard resets the wizard flag so it runs again
    if std::env::args().any(|a| a == "--force-wizard" || a == "-w") {
        config.wizard_completed = false;
        let _ = config.save();
    }
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            config: Mutex::new(config),
            server: server::new_server_state(),
            http_client,
            downloads: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            // Hardware
            get_system_info,
            suggest_model_config,
            // Runtime
            get_runtime_info,
            check_latest_release,
            download_runtime,
            set_custom_runtime,
            scan_custom_runtime,
            set_custom_runtime_binary,
            set_active_runtime,
            delete_managed_runtime,
            remove_custom_runtime,
            set_auto_delete_runtimes,
            get_available_backends,
            // Models
            list_installed_models,
            get_recommended_models,
            get_known_owners,
            search_hf_models,
            get_hf_repo_files,
            download_model,
            delete_model,
            cancel_download,
            abort_download,
            get_models_dir,
            get_model_dirs,
            add_model_dir,
            remove_model_dir,
            set_download_dir,
            // Server
            start_server,
            stop_server,
            get_server_status,
            get_server_logs,
            suggest_server_config,
            open_chat_window,
            // Config
            get_config,
            set_models_dir,
            toggle_favorite_model,
            set_selected_model,
            set_wizard_completed,
            // Server config presets
            list_server_presets,
            save_server_preset,
            load_server_preset,
            delete_server_preset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

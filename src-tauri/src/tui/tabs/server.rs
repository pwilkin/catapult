use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use catapult_lib::config::AppConfig;
use catapult_lib::server::ServerConfig;

use crate::tui::app::{Action, Tab, TuiApp};
use crate::tui::event::TuiEvent;
use crate::tui::params::PARAMS;
use crate::tui::server_ctl;
use crate::tui::widgets::autocomplete::{self, AutocompleteItem, AutocompleteState};

// ── State ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServerFocus {
    Overrides,
    Search,
    ValueEdit,
    PresetPicker,
    PresetNameInput,
}

pub struct ServerTabState {
    pub config: ServerConfig,
    pub autocomplete: AutocompleteState,
    pub focus: ServerFocus,
    pub override_selected: usize,
    pub overrides: Vec<OverrideEntry>,
    pub editing_value: String,
    pub editing_param: Option<String>,
    pub status_message: Option<String>,
    pub stopping: bool,
    // Presets
    pub presets: Vec<String>,
    pub preset_selected: usize,
    pub preset_name_input: String,
    pub preset_action: PresetAction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum PresetAction {
    Load,
    Save,
    Delete,
}

#[derive(Clone)]
pub struct OverrideEntry {
    pub key: String,
    pub value: String,
    pub description: String,
}

impl ServerTabState {
    pub fn new() -> Self {
        let items: Vec<AutocompleteItem> = PARAMS
            .iter()
            .map(|p| AutocompleteItem {
                key: p.key.to_string(),
                label: p.label.to_string(),
                description: p.description.to_string(),
            })
            .collect();

        Self {
            config: ServerConfig::default(),
            autocomplete: AutocompleteState::new(items),
            focus: ServerFocus::Overrides,
            override_selected: 0,
            overrides: Vec::new(),
            editing_value: String::new(),
            editing_param: None,
            status_message: None,
            stopping: false,
            presets: Vec::new(),
            preset_selected: 0,
            preset_name_input: String::new(),
            preset_action: PresetAction::Load,
        }
    }

    pub fn refresh_overrides(&mut self) {
        self.overrides = collect_overrides(&self.config);
    }

    pub fn load_presets(&mut self) {
        self.presets = list_presets().unwrap_or_default();
    }
}

impl Default for ServerTabState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Preset file helpers (reuse same format as lib.rs) ────────────────────────

fn list_presets() -> anyhow::Result<Vec<String>> {
    let dir = AppConfig::presets_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
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

fn save_preset(name: &str, config: &ServerConfig) -> anyhow::Result<()> {
    let dir = AppConfig::presets_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", name));
    let preset_config = ServerConfig {
        model_path: String::new(),
        mmproj_path: None,
        ..config.clone()
    };
    let content = serde_json::to_string_pretty(&preset_config)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn load_preset(name: &str) -> anyhow::Result<ServerConfig> {
    let dir = AppConfig::presets_dir()?;
    let path = dir.join(format!("{}.json", name));
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

fn delete_preset(name: &str) -> anyhow::Result<()> {
    let dir = AppConfig::presets_dir()?;
    let path = dir.join(format!("{}.json", name));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ── Overrides collection ─────────────────────────────────────────────────────

fn collect_overrides(config: &ServerConfig) -> Vec<OverrideEntry> {
    let defaults = ServerConfig::default();
    let mut overrides = Vec::new();

    macro_rules! check_field {
        ($field:ident, $key:expr, $desc:expr) => {
            if config.$field != defaults.$field {
                overrides.push(OverrideEntry {
                    key: $key.to_string(),
                    value: format!("{:?}", config.$field),
                    description: $desc.to_string(),
                });
            }
        };
    }

    if !config.model_path.is_empty() {
        let name = std::path::Path::new(&config.model_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&config.model_path);
        overrides.push(OverrideEntry {
            key: "model_path".to_string(),
            value: name.to_string(),
            description: "Model file".to_string(),
        });
    }

    check_field!(n_gpu_layers, "n_gpu_layers", "GPU layers (-1=all)");
    check_field!(n_ctx, "n_ctx", "Context size (0=auto)");
    check_field!(flash_attn, "flash_attn", "Flash attention");
    check_field!(temperature, "temperature", "Temperature");
    check_field!(top_k, "top_k", "Top-K");
    check_field!(min_p, "min_p", "Min-P");
    check_field!(top_p, "top_p", "Top-P");
    check_field!(host, "host", "Host");
    check_field!(port, "port", "Port");
    check_field!(n_batch, "n_batch", "Batch size");
    check_field!(n_ubatch, "n_ubatch", "Micro-batch size");
    check_field!(cache_type_k, "cache_type_k", "KV cache type K");
    check_field!(cache_type_v, "cache_type_v", "KV cache type V");
    check_field!(mlock, "mlock", "Lock memory");
    check_field!(no_mmap, "no_mmap", "Disable mmap");
    check_field!(parallel, "parallel", "Parallel slots");
    check_field!(n_predict, "n_predict", "Max tokens");
    check_field!(cont_batching, "cont_batching", "Continuous batching");

    if let Some(threads) = config.n_threads {
        overrides.push(OverrideEntry {
            key: "n_threads".to_string(),
            value: threads.to_string(),
            description: "CPU threads".to_string(),
        });
    }
    if let Some(seed) = config.seed {
        overrides.push(OverrideEntry {
            key: "seed".to_string(),
            value: seed.to_string(),
            description: "Random seed".to_string(),
        });
    }

    for (k, v) in &config.extra_params {
        if k == "__raw__" {
            if !v.is_empty() {
                overrides.push(OverrideEntry {
                    key: "__raw__".to_string(),
                    value: v.clone(),
                    description: "Raw CLI arguments".to_string(),
                });
            }
        } else {
            let desc = PARAMS
                .iter()
                .find(|p| p.key == k.as_str())
                .map(|p| p.description)
                .unwrap_or("Custom parameter");
            overrides.push(OverrideEntry {
                key: k.clone(),
                value: if v.is_empty() {
                    "(flag)".to_string()
                } else {
                    v.clone()
                },
                description: desc.to_string(),
            });
        }
    }

    overrides
}

// ── Server stop event ────────────────────────────────────────────────────────

pub fn stop_from_outside(app: &mut TuiApp) {
    stop_server_async(app);
}

pub fn on_server_stopped(app: &mut TuiApp) {
    app.server_tab.stopping = false;
    app.server = None;
    app.server_tab.status_message = Some("Server stopped.".to_string());
}

// ── Key handling ─────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    // Block input during stopping
    if app.server_tab.stopping {
        return Action::None;
    }

    match app.server_tab.focus {
        ServerFocus::Overrides => handle_overrides_key(app, key),
        ServerFocus::Search => handle_search_key(app, key),
        ServerFocus::ValueEdit => handle_value_edit_key(app, key),
        ServerFocus::PresetPicker => handle_preset_picker(app, key),
        ServerFocus::PresetNameInput => handle_preset_name(app, key),
    }
}

fn handle_overrides_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up => {
            if app.server_tab.override_selected > 0 {
                app.server_tab.override_selected -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.server_tab.overrides.len();
            if count == 0 || app.server_tab.override_selected >= count - 1 {
                // Move past overrides into search
                app.server_tab.focus = ServerFocus::Search;
                app.input_focused = true;
                app.server_tab.autocomplete.reset();
            } else {
                app.server_tab.override_selected += 1;
            }
        }
        KeyCode::Char('/') | KeyCode::Tab => {
            app.server_tab.focus = ServerFocus::Search;
            app.input_focused = true;
            app.server_tab.autocomplete.reset();
        }
        KeyCode::Enter => {
            if app.server.is_none() {
                start_server_action(app);
            }
        }
        KeyCode::Char('x') => {
            stop_server_async(app);
        }
        KeyCode::Char('L') | KeyCode::Char('l') => {
            app.server_tab.load_presets();
            app.server_tab.preset_action = PresetAction::Load;
            app.server_tab.preset_selected = 0;
            app.server_tab.focus = ServerFocus::PresetPicker;
        }
        KeyCode::Char('S') | KeyCode::Char('s') if !app.input_focused => {
            // Can't use 's' as global tab switch is handled after Unhandled,
            // but here we're in the overrides focus so we consume it
            app.server_tab.preset_name_input.clear();
            app.server_tab.focus = ServerFocus::PresetNameInput;
            app.server_tab.preset_action = PresetAction::Save;
            app.input_focused = true;
        }
        KeyCode::Esc => {
            app.active_tab = Tab::Dashboard;
        }
        KeyCode::Backspace | KeyCode::Delete => {
            if let Some(entry) = app
                .server_tab
                .overrides
                .get(app.server_tab.override_selected)
            {
                let key = entry.key.clone();
                remove_override(&mut app.server_tab.config, &key);
                app.server_tab.refresh_overrides();
                let count = app.server_tab.overrides.len();
                if app.server_tab.override_selected > 0
                    && app.server_tab.override_selected >= count
                {
                    app.server_tab.override_selected = count.saturating_sub(1);
                }
            }
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

fn handle_search_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    use tui_input::backend::crossterm::EventHandler;
    match key.code {
        KeyCode::Esc => {
            app.server_tab.focus = ServerFocus::Overrides;
            app.input_focused = false;
            app.server_tab.autocomplete.open = false;
        }
        KeyCode::Enter => {
            if let Some(item) = app.server_tab.autocomplete.selected_item() {
                let param_key = item.key.clone();
                app.server_tab.editing_param = Some(param_key);
                app.server_tab.editing_value.clear();
                app.server_tab.focus = ServerFocus::ValueEdit;
                app.server_tab.autocomplete.open = false;
            }
        }
        KeyCode::Up => {
            if app.server_tab.autocomplete.open {
                app.server_tab.autocomplete.move_up();
            } else {
                // Move back to overrides
                app.server_tab.focus = ServerFocus::Overrides;
                app.input_focused = false;
            }
        }
        KeyCode::Down => {
            app.server_tab.autocomplete.move_down();
        }
        _ => {
            app.server_tab
                .autocomplete
                .input
                .handle_event(&crossterm::event::Event::Key(key));
            app.server_tab.autocomplete.update_filter();
        }
    }
    Action::None
}

fn handle_value_edit_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.server_tab.focus = ServerFocus::Search;
            app.server_tab.editing_param = None;
        }
        KeyCode::Enter => {
            if let Some(ref param_key) = app.server_tab.editing_param.clone() {
                let value = app.server_tab.editing_value.clone();
                apply_override(&mut app.server_tab.config, param_key, &value);
                app.server_tab.refresh_overrides();
                app.server_tab.focus = ServerFocus::Search;
                app.server_tab.editing_param = None;
                app.server_tab.autocomplete.reset();
            }
        }
        KeyCode::Backspace => {
            app.server_tab.editing_value.pop();
        }
        KeyCode::Char(c) => {
            app.server_tab.editing_value.push(c);
        }
        _ => {}
    }
    Action::None
}

fn handle_preset_picker(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.server_tab.focus = ServerFocus::Overrides;
        }
        KeyCode::Up => {
            if app.server_tab.preset_selected > 0 {
                app.server_tab.preset_selected -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.server_tab.presets.len();
            if count > 0 && app.server_tab.preset_selected < count - 1 {
                app.server_tab.preset_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(name) = app.server_tab.presets.get(app.server_tab.preset_selected).cloned()
            {
                match app.server_tab.preset_action {
                    PresetAction::Load => {
                        let model_path = app.server_tab.config.model_path.clone();
                        let mmproj = app.server_tab.config.mmproj_path.clone();
                        match load_preset(&name) {
                            Ok(mut cfg) => {
                                cfg.model_path = model_path;
                                cfg.mmproj_path = mmproj;
                                app.server_tab.config = cfg;
                                app.server_tab.refresh_overrides();
                                app.server_tab.status_message =
                                    Some(format!("Loaded preset: {}", name));
                            }
                            Err(e) => {
                                app.server_tab.status_message =
                                    Some(format!("Error loading: {}", e));
                            }
                        }
                    }
                    PresetAction::Delete => {
                        match delete_preset(&name) {
                            Ok(_) => {
                                app.server_tab.status_message =
                                    Some(format!("Deleted preset: {}", name));
                                app.server_tab.load_presets();
                            }
                            Err(e) => {
                                app.server_tab.status_message =
                                    Some(format!("Error deleting: {}", e));
                            }
                        }
                    }
                    _ => {}
                }
                app.server_tab.focus = ServerFocus::Overrides;
            }
        }
        _ => {}
    }
    Action::None
}

fn handle_preset_name(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.server_tab.focus = ServerFocus::Overrides;
            app.input_focused = false;
        }
        KeyCode::Enter => {
            let name = app.server_tab.preset_name_input.trim().to_string();
            if !name.is_empty() {
                match save_preset(&name, &app.server_tab.config) {
                    Ok(_) => {
                        app.server_tab.status_message =
                            Some(format!("Saved preset: {}", name));
                    }
                    Err(e) => {
                        app.server_tab.status_message =
                            Some(format!("Error saving: {}", e));
                    }
                }
            }
            app.server_tab.focus = ServerFocus::Overrides;
            app.input_focused = false;
        }
        KeyCode::Backspace => {
            app.server_tab.preset_name_input.pop();
        }
        KeyCode::Char(c) => {
            app.server_tab.preset_name_input.push(c);
        }
        _ => {}
    }
    Action::None
}

// ── Apply / remove overrides ─────────────────────────────────────────────────

fn apply_override(config: &mut ServerConfig, key: &str, value: &str) {
    match key {
        "n_gpu_layers" => {
            if let Ok(v) = value.parse() {
                config.n_gpu_layers = v;
            }
        }
        "n_ctx" => {
            if let Ok(v) = value.parse() {
                config.n_ctx = v;
            }
        }
        "flash_attn" => config.flash_attn = value.to_string(),
        "temperature" => {
            if let Ok(v) = value.parse() {
                config.temperature = v;
            }
        }
        "top_k" => {
            if let Ok(v) = value.parse() {
                config.top_k = v;
            }
        }
        "min_p" => {
            if let Ok(v) = value.parse() {
                config.min_p = v;
            }
        }
        "top_p" => {
            if let Ok(v) = value.parse() {
                config.top_p = v;
            }
        }
        "host" => config.host = value.to_string(),
        "port" => {
            if let Ok(v) = value.parse() {
                config.port = v;
            }
        }
        "n_batch" => {
            if let Ok(v) = value.parse() {
                config.n_batch = v;
            }
        }
        "n_ubatch" => {
            if let Ok(v) = value.parse() {
                config.n_ubatch = v;
            }
        }
        "cache_type_k" => config.cache_type_k = value.to_string(),
        "cache_type_v" => config.cache_type_v = value.to_string(),
        "mlock" => config.mlock = value == "true" || value == "on" || value == "1",
        "no_mmap" => config.no_mmap = value == "true" || value == "on" || value == "1",
        "parallel" => {
            if let Ok(v) = value.parse() {
                config.parallel = v;
            }
        }
        "n_predict" => {
            if let Ok(v) = value.parse() {
                config.n_predict = v;
            }
        }
        "n_threads" => config.n_threads = value.parse().ok(),
        "seed" => {
            config.seed = if value == "-1" || value.is_empty() {
                None
            } else {
                value.parse().ok()
            }
        }
        "__raw__" => {
            if value.is_empty() {
                config.extra_params.remove("__raw__");
            } else {
                config
                    .extra_params
                    .insert("__raw__".to_string(), value.to_string());
            }
        }
        _ => {
            if value.is_empty() {
                config.extra_params.insert(key.to_string(), String::new());
            } else {
                config
                    .extra_params
                    .insert(key.to_string(), value.to_string());
            }
        }
    }
}

fn remove_override(config: &mut ServerConfig, key: &str) {
    let defaults = ServerConfig::default();
    match key {
        "model_path" => config.model_path = String::new(),
        "n_gpu_layers" => config.n_gpu_layers = defaults.n_gpu_layers,
        "n_ctx" => config.n_ctx = defaults.n_ctx,
        "flash_attn" => config.flash_attn = defaults.flash_attn.clone(),
        "temperature" => config.temperature = defaults.temperature,
        "top_k" => config.top_k = defaults.top_k,
        "min_p" => config.min_p = defaults.min_p,
        "top_p" => config.top_p = defaults.top_p,
        "host" => config.host = defaults.host.clone(),
        "port" => config.port = defaults.port,
        "n_batch" => config.n_batch = defaults.n_batch,
        "n_ubatch" => config.n_ubatch = defaults.n_ubatch,
        "cache_type_k" => config.cache_type_k = defaults.cache_type_k.clone(),
        "cache_type_v" => config.cache_type_v = defaults.cache_type_v.clone(),
        "mlock" => config.mlock = defaults.mlock,
        "no_mmap" => config.no_mmap = defaults.no_mmap,
        "parallel" => config.parallel = defaults.parallel,
        "n_predict" => config.n_predict = defaults.n_predict,
        "cont_batching" => config.cont_batching = defaults.cont_batching,
        "n_threads" => config.n_threads = None,
        "seed" => config.seed = None,
        _ => {
            config.extra_params.remove(key);
        }
    }
}

// ── Server actions ───────────────────────────────────────────────────────────

fn start_server_action(app: &mut TuiApp) {
    if app.server_tab.config.model_path.is_empty() {
        app.server_tab.status_message = Some("No model selected.".to_string());
        return;
    }

    let config = app.config.clone();
    let runtime_info = match catapult_lib::runtime::get_runtime_info(&config) {
        Ok(ri) => ri,
        Err(e) => {
            app.server_tab.status_message = Some(format!("Runtime error: {}", e));
            return;
        }
    };

    let server_binary = match runtime_info.server_binary {
        Some(b) => b,
        None => {
            app.server_tab.status_message = Some("No server binary found.".to_string());
            return;
        }
    };

    match server_ctl::start_server(&server_binary, &app.server_tab.config, &app.config) {
        Ok(pid) => {
            app.server_tab.status_message = Some(format!("Server started (PID {})", pid));
        }
        Err(e) => {
            app.server_tab.status_message = Some(format!("Failed to start: {}", e));
        }
    }
}

fn stop_server_async(app: &mut TuiApp) {
    if let Some(ref ds) = app.server {
        let pid = ds.pid;
        app.server_tab.stopping = true;
        app.server_tab.status_message = Some(format!("Stopping server (PID {})...", pid));

        let tx = app.event_tx.clone();
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                server_ctl::stop_server(pid)
            })
            .await;
            let _ = tx.send(TuiEvent::ServerStopped);
        });
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    app.server_tab.refresh_overrides();

    // Stopping modal
    if app.server_tab.stopping {
        let lines = vec![
            Line::raw(""),
            Line::raw(""),
            Line::from(Span::styled(
                "  Stopping server...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  Sending SIGTERM, waiting for graceful shutdown (up to 30s)",
                Style::default().fg(Color::Cyan),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Preset picker overlay
    if app.server_tab.focus == ServerFocus::PresetPicker {
        render_preset_picker(app, area, frame);
        return;
    }

    // Preset name input overlay
    if app.server_tab.focus == ServerFocus::PresetNameInput {
        render_preset_name_input(app, area, frame);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3),  // Model + server status
        Constraint::Length(1), // Separator
        Constraint::Min(6),   // Overrides + search (main area)
        Constraint::Length(2), // Controls
        Constraint::Length(1), // Status message
    ])
    .split(area);

    render_model_and_status(app, chunks[0], frame);
    render_separator("Parameters", chunks[1], frame);
    render_params_area(app, chunks[2], frame);
    render_controls(app, chunks[3], frame);
    render_status(app, chunks[4], frame);
}

fn render_model_and_status(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let model_name = if app.server_tab.config.model_path.is_empty() {
        "(none — select from Models tab)"
    } else {
        std::path::Path::new(&app.server_tab.config.model_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&app.server_tab.config.model_path)
    };

    let server_status = match &app.server {
        Some(ds) => Span::styled(
            format!("  Running :{} (PID {})", ds.port, ds.pid),
            Style::default().fg(Color::Green),
        ),
        None => Span::styled("  Stopped", Style::default().fg(Color::Red)),
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(" Model  ", Style::default().fg(Color::Blue)),
            Span::styled(model_name, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" Server ", Style::default().fg(Color::Blue)),
            server_status,
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_separator(title: &str, area: Rect, frame: &mut Frame) {
    let w = area.width as usize;
    let title_len = title.len() + 2;
    let dashes = w.saturating_sub(title_len);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "─".repeat(dashes),
            Style::default().fg(Color::Blue),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_params_area(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let override_count = app.server_tab.overrides.len();
    // Reserve at least 3 lines for the search area
    let max_overrides = (area.height as usize).saturating_sub(4);
    let visible_overrides = override_count.min(max_overrides);

    let chunks = Layout::vertical([
        Constraint::Length(visible_overrides.max(1) as u16), // Overrides
        Constraint::Length(1),                                // Search label + input
        Constraint::Min(0),                                  // Dropdown
    ])
    .split(area);

    // Overrides list with scroll
    if app.server_tab.overrides.is_empty() {
        let empty = Line::from(Span::styled(
            "  (all defaults — type below or press Down/Tab to add parameters)",
            Style::default().fg(Color::Cyan),
        ));
        frame.render_widget(Paragraph::new(empty), chunks[0]);
    } else {
        let scroll = if visible_overrides > 0 && app.server_tab.override_selected >= visible_overrides {
            app.server_tab.override_selected - visible_overrides + 1
        } else {
            0
        };

        let lines: Vec<Line> = app
            .server_tab
            .overrides
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_overrides)
            .map(|(i, ov)| {
                let is_selected = app.server_tab.focus == ServerFocus::Overrides
                    && i == app.server_tab.override_selected;
                let marker = if is_selected { " > " } else { "   " };
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let desc_style = if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Cyan)
                };

                Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(format!("{:<18} = {:<20}", ov.key, ov.value), style),
                    Span::styled(format!("({})", ov.description), desc_style),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), chunks[0]);
    }

    // Search / value edit
    if app.server_tab.focus == ServerFocus::ValueEdit {
        if let Some(ref param_key) = app.server_tab.editing_param {
            let line = Line::from(vec![
                Span::styled(
                    format!("  {} = ", param_key),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    &app.server_tab.editing_value,
                    Style::default().fg(Color::White),
                ),
                Span::styled("_", Style::default().fg(Color::White).bg(Color::White)),
            ]);
            frame.render_widget(Paragraph::new(line), chunks[1]);
        }
    } else {
        let focused = app.server_tab.focus == ServerFocus::Search;
        let prefix = if focused { "  > " } else { "  / " };
        let value = app.server_tab.autocomplete.input.value();
        let cursor = if focused { "_" } else { "" };
        let hint = if value.is_empty() && focused {
            " type to search parameters..."
        } else if value.is_empty() {
            " (press Down or / to add parameters)"
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Cyan)),
            Span::styled(value, Style::default().fg(Color::White)),
            Span::styled(cursor, Style::default().fg(Color::White)),
            Span::styled(hint, Style::default().fg(Color::Blue)),
        ]);
        frame.render_widget(Paragraph::new(line), chunks[1]);

        // Dropdown
        if focused && app.server_tab.autocomplete.open {
            autocomplete::render_autocomplete_dropdown(
                &app.server_tab.autocomplete,
                chunks[2],
                frame.buffer_mut(),
            );
        }
    }
}

fn render_controls(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let is_running = app.server.is_some();

    let mut spans = vec![Span::raw("  ")];

    if is_running {
        spans.push(Span::styled("[x]Stop", Style::default().fg(Color::Red)));
    } else {
        spans.push(Span::styled("[Enter]Start", Style::default().fg(Color::Green)));
    }

    spans.push(Span::styled(
        "  [l]Load preset  [s]Save preset  [Del]Remove param  [Esc]Back",
        Style::default().fg(Color::Cyan),
    ));

    let lines = vec![Line::raw(""), Line::from(spans)];
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_status(app: &TuiApp, area: Rect, frame: &mut Frame) {
    if let Some(ref msg) = app.server_tab.status_message {
        let style = if msg.contains("error") || msg.contains("Failed") || msg.contains("Error") {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Yellow)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!("  {}", msg), style))),
            area,
        );
    }
}

fn render_preset_picker(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let action_label = match app.server_tab.preset_action {
        PresetAction::Load => "Load Preset",
        PresetAction::Delete => "Delete Preset",
        _ => "Preset",
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {} ", action_label),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];

    if app.server_tab.presets.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No presets saved yet.",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        for (i, name) in app.server_tab.presets.iter().enumerate() {
            let is_selected = i == app.server_tab.preset_selected;
            let marker = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(format!("{}{}", marker, name), style)));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  [Enter]Select  [Esc]Cancel",
        Style::default().fg(Color::Cyan),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_preset_name_input(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let lines = vec![
        Line::from(Span::styled(
            " Save Preset ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Name: ", Style::default().fg(Color::Blue)),
            Span::styled(
                &app.server_tab.preset_name_input,
                Style::default().fg(Color::White),
            ),
            Span::styled("_", Style::default().fg(Color::White).bg(Color::White)),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  [Enter]Save  [Esc]Cancel",
            Style::default().fg(Color::Cyan),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

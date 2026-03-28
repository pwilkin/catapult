use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use tui_input::backend::crossterm::EventHandler;

use catapult_lib::huggingface::{HfFile, HfModel};
use catapult_lib::models::{ModelInfo, RecommendedModel};

use crate::tui::app::{Action, Tab, TuiApp};
use crate::tui::event::TuiEvent;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelsMode {
    Installed,
    Recommended,
    Search,
    Directories,
}

pub struct ModelsTabState {
    pub models: Vec<ModelInfo>,
    pub recommended: Vec<RecommendedModel>,
    pub selected: usize,
    pub loaded: bool,
    pub mode: ModelsMode,
    // Filter
    pub filter_input: tui_input::Input,
    pub filter_active: bool,
    pub confirm_delete: bool,
    // HF search
    pub search_input: tui_input::Input,
    pub search_results: Vec<HfModel>,
    pub search_selected: usize,
    pub search_loading: bool,
    pub search_error: Option<String>,
    pub expanded_repo: Option<usize>,
    pub file_selected: usize,
    pub files_loading: bool,
    // Directories
    pub dir_selected: usize,
    pub dir_input: tui_input::Input,
    pub dir_input_active: bool,
    pub dir_input_mode: DirInputMode,
    pub dir_confirm_remove: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DirInputMode {
    AddScan,
    SetDownload,
}

impl Default for ModelsTabState {
    fn default() -> Self {
        Self {
            models: Vec::new(),
            recommended: Vec::new(),
            selected: 0,
            loaded: false,
            mode: ModelsMode::Installed,
            filter_input: tui_input::Input::default(),
            filter_active: false,
            confirm_delete: false,
            search_input: tui_input::Input::default(),
            search_results: Vec::new(),
            search_selected: 0,
            search_loading: false,
            search_error: None,
            expanded_repo: None,
            file_selected: 0,
            files_loading: false,
            dir_selected: 0,
            dir_input: tui_input::Input::default(),
            dir_input_active: false,
            dir_input_mode: DirInputMode::AddScan,
            dir_confirm_remove: false,
        }
    }
}

impl ModelsTabState {
    fn filtered_models(&self) -> Vec<(usize, &ModelInfo)> {
        let filter = self.filter_input.value().to_lowercase();
        self.models
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if filter.is_empty() {
                    return true;
                }
                m.name.to_lowercase().contains(&filter)
                    || m.filename.to_lowercase().contains(&filter)
                    || m.quant
                        .as_ref()
                        .map(|q| q.to_lowercase().contains(&filter))
                        .unwrap_or(false)
            })
            .collect()
    }
}

// ── Event handlers from async tasks ──────────────────────────────────────────

pub fn on_search_results(app: &mut TuiApp, results: Result<Vec<HfModel>, String>) {
    app.models_tab.search_loading = false;
    match results {
        Ok(models) => {
            app.models_tab.search_results = models;
            app.models_tab.search_selected = 0;
            app.models_tab.search_error = None;
            app.models_tab.expanded_repo = None;
        }
        Err(e) => {
            app.models_tab.search_error = Some(e);
            app.models_tab.search_results.clear();
        }
    }
}

pub fn on_repo_files(app: &mut TuiApp, repo_id: &str, files: Result<Vec<HfFile>, String>) {
    app.models_tab.files_loading = false;
    match files {
        Ok(files) => {
            // Find the repo and update its files
            if let Some(model) = app
                .models_tab
                .search_results
                .iter_mut()
                .find(|m| m.repo_id == repo_id)
            {
                model.files = files;
            }
            app.models_tab.file_selected = 0;
        }
        Err(e) => {
            app.models_tab.search_error = Some(e);
        }
    }
}

// ── Key handling ─────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match app.models_tab.mode {
        ModelsMode::Installed => handle_installed(app, key),
        ModelsMode::Recommended => handle_recommended(app, key),
        ModelsMode::Search => handle_search(app, key),
        ModelsMode::Directories => handle_directories(app, key),
    }
}

fn handle_installed(app: &mut TuiApp, key: KeyEvent) -> Action {
    // Filter input mode
    if app.models_tab.filter_active {
        match key.code {
            KeyCode::Esc => {
                app.models_tab.filter_active = false;
                app.input_focused = false;
                app.models_tab.filter_input = tui_input::Input::default();
            }
            KeyCode::Enter => {
                app.models_tab.filter_active = false;
                app.input_focused = false;
            }
            _ => {
                app.models_tab
                    .filter_input
                    .handle_event(&crossterm::event::Event::Key(key));
                app.models_tab.selected = 0;
            }
        }
        return Action::None;
    }

    // Delete confirmation
    if app.models_tab.confirm_delete {
        match key.code {
            KeyCode::Char('y') => {
                let filtered = app.models_tab.filtered_models();
                if let Some(&(orig_idx, _)) = filtered.get(app.models_tab.selected) {
                    if let Some(model) = app.models_tab.models.get(orig_idx) {
                        let path = model.path.clone();
                        let _ = catapult_lib::models::delete_model(&path);
                        app.models_tab.loaded = false;
                    }
                }
                app.models_tab.confirm_delete = false;
            }
            _ => {
                app.models_tab.confirm_delete = false;
            }
        }
        return Action::None;
    }

    let filtered = app.models_tab.filtered_models();
    let filtered_count = filtered.len();

    match key.code {
        KeyCode::Up => {
            if app.models_tab.selected > 0 {
                app.models_tab.selected -= 1;
            }
        }
        KeyCode::Down => {
            if filtered_count > 0 && app.models_tab.selected < filtered_count - 1 {
                app.models_tab.selected += 1;
            }
        }
        KeyCode::Enter => {
            let filtered = app.models_tab.filtered_models();
            if let Some(&(_, model)) = filtered.get(app.models_tab.selected) {
                app.server_tab.config.model_path = model.path.display().to_string();
                if let Some(ref mmproj) = model.mmproj_path {
                    app.server_tab.config.mmproj_path = Some(mmproj.display().to_string());
                }
                app.server_tab.status_message = Some(format!("Model selected: {}", model.name));
            }
        }
        KeyCode::Char('/') => {
            app.models_tab.filter_active = true;
            app.input_focused = true;
        }
        KeyCode::Char('f') => {
            let filtered = app.models_tab.filtered_models();
            if let Some(&(orig_idx, _)) = filtered.get(app.models_tab.selected) {
                if let Some(model) = app.models_tab.models.get(orig_idx) {
                    let model_id = model.id.clone();
                    if let Some(pos) =
                        app.config.favorite_models.iter().position(|f| f == &model_id)
                    {
                        app.config.favorite_models.remove(pos);
                    } else {
                        app.config.favorite_models.push(model_id);
                    }
                    let _ = app.config.save();
                }
            }
        }
        KeyCode::Char('x') => {
            if !app.models_tab.models.is_empty() {
                app.models_tab.confirm_delete = true;
            }
        }
        KeyCode::Char('b') => {
            app.models_tab.mode = ModelsMode::Search;
            app.models_tab.search_input = tui_input::Input::default();
            app.models_tab.search_results.clear();
            app.models_tab.search_error = None;
            app.models_tab.search_selected = 0;
            app.models_tab.expanded_repo = None;
            app.input_focused = true;
        }
        KeyCode::Char('p') => {
            app.models_tab.mode = ModelsMode::Directories;
        }
        KeyCode::Char('e') => {
            app.models_tab.mode = ModelsMode::Recommended;
            if app.models_tab.recommended.is_empty() {
                let config = app.config.clone();
                app.models_tab.recommended =
                    catapult_lib::models::get_recommended_models(&config).unwrap_or_default();
            }
            app.models_tab.selected = 0;
        }
        KeyCode::Esc => {
            if !app.models_tab.filter_input.value().is_empty() {
                app.models_tab.filter_input = tui_input::Input::default();
                app.models_tab.selected = 0;
            } else {
                // Cascade: Esc from top-level goes back to dashboard
                app.active_tab = Tab::Dashboard;
            }
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

fn handle_recommended(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.models_tab.mode = ModelsMode::Installed;
            app.models_tab.selected = 0;
        }
        KeyCode::Up => {
            if app.models_tab.selected > 0 {
                app.models_tab.selected -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.models_tab.recommended.len();
            if count > 0 && app.models_tab.selected < count - 1 {
                app.models_tab.selected += 1;
            }
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

fn handle_search(app: &mut TuiApp, key: KeyEvent) -> Action {
    // If we're in the repo file list
    if let Some(repo_idx) = app.models_tab.expanded_repo {
        if !app.input_focused {
            let file_count = app
                .models_tab
                .search_results
                .get(repo_idx)
                .map(|m| m.files.len())
                .unwrap_or(0);

            match key.code {
                KeyCode::Esc => {
                    app.models_tab.expanded_repo = None;
                    app.models_tab.file_selected = 0;
                }
                KeyCode::Up => {
                    if app.models_tab.file_selected > 0 {
                        app.models_tab.file_selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if file_count > 0 && app.models_tab.file_selected < file_count - 1 {
                        app.models_tab.file_selected += 1;
                    }
                }
                KeyCode::Enter => {
                    // Download the selected file
                    let download_info = app
                        .models_tab
                        .search_results
                        .get(repo_idx)
                        .and_then(|model| {
                            model
                                .files
                                .get(app.models_tab.file_selected)
                                .map(|file| (model.repo_id.clone(), file.clone()))
                        });
                    if let Some((repo_id, file)) = download_info {
                        if !app.downloads.contains_key(&file.filename) {
                            start_file_download(app, &repo_id, &file);
                        }
                    }
                }
                _ => {}
            }
            return Action::None;
        }
    }

    // If we're in the results list (not typing)
    if !app.input_focused && !app.models_tab.search_results.is_empty() {
        match key.code {
            KeyCode::Esc => {
                // Back to installed
                app.models_tab.mode = ModelsMode::Installed;
                app.models_tab.selected = 0;
            }
            KeyCode::Up => {
                if app.models_tab.search_selected > 0 {
                    app.models_tab.search_selected -= 1;
                }
            }
            KeyCode::Down => {
                if app.models_tab.search_selected < app.models_tab.search_results.len() - 1 {
                    app.models_tab.search_selected += 1;
                }
            }
            KeyCode::Enter => {
                // Expand repo to see files
                let idx = app.models_tab.search_selected;
                if let Some(model) = app.models_tab.search_results.get(idx) {
                    if model.files.is_empty() && !app.models_tab.files_loading {
                        // Fetch files
                        app.models_tab.files_loading = true;
                        app.models_tab.expanded_repo = Some(idx);
                        let repo_id = model.repo_id.clone();
                        let client = app.http_client.clone();
                        let tx = app.event_tx.clone();
                        tokio::spawn(async move {
                            let result =
                                catapult_lib::huggingface::get_repo_files(&client, &repo_id).await;
                            let _ = tx.send(TuiEvent::HfRepoFiles(
                                repo_id,
                                result.map_err(|e| e.to_string()),
                            ));
                        });
                    } else {
                        app.models_tab.expanded_repo = Some(idx);
                        app.models_tab.file_selected = 0;
                    }
                }
            }
            KeyCode::Char('/') => {
                app.input_focused = true;
            }
            _ => return Action::Unhandled,
        }
        return Action::None;
    }

    // Input mode
    match key.code {
        KeyCode::Esc => {
            if app.models_tab.search_results.is_empty() {
                // Nothing searched yet — go back
                app.models_tab.mode = ModelsMode::Installed;
                app.models_tab.selected = 0;
            }
            app.input_focused = false;
        }
        KeyCode::Enter => {
            let query = app.models_tab.search_input.value().to_string();
            if !query.is_empty() && !app.models_tab.search_loading {
                app.models_tab.search_loading = true;
                app.models_tab.search_error = None;
                app.models_tab.search_results.clear();
                app.models_tab.expanded_repo = None;
                app.input_focused = false;

                let client = app.http_client.clone();
                let tx = app.event_tx.clone();
                tokio::spawn(async move {
                    let result =
                        catapult_lib::huggingface::search_models(&client, &query, None).await;
                    let _ = tx.send(TuiEvent::HfSearchResults(
                        result.map_err(|e| e.to_string()),
                    ));
                });
            }
        }
        _ => {
            app.models_tab
                .search_input
                .handle_event(&crossterm::event::Event::Key(key));
        }
    }
    Action::None
}

fn handle_directories(app: &mut TuiApp, key: KeyEvent) -> Action {
    // Path input mode
    if app.models_tab.dir_input_active {
        match key.code {
            KeyCode::Esc => {
                app.models_tab.dir_input_active = false;
                app.input_focused = false;
                app.models_tab.dir_input = tui_input::Input::default();
            }
            KeyCode::Enter => {
                let path_str = app.models_tab.dir_input.value().to_string();
                if !path_str.is_empty() {
                    let path = std::path::PathBuf::from(&path_str);
                    match app.models_tab.dir_input_mode {
                        DirInputMode::AddScan => {
                            if path.is_dir() {
                                if !app.config.model_dirs.contains(&path) {
                                    app.config.model_dirs.push(path);
                                    let _ = app.config.save();
                                    app.models_tab.loaded = false;
                                }
                            }
                        }
                        DirInputMode::SetDownload => {
                            if path.is_dir() {
                                app.config.download_dir = Some(path);
                                let _ = app.config.save();
                            }
                        }
                    }
                }
                app.models_tab.dir_input_active = false;
                app.input_focused = false;
                app.models_tab.dir_input = tui_input::Input::default();
            }
            _ => {
                app.models_tab
                    .dir_input
                    .handle_event(&crossterm::event::Event::Key(key));
            }
        }
        return Action::None;
    }

    // Remove confirmation
    if app.models_tab.dir_confirm_remove {
        match key.code {
            KeyCode::Char('y') => {
                if app.models_tab.dir_selected > 0 {
                    let idx = app.models_tab.dir_selected - 1; // -1 because 0 is download dir
                    if idx < app.config.model_dirs.len() {
                        app.config.model_dirs.remove(idx);
                        let _ = app.config.save();
                        app.models_tab.loaded = false;
                        if app.models_tab.dir_selected > 0 {
                            app.models_tab.dir_selected -= 1;
                        }
                    }
                }
                app.models_tab.dir_confirm_remove = false;
            }
            _ => {
                app.models_tab.dir_confirm_remove = false;
            }
        }
        return Action::None;
    }

    // Normal navigation
    let dir_count = 1 + app.config.model_dirs.len(); // download dir + scan dirs
    match key.code {
        KeyCode::Up => {
            if app.models_tab.dir_selected > 0 {
                app.models_tab.dir_selected -= 1;
            }
        }
        KeyCode::Down => {
            if app.models_tab.dir_selected < dir_count - 1 {
                app.models_tab.dir_selected += 1;
            }
        }
        KeyCode::Char('a') => {
            // Add new scan directory
            app.models_tab.dir_input_mode = DirInputMode::AddScan;
            app.models_tab.dir_input = tui_input::Input::default();
            app.models_tab.dir_input_active = true;
            app.input_focused = true;
        }
        KeyCode::Char('e') => {
            // Edit: set download dir if row 0 selected, otherwise no-op
            if app.models_tab.dir_selected == 0 {
                app.models_tab.dir_input_mode = DirInputMode::SetDownload;
                // Pre-fill with current value
                let current = app
                    .config
                    .download_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                app.models_tab.dir_input = tui_input::Input::default().with_value(current);
                app.models_tab.dir_input_active = true;
                app.input_focused = true;
            }
        }
        KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
            // Remove selected scan directory (not download dir)
            if app.models_tab.dir_selected > 0 {
                app.models_tab.dir_confirm_remove = true;
            }
        }
        KeyCode::Esc => {
            app.models_tab.mode = ModelsMode::Installed;
            app.models_tab.selected = 0;
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

// ── Download ─────────────────────────────────────────────────────────────────

fn start_file_download(app: &mut TuiApp, repo_id: &str, file: &HfFile) {
    let filename = file.filename.clone();

    // Check if already downloading
    if app.downloads.contains_key(&filename) {
        return;
    }

    let client = app.http_client.clone();
    let repo_id = repo_id.to_string();
    let file_clone = file.clone();
    let size_bytes = file.size_bytes;
    let config = app.config.clone();
    let tx = app.event_tx.clone();
    let dl_id = filename.clone();

    let handle = tokio::spawn(async move {
        let tx2 = tx.clone();
        let dl_id2 = dl_id.clone();
        let last_send = std::sync::Mutex::new(std::time::Instant::now());
        let result = catapult_lib::models::download_model(
            &client,
            &repo_id,
            &file_clone,
            &config,
            move |progress| {
                let mut last = last_send.lock().unwrap();
                let now = std::time::Instant::now();
                if now.duration_since(*last) >= std::time::Duration::from_millis(250) {
                    *last = now;
                    let _ = tx2.send(TuiEvent::DownloadProgress(progress));
                }
            },
        )
        .await;

        let (status, percent) = match &result {
            Ok(_) => ("complete".to_string(), 100.0),
            Err(e) => (format!("error: {}", e), 0.0),
        };
        let _ = tx.send(TuiEvent::DownloadProgress(
            catapult_lib::runtime::DownloadProgress {
                id: dl_id2,
                bytes_downloaded: 0,
                total_bytes: 0,
                percent,
                status,
            },
        ));
    });

    app.downloads.insert(
        filename.clone(),
        crate::tui::app::ActiveDownload {
            progress: catapult_lib::runtime::DownloadProgress {
                id: filename,
                bytes_downloaded: 0,
                total_bytes: size_bytes,
                percent: 0.0,
                status: "starting".to_string(),
            },
            task_handle: handle,
            dismiss_countdown: None,
        },
    );
}

// ── Rendering ────────────────────────────────────────────────────────────────

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    if !app.models_tab.loaded {
        let config = app.config.clone();
        app.models_tab.models =
            catapult_lib::models::list_installed_models(&config).unwrap_or_default();
        let favs = app.config.favorite_models.clone();
        app.models_tab.models.sort_by(|a, b| {
            let a_fav = favs.contains(&a.id);
            let b_fav = favs.contains(&b.id);
            b_fav.cmp(&a_fav).then_with(|| a.name.cmp(&b.name))
        });
        app.models_tab.loaded = true;
    }

    match app.models_tab.mode {
        ModelsMode::Installed => render_installed(app, area, frame),
        ModelsMode::Recommended => render_recommended(app, area, frame),
        ModelsMode::Search => render_search(app, area, frame),
        ModelsMode::Directories => render_directories(app, area, frame),
    }
}

fn render_installed(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(area);

    let filter_text = app.models_tab.filter_input.value();
    let filter_display = if app.models_tab.filter_active {
        Span::styled(
            format!(" /: {}_ ", filter_text),
            Style::default().fg(Color::Yellow),
        )
    } else if !filter_text.is_empty() {
        Span::styled(
            format!(" filter: {} ", filter_text),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::raw("")
    };

    let filtered = app.models_tab.filtered_models();
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " Installed Models ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            filter_display,
        ]),
        Line::from(Span::styled(
            format!(
                "  {} models{}",
                filtered.len(),
                if filter_text.is_empty() {
                    String::new()
                } else {
                    format!(" (of {} total)", app.models_tab.models.len())
                }
            ),
            Style::default().fg(Color::Blue),
        )),
    ]);
    frame.render_widget(header, chunks[0]);

    // Column header
    let col_header = Line::from(vec![
        Span::styled("      ", Style::default().fg(Color::Blue)),
        Span::styled(
            format!("{:<30} {:>8}  {:<8} {:>5}  {:>5}", "Name", "Size", "Quant", "Param", "Ctx"),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ),
    ]);

    let visible = chunks[1].height as usize;
    let scroll = if visible > 1 && app.models_tab.selected >= visible - 1 {
        app.models_tab.selected - (visible - 2)
    } else {
        0
    };

    let mut lines = vec![col_header];
    for (i, &(_, m)) in filtered.iter().enumerate().skip(scroll).take(visible.saturating_sub(1)) {
        let is_fav = app.config.favorite_models.contains(&m.id);
        let is_selected = i == app.models_tab.selected;
        let marker = if is_selected { " > " } else { "   " };
        let star = if is_fav { "* " } else { "  " };
        let size = format_size(m.size_bytes);
        let quant = m.quant.clone().unwrap_or_default();
        let params = m.params_b.clone().unwrap_or_default();
        let ctx = m
            .context_length
            .map(|c| format!("{}K", c / 1024))
            .unwrap_or_default();

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let star_style = if is_selected {
            style
        } else {
            Style::default().fg(Color::Yellow)
        };

        let name_width = 30;
        let name = if m.name.len() > name_width {
            format!("{}~", &m.name[..name_width - 1])
        } else {
            format!("{:<width$}", m.name, width = name_width)
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(star, star_style),
            Span::styled(name, style),
            Span::styled(format!(" {:>8}", size), style),
            Span::styled(format!("  {:<8}", quant), style),
            Span::styled(format!(" {:>5}", params), style),
            Span::styled(format!("  {:>5}", ctx), style),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), chunks[1]);

    let footer_line = if app.models_tab.confirm_delete {
        let name = app
            .models_tab
            .filtered_models()
            .get(app.models_tab.selected)
            .map(|&(_, m)| m.name.as_str())
            .unwrap_or("?");
        Line::from(vec![
            Span::styled(
                format!(" Delete {}? ", name),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[y]es  [any key]cancel", Style::default().fg(Color::Cyan)),
        ])
    } else {
        Line::from(Span::styled(
            " [/]Filter  [Enter]Select  [f]Fav  [x]Del  [e]Recommended  [b]Browse HF  [p]Dirs  [Esc]Back",
            Style::default().fg(Color::Cyan),
        ))
    };
    frame.render_widget(Paragraph::new(vec![Line::raw(""), footer_line]), chunks[2]);
}

fn render_recommended(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(area);

    let header = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            " Recommended Models ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} models", app.models_tab.recommended.len()),
            Style::default().fg(Color::Blue),
        ),
    ])]);
    frame.render_widget(header, chunks[0]);

    let visible = chunks[1].height as usize;
    let scroll = if visible > 1 && app.models_tab.selected >= visible - 1 {
        app.models_tab.selected - (visible - 2)
    } else {
        0
    };

    let col_header = Line::from(Span::styled(
        format!("   {:<30} {:>6}  {:<8} {:>10} {:>9}", "Name", "Params", "Quant", "Size", "Status"),
        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
    ));

    let mut lines = vec![col_header];
    for (i, m) in app
        .models_tab
        .recommended
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible.saturating_sub(1))
    {
        let is_selected = i == app.models_tab.selected;
        let marker = if is_selected { " > " } else { "   " };
        let installed = if m.installed { "installed" } else { "" };
        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let name_width = 30;
        let name = if m.name.len() > name_width {
            format!("{}~", &m.name[..name_width - 1])
        } else {
            format!("{:<width$}", m.name, width = name_width)
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(name, style),
            Span::styled(format!(" {:>4}B", m.params_b), style),
            Span::styled(format!("  {:<8}", m.quant), style),
            Span::styled(format!(" ~{:>6} MB", m.estimated_size_mb), style),
            Span::styled(
                format!(" {:>9}", installed),
                if is_selected {
                    style
                } else if m.installed {
                    Style::default().fg(Color::Green)
                } else {
                    style
                },
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), chunks[1]);

    let footer = Paragraph::new(vec![
        Line::raw(""),
        Line::from(Span::styled(
            " [Esc]Back to installed",
            Style::default().fg(Color::Cyan),
        )),
    ]);
    frame.render_widget(footer, chunks[2]);
}

fn render_search(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(2), // Header
        Constraint::Length(2), // Search input
        Constraint::Min(4),   // Results
        Constraint::Length(2), // Footer
    ])
    .split(area);

    // Header
    let header = Paragraph::new(vec![Line::from(Span::styled(
        " Browse HuggingFace ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))]);
    frame.render_widget(header, chunks[0]);

    // Search input
    let value = app.models_tab.search_input.value();
    let cursor = if app.input_focused { "_" } else { "" };
    let input_line = Line::from(vec![
        Span::styled("  Search: ", Style::default().fg(Color::Blue)),
        Span::styled(
            format!("{}{}", value, cursor),
            Style::default().fg(Color::White),
        ),
        if app.models_tab.search_loading {
            Span::styled("  Searching...", Style::default().fg(Color::Yellow))
        } else {
            Span::raw("")
        },
    ]);
    frame.render_widget(Paragraph::new(vec![input_line]), chunks[1]);

    // Results area
    if let Some(ref err) = app.models_tab.search_error {
        let error = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            )),
        ]);
        frame.render_widget(error, chunks[2]);
    } else if app.models_tab.search_loading {
        let spinner_chars = ['|', '/', '-', '\\'];
        let tick = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 200) as usize;
        let spinner = spinner_chars[tick % spinner_chars.len()];
        let loading = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                format!("  {} Searching HuggingFace...", spinner),
                Style::default().fg(Color::Yellow),
            )),
        ]);
        frame.render_widget(loading, chunks[2]);
    } else if app.models_tab.search_results.is_empty() {
        let hint = if value.is_empty() {
            "  Type a model name and press Enter to search"
        } else {
            "  Press Enter to search"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled(hint, Style::default().fg(Color::Cyan))),
            ]),
            chunks[2],
        );
    } else if let Some(repo_idx) = app.models_tab.expanded_repo {
        // Show files for expanded repo
        render_repo_files(app, repo_idx, chunks[2], frame);
    } else {
        // Show repo list
        render_search_results(app, chunks[2], frame);
    }

    let footer_text = if app.models_tab.expanded_repo.is_some() {
        " [Enter]Download  [Esc]Back to results"
    } else if !app.models_tab.search_results.is_empty() && !app.input_focused {
        " [Enter]View files  [/]New search  [Esc]Back"
    } else {
        " [Enter]Search  [Esc]Back"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(footer_text, Style::default().fg(Color::Cyan))),
        ]),
        chunks[3],
    );
}

fn render_search_results(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let results = &app.models_tab.search_results;
    let count_line = Line::from(Span::styled(
        format!("  {} repositories found", results.len()),
        Style::default().fg(Color::Blue),
    ));

    let visible = (area.height as usize).saturating_sub(1);
    let scroll = if app.models_tab.search_selected >= visible {
        app.models_tab.search_selected - visible + 1
    } else {
        0
    };

    let mut lines = vec![count_line];
    for (i, model) in results.iter().enumerate().skip(scroll).take(visible) {
        let is_selected = i == app.models_tab.search_selected;
        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let marker = if is_selected { " > " } else { "   " };
        let files_hint = if model.files.is_empty() {
            String::new()
        } else {
            format!("  {} files", model.files.len())
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&model.name, style),
            Span::styled(
                format!("  by {}", model.author),
                if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Blue)
                },
            ),
            Span::styled(
                format!("  {} dl{}", format_downloads(model.downloads), files_hint),
                if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Cyan)
                },
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_repo_files(app: &TuiApp, repo_idx: usize, area: Rect, frame: &mut Frame) {
    let model = match app.models_tab.search_results.get(repo_idx) {
        Some(m) => m,
        None => return,
    };

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(&model.repo_id, Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("  {} files", model.files.len()),
            Style::default().fg(Color::Blue),
        ),
    ])];

    if app.models_tab.files_loading {
        lines.push(Line::from(Span::styled(
            "  Loading files...",
            Style::default().fg(Color::Yellow),
        )));
    } else if model.files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No GGUF files found",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        let visible = (area.height as usize).saturating_sub(2);
        let scroll = if app.models_tab.file_selected >= visible {
            app.models_tab.file_selected - visible + 1
        } else {
            0
        };

        for (i, file) in model.files.iter().enumerate().skip(scroll).take(visible) {
            let is_selected = i == app.models_tab.file_selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let marker = if is_selected { " > " } else { "   " };
            let quant = file.quant.clone().unwrap_or_default();
            let size = format_size(file.size_bytes);
            let split = if file.is_split {
                format!(" ({} parts)", file.split_parts.len())
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{:<40}", file.filename), style),
                Span::styled(format!("{:>8}", size), style),
                Span::styled(format!("  {:<8}", quant), style),
                Span::styled(split, style),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_directories(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let input_h = if app.models_tab.dir_input_active { 2u16 } else { 0 };
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(input_h),
        Constraint::Length(2),
    ])
    .split(area);

    // Header
    let header = Paragraph::new(vec![Line::from(Span::styled(
        " Model Directories ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))]);
    frame.render_widget(header, chunks[0]);

    // Directory list
    let mut lines = Vec::new();
    let mut idx = 0;

    // Row 0: Download directory
    let dl_dir = app
        .config
        .download_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .or_else(|| {
            catapult_lib::config::AppConfig::default_models_dir()
                .ok()
                .map(|p| p.display().to_string())
        })
        .unwrap_or_else(|| "(default)".to_string());

    let is_selected = app.models_tab.dir_selected == idx;
    let marker = if is_selected { " > " } else { "   " };
    let style = if is_selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let label_style = if is_selected {
        style
    } else {
        Style::default().fg(Color::Blue)
    };
    lines.push(Line::from(vec![
        Span::styled(marker, style),
        Span::styled("Download dir: ", label_style),
        Span::styled(&dl_dir, style),
    ]));
    idx += 1;

    // Scan directories
    if app.config.model_dirs.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "   No extra scan directories configured.",
            Style::default().fg(Color::Cyan),
        )));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Scan directories:",
            Style::default().fg(Color::Blue),
        )));

        for dir in &app.config.model_dirs {
            let is_selected = app.models_tab.dir_selected == idx;
            let marker = if is_selected { " > " } else { "   " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(dir.display().to_string(), style),
            ]));
            idx += 1;
        }
    }

    frame.render_widget(Paragraph::new(lines), chunks[1]);

    // Input area (if active)
    if app.models_tab.dir_input_active {
        let label = match app.models_tab.dir_input_mode {
            DirInputMode::AddScan => "  Add directory: ",
            DirInputMode::SetDownload => "  Download dir: ",
        };
        let value = app.models_tab.dir_input.value();
        let input_line = Line::from(vec![
            Span::styled(label, Style::default().fg(Color::Yellow)),
            Span::styled(value, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White).bg(Color::White)),
        ]);
        frame.render_widget(
            Paragraph::new(vec![Line::raw(""), input_line]),
            chunks[2],
        );
    }

    // Footer
    let footer_text = if app.models_tab.dir_confirm_remove {
        let dir_name = app
            .config
            .model_dirs
            .get(app.models_tab.dir_selected.saturating_sub(1))
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!(" Remove {}? ", dir_name),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("[y]es  [any key]cancel", Style::default().fg(Color::Cyan)),
        ])
    } else if app.models_tab.dir_input_active {
        Line::from(Span::styled(
            " [Enter]Confirm  [Esc]Cancel",
            Style::default().fg(Color::Cyan),
        ))
    } else {
        Line::from(Span::styled(
            " [a]Add scan dir  [e]Edit download dir  [x]Remove selected  [Esc]Back",
            Style::default().fg(Color::Cyan),
        ))
    };

    frame.render_widget(
        Paragraph::new(vec![Line::raw(""), footer_text]),
        chunks[3],
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.0} MB", mb)
    }
}

fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

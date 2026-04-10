use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use catapult_lib::runtime::{AssetOption, ReleaseInfo};

use crate::tui::app::{Action, ActiveDownload, Tab, TuiApp};
use crate::tui::event::TuiEvent;

pub struct RuntimeTabState {
    pub selected: usize,
    pub release: Option<ReleaseInfo>,
    pub release_loading: bool,
    pub release_error: Option<String>,
    pub asset_selected: usize,
    pub show_assets: bool,
}

impl Default for RuntimeTabState {
    fn default() -> Self {
        Self {
            selected: 0,
            release: None,
            release_loading: false,
            release_error: None,
            asset_selected: 0,
            show_assets: false,
        }
    }
}

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    // Asset selection mode
    if app.runtime_tab.show_assets {
        return handle_asset_selection(app, key);
    }

    match key.code {
        KeyCode::Up => {
            if app.runtime_tab.selected > 0 {
                app.runtime_tab.selected -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.config.managed_runtimes.len() + app.config.custom_runtimes.len();
            if count > 0 && app.runtime_tab.selected < count - 1 {
                app.runtime_tab.selected += 1;
            }
        }
        KeyCode::Char('d') => {
            // Fetch latest release, then show asset picker
            if !app.runtime_tab.release_loading {
                app.runtime_tab.release_loading = true;
                app.runtime_tab.release_error = None;

                let client = app.http_client.clone();
                let tx = app.event_tx.clone();

                // Get available backends
                let backend_ids: Vec<String> = catapult_lib::hardware::get_system_info()
                    .map(|si| {
                        si.available_backends
                            .iter()
                            .filter(|b| b.available)
                            .map(|b| b.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                tokio::spawn(async move {
                    let result =
                        catapult_lib::runtime::fetch_latest_release(&client, &backend_ids).await;
                    let _ = tx.send(TuiEvent::RuntimeRelease(
                        result.map_err(|e| e.to_string()),
                    ));
                });
            }
        }
        KeyCode::Char('a') => {
            // Activate selected runtime
            let managed_count = app.config.managed_runtimes.len();
            if app.runtime_tab.selected < managed_count {
                let rt = &app.config.managed_runtimes[app.runtime_tab.selected];
                let build = rt.build;
                let backend_id = rt.backend_id.clone();
                app.config.active_runtime =
                    catapult_lib::config::ActiveRuntime::Managed { build, backend_id };
                let _ = app.config.save();
                app.dashboard.loaded = false;
                app.chat_tab.checked = false;
            } else {
                let idx = app.runtime_tab.selected - managed_count;
                if idx < app.config.custom_runtimes.len() {
                    app.config.active_runtime =
                        catapult_lib::config::ActiveRuntime::Custom { index: idx };
                    let _ = app.config.save();
                    app.dashboard.loaded = false;
                    app.chat_tab.checked = false;
                }
            }
        }
        KeyCode::Esc => {
            app.active_tab = Tab::Dashboard;
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

fn handle_asset_selection(app: &mut TuiApp, key: KeyEvent) -> Action {
    let asset_count = app
        .runtime_tab
        .release
        .as_ref()
        .map(|r| r.available_assets.len())
        .unwrap_or(0);

    match key.code {
        KeyCode::Up => {
            if app.runtime_tab.asset_selected > 0 {
                app.runtime_tab.asset_selected -= 1;
            }
        }
        KeyCode::Down => {
            if asset_count > 0 && app.runtime_tab.asset_selected < asset_count - 1 {
                app.runtime_tab.asset_selected += 1;
            }
        }
        KeyCode::Enter => {
            // Start downloading the selected asset
            if let Some(ref release) = app.runtime_tab.release.clone() {
                if let Some(asset) = release.available_assets.get(app.runtime_tab.asset_selected) {
                    start_runtime_download(app, asset, &release.tag_name);
                    app.runtime_tab.show_assets = false;
                }
            }
        }
        KeyCode::Esc => {
            app.runtime_tab.show_assets = false;
        }
        _ => {}
    }
    Action::None
}

pub fn on_release_result(app: &mut TuiApp, result: Result<ReleaseInfo, String>) {
    app.runtime_tab.release_loading = false;
    match result {
        Ok(release) => {
            app.runtime_tab.release = Some(release);
            app.runtime_tab.release_error = None;
            app.runtime_tab.show_assets = true;
            app.runtime_tab.asset_selected = 0;
        }
        Err(e) => {
            app.runtime_tab.release_error = Some(e);
        }
    }
}

fn start_runtime_download(app: &mut TuiApp, asset: &AssetOption, tag_name: &str) {
    let dl_id = format!("runtime-{}", asset.backend_id);

    if app.downloads.contains_key(&dl_id) {
        return;
    }

    let client = app.http_client.clone();
    let asset = asset.clone();
    let tag_name = tag_name.to_string();
    let tx = app.event_tx.clone();
    let dl_id2 = dl_id.clone();
    let total_bytes = asset.size_mb * 1024 * 1024;

    let handle = tokio::spawn(async move {
        let tx2 = tx.clone();
        let dl_id3 = dl_id2.clone();
        let last_send = std::sync::Mutex::new(std::time::Instant::now());
        let result = catapult_lib::runtime::download_runtime(
            &client,
            &asset,
            &tag_name,
            move |mut progress| {
                let mut last = last_send.lock().unwrap();
                let now = std::time::Instant::now();
                if now.duration_since(*last) < std::time::Duration::from_millis(250) {
                    return;
                }
                *last = now;
                progress.id = dl_id3.clone();
                let _ = tx2.send(TuiEvent::DownloadProgress(progress));
            },
        )
        .await;

        let (status, percent) = match &result {
            Ok(_downloaded) => ("complete".to_string(), 100.0),
            Err(e) => (format!("error: {}", e), 0.0),
        };

        // Send downloaded runtime info back on success
        if let Ok(downloaded) = result {
            let _ = tx.send(TuiEvent::RuntimeDownloaded(downloaded));
        }

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
        dl_id.clone(),
        ActiveDownload {
            progress: catapult_lib::runtime::DownloadProgress {
                id: dl_id,
                bytes_downloaded: 0,
                total_bytes,
                percent: 0.0,
                status: "starting".to_string(),
            },
            task_handle: handle,
            dismiss_countdown: None,
        },
    );
}

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(4),  // Active runtime
        Constraint::Min(4),    // Lists or asset picker
        Constraint::Length(2), // Footer
    ])
    .split(area);

    render_active_runtime(app, chunks[0], frame);

    if app.runtime_tab.show_assets {
        render_asset_picker(app, chunks[1], frame);
    } else {
        render_runtime_lists(app, chunks[1], frame);
    }

    render_footer(app, chunks[2], frame);
}

fn render_active_runtime(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let mut lines = vec![Line::from(Span::styled(
        " Active Runtime ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];

    match &app.config.active_runtime {
        catapult_lib::config::ActiveRuntime::Managed { build, backend_id } => {
            let rt = if backend_id.is_empty() {
                app.config.managed_runtimes.iter().find(|r| r.build == *build)
            } else {
                app.config.managed_runtimes.iter().find(|r| r.build == *build && r.backend_id == *backend_id)
            };
            if let Some(rt) = rt {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("Managed", Style::default().fg(Color::Green)),
                    Span::styled(
                        format!("  b{} {}", rt.build, rt.backend_label),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }
        }
        catapult_lib::config::ActiveRuntime::Custom { index } => {
            if let Some(rt) = app.config.custom_runtimes.get(*index) {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("Custom", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("  {}", rt.binary_path.display()),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }
        }
        catapult_lib::config::ActiveRuntime::None => {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("None — press [d] to download", Style::default().fg(Color::Red)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_runtime_lists(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let mut lines = Vec::new();
    let mut idx = 0;

    // Managed runtimes
    if !app.config.managed_runtimes.is_empty() {
        lines.push(Line::from(Span::styled(
            " Managed Runtimes ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        for rt in &app.config.managed_runtimes {
            let is_active = matches!(
                &app.config.active_runtime,
                catapult_lib::config::ActiveRuntime::Managed { build, backend_id } if *build == rt.build && (backend_id.is_empty() || *backend_id == rt.backend_id)
            );
            let is_selected = idx == app.runtime_tab.selected;
            let marker = if is_selected { " > " } else { "   " };
            let active_badge = if is_active { " [ACTIVE]" } else { "" };

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
                Span::styled(
                    format!("b{}  {}", rt.build, rt.backend_label),
                    style,
                ),
                Span::styled(
                    active_badge,
                    if is_selected {
                        style
                    } else {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    },
                ),
            ]));
            idx += 1;
        }
    }

    // Custom runtimes
    if !app.config.custom_runtimes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Custom Runtimes ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        for rt in &app.config.custom_runtimes {
            let is_active = matches!(
                &app.config.active_runtime,
                catapult_lib::config::ActiveRuntime::Custom { index } if *index == idx - app.config.managed_runtimes.len()
            );
            let is_selected = idx == app.runtime_tab.selected;
            let marker = if is_selected { " > " } else { "   " };
            let active_badge = if is_active { " [ACTIVE]" } else { "" };

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
                Span::styled(rt.binary_path.display().to_string(), style),
                Span::styled(
                    active_badge,
                    if is_selected {
                        style
                    } else {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    },
                ),
            ]));
            idx += 1;
        }
    }

    if app.config.managed_runtimes.is_empty() && app.config.custom_runtimes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  No runtimes installed. Press [d] to download the latest llama.cpp build.",
            Style::default().fg(Color::Yellow),
        )));
    }

    // Show release info if loaded
    if let Some(ref err) = app.runtime_tab.release_error {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("  Error checking releases: {}", err),
            Style::default().fg(Color::Red),
        )));
    }
    if app.runtime_tab.release_loading {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Checking for latest release...",
            Style::default().fg(Color::Yellow),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_asset_picker(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let release = match &app.runtime_tab.release {
        Some(r) => r,
        None => return,
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                " Latest Release ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}  ({})", release.tag_name, release.published_at),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(Span::styled(
            "  Select a build to download:",
            Style::default().fg(Color::Blue),
        )),
        Line::raw(""),
    ];

    for (i, asset) in release.available_assets.iter().enumerate() {
        let is_selected = i == app.runtime_tab.asset_selected;
        let marker = if is_selected { " > " } else { "   " };

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let rec = if asset.score >= 90 {
            Span::styled(
                " (recommended)",
                if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Green)
                },
            )
        } else {
            Span::raw("")
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&asset.backend_label, style),
            Span::styled(format!("  {} MB", asset.size_mb), style),
            rec,
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_footer(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let hint = if app.runtime_tab.show_assets {
        " [Enter]Download  [Esc]Cancel"
    } else {
        " [d]Download latest  [a]Activate  [Esc]Back"
    };
    let footer = Paragraph::new(vec![
        Line::raw(""),
        Line::from(Span::styled(hint, Style::default().fg(Color::Cyan))),
    ]);
    frame.render_widget(footer, area);
}

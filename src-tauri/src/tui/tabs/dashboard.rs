use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use catapult_lib::hardware::SystemInfo;
use catapult_lib::models::ModelInfo;
use catapult_lib::runtime::RuntimeInfo;

use crate::tui::app::{Action, Tab, TuiApp};
use crate::tui::server_ctl;

#[derive(Default)]
pub struct DashboardState {
    pub system_info: Option<SystemInfo>,
    pub runtime_info: Option<RuntimeInfo>,
    pub models: Vec<ModelInfo>,
    pub selected_model: usize,
    pub loaded: bool,
}

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up => {
            if app.dashboard.selected_model > 0 {
                app.dashboard.selected_model -= 1;
            }
        }
        KeyCode::Down => {
            let count = app.dashboard.models.len();
            if count > 0 && app.dashboard.selected_model < count - 1 {
                app.dashboard.selected_model += 1;
            }
        }
        KeyCode::Enter => {
            // Select model and jump to server tab
            if let Some(model) = app.dashboard.models.get(app.dashboard.selected_model) {
                app.server_tab.config.model_path = model.path.display().to_string();
                if let Some(ref mmproj) = model.mmproj_path {
                    app.server_tab.config.mmproj_path = Some(mmproj.display().to_string());
                }
                // If server not running, jump to server tab to launch
                if app.server.is_none() {
                    app.active_tab = Tab::Server;
                }
            }
        }
        KeyCode::Char('f') => {
            // Toggle favorite
            if let Some(model) = app.dashboard.models.get(app.dashboard.selected_model) {
                let id = model.id.clone();
                if let Some(pos) = app.config.favorite_models.iter().position(|f| f == &id) {
                    app.config.favorite_models.remove(pos);
                } else {
                    app.config.favorite_models.push(id);
                }
                let _ = app.config.save();
            }
        }
        KeyCode::Char('x') => {
            // Stop server (async — switches to server tab to show modal)
            if app.server.is_some() {
                app.active_tab = Tab::Server;
                super::server::stop_from_outside(app);
            }
        }
        KeyCode::Esc => {
            return Action::Quit;
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    if !app.dashboard.loaded {
        app.dashboard.system_info = catapult_lib::hardware::get_system_info().ok();
        let config = app.config.clone();
        app.dashboard.runtime_info = catapult_lib::runtime::get_runtime_info(&config).ok();
        app.dashboard.models =
            catapult_lib::models::list_installed_models(&config).unwrap_or_default();
        // Sort: favorites first, then by name
        let favs = app.config.favorite_models.clone();
        app.dashboard.models.sort_by(|a, b| {
            let a_fav = favs.contains(&a.id);
            let b_fav = favs.contains(&b.id);
            b_fav.cmp(&a_fav).then_with(|| a.name.cmp(&b.name))
        });
        app.dashboard.loaded = true;
    }

    // Two-column layout
    let columns = Layout::horizontal([
        Constraint::Percentage(45),
        Constraint::Percentage(55),
    ])
    .split(area);

    render_left_column(app, columns[0], frame);
    render_right_column(app, columns[1], frame);
}

fn render_left_column(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let has_runtime = app
        .dashboard
        .runtime_info
        .as_ref()
        .map(|r| r.installed)
        .unwrap_or(false);
    let has_models = !app.dashboard.models.is_empty();

    let guide_h = if !has_runtime || !has_models { 6 } else { 0 };

    let chunks = Layout::vertical([
        Constraint::Length(7),  // System info
        Constraint::Length(5),  // Runtime
        Constraint::Length(7),  // Server
        Constraint::Length(guide_h), // Quick start (conditional)
        Constraint::Min(0),
    ])
    .split(area);

    render_system_info(app, chunks[0], frame);
    render_runtime_info(app, chunks[1], frame);
    render_server_status(app, chunks[2], frame);
    if !has_runtime || !has_models {
        render_quick_start(has_runtime, has_models, chunks[3], frame);
    }
}

fn render_right_column(app: &TuiApp, area: Rect, frame: &mut Frame) {
    // Reserve space for header + models + footer
    let models_header_h = 3u16;
    let models_footer_h = 2;

    let chunks = Layout::vertical([
        Constraint::Length(models_header_h),
        Constraint::Min(4),
        Constraint::Length(models_footer_h),
    ])
    .split(area);

    render_models_header(app, chunks[0], frame);
    render_models_list(app, chunks[1], frame);
    render_models_footer(app, chunks[2], frame);
}

// ── Left column sections ─────────────────────────────────────────────────────

fn render_system_info(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let mut lines = vec![section_header(" System ")];

    if let Some(ref si) = app.dashboard.system_info {
        lines.push(kv_line("  CPU     ", &format!(
            "{} ({}C/{}T)", si.cpu_name, si.cpu_cores, si.cpu_threads
        )));
        lines.push(kv_line("  RAM     ", &format!(
            "{:.1} GB avail / {:.1} GB total",
            si.available_ram_mb as f64 / 1024.0,
            si.total_ram_mb as f64 / 1024.0
        )));
        if si.gpus.is_empty() {
            lines.push(Line::from(vec![
                label_span("  GPU     "),
                Span::styled("None detected", Style::default().fg(Color::Yellow)),
            ]));
        } else {
            for (i, gpu) in si.gpus.iter().enumerate() {
                let lbl = if i == 0 { "  GPU     " } else { "          " };
                let vram = if gpu.vram_mb > 0 {
                    format!(" ({} MB)", gpu.vram_mb)
                } else {
                    " (shared)".to_string()
                };
                lines.push(kv_line(lbl, &format!("{}{}", gpu.name, vram)));
            }
        }
        lines.push(Line::from(vec![
            label_span("  Backend "),
            Span::styled(&si.recommended_backend, Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "  Loading...",
            Style::default().fg(Color::Cyan),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_runtime_info(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let mut lines = vec![section_header(" Runtime ")];

    if let Some(ref ri) = app.dashboard.runtime_info {
        if ri.installed {
            let type_label = match ri.runtime_type.as_str() {
                "managed" => "Managed",
                "custom" => "Custom",
                _ => "Unknown",
            };
            let build_info = match (&ri.build, &ri.backend) {
                (Some(b), Some(be)) => format!("b{} {}", b, be),
                (Some(b), None) => format!("b{}", b),
                (None, Some(be)) => be.clone(),
                _ => String::new(),
            };
            lines.push(Line::from(vec![
                label_span("  Status  "),
                Span::styled("Installed  ", Style::default().fg(Color::Green)),
                Span::styled(type_label, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  {}", build_info),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
            lines.push(hint_line("  [r] Manage runtimes, download updates"));
        } else {
            lines.push(Line::from(vec![
                label_span("  Status  "),
                Span::styled("Not installed", Style::default().fg(Color::Red)),
            ]));
            lines.push(hint_line("  [r] Go to Runtime tab to download"));
        }
    } else {
        lines.push(Line::from(vec![
            label_span("  Status  "),
            Span::styled("Unknown", Style::default().fg(Color::Yellow)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_server_status(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let mut lines = vec![section_header(" Server ")];

    match &app.server {
        Some(ds) => {
            lines.push(Line::from(vec![
                label_span("  Status  "),
                Span::styled(
                    format!("Running :{} (PID {})", ds.port, ds.pid),
                    Style::default().fg(Color::Green),
                ),
            ]));
            if let Some(ref model) = ds.model_path {
                let model_name = std::path::Path::new(model)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(model);
                lines.push(kv_line("  Model   ", model_name));
            }
            if let Some(ref label) = ds.runtime_label {
                lines.push(kv_line("  Runtime ", label));
            }
            let origin_note = match ds.origin {
                server_ctl::ServerOrigin::Tui => "",
                server_ctl::ServerOrigin::External => " (started externally)",
                server_ctl::ServerOrigin::ExternalUnknown => " (external, unknown runtime)",
            };
            if !origin_note.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", origin_note.trim()),
                    Style::default().fg(Color::Yellow),
                )));
            }
            lines.push(hint_line("  [x] Stop server  [s] Server config  [l] View logs"));
        }
        None => {
            lines.push(Line::from(vec![
                label_span("  Status  "),
                Span::styled("Stopped", Style::default().fg(Color::Red)),
            ]));
            lines.push(hint_line("  [s] Configure & launch  [Enter] Quick-launch selected model"));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_quick_start(has_runtime: bool, has_models: bool, area: Rect, frame: &mut Frame) {
    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            " Quick Start ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let mut step = 1;
    if !has_runtime {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}. ", step), Style::default().fg(Color::Yellow)),
            Span::styled("Download a runtime  ", Style::default().fg(Color::White)),
            Span::styled("[r] Runtime tab", Style::default().fg(Color::Cyan)),
        ]));
        step += 1;
    }
    if !has_models {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}. ", step), Style::default().fg(Color::Yellow)),
            Span::styled("Download a model  ", Style::default().fg(Color::White)),
            Span::styled("[m] Models tab", Style::default().fg(Color::Cyan)),
        ]));
        step += 1;
    }
    lines.push(Line::from(vec![
        Span::styled(format!("  {}. ", step), Style::default().fg(Color::Yellow)),
        Span::styled("Configure & launch  ", Style::default().fg(Color::White)),
        Span::styled("[s] Server tab", Style::default().fg(Color::Cyan)),
    ]));

    frame.render_widget(Paragraph::new(lines), area);
}

// ── Right column sections ────────────────────────────────────────────────────

fn render_models_header(app: &TuiApp, area: Rect, frame: &mut Frame) {
    let total = app.dashboard.models.len();
    let fav_count = app
        .dashboard
        .models
        .iter()
        .filter(|m| app.config.favorite_models.contains(&m.id))
        .count();

    let mut lines = vec![section_header(" Models ")];

    if total == 0 {
        lines.push(Line::from(vec![
            Span::styled("  No models installed  ", Style::default().fg(Color::Yellow)),
            Span::styled("[m] Browse & download", Style::default().fg(Color::Cyan)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{} installed", total), Style::default().fg(Color::White)),
            if fav_count > 0 {
                Span::styled(
                    format!(", {} favorited", fav_count),
                    Style::default().fg(Color::Yellow),
                )
            } else {
                Span::raw("")
            },
            Span::styled("  [m] Manage", Style::default().fg(Color::Cyan)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_models_list(app: &TuiApp, area: Rect, frame: &mut Frame) {
    if app.dashboard.models.is_empty() {
        return;
    }

    let visible = area.height as usize;
    // Scroll to keep selection visible
    let scroll = if app.dashboard.selected_model >= visible {
        app.dashboard.selected_model - visible + 1
    } else {
        0
    };

    let lines: Vec<Line> = app
        .dashboard
        .models
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(i, model)| {
            let is_fav = app.config.favorite_models.contains(&model.id);
            let is_selected = i == app.dashboard.selected_model;

            let marker = if is_selected { " > " } else { "   " };
            let star = if is_fav { "* " } else { "  " };
            let size = format_size(model.size_bytes);
            let quant = model.quant.clone().unwrap_or_default();
            let params = model.params_b.clone().unwrap_or_default();

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

            let quant_style = if is_selected { style } else { quant_color(&quant) };

            // Compute name width to fit in available space
            let fixed_width = 3 + 2 + 9 + 9 + 6; // marker + star + size + quant + params
            let name_width = (area.width as usize).saturating_sub(fixed_width);
            let name = if model.name.len() > name_width {
                format!("{}~", &model.name[..name_width.saturating_sub(1)])
            } else {
                format!("{:<width$}", model.name, width = name_width)
            };

            Line::from(vec![
                Span::styled(marker, style),
                Span::styled(star, star_style),
                Span::styled(name, style),
                Span::styled(format!("{:>8} ", size), style),
                Span::styled(format!("{:<8}", quant), quant_style),
                Span::styled(format!("{:>5}", params), style),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_models_footer(app: &TuiApp, area: Rect, frame: &mut Frame) {
    if app.dashboard.models.is_empty() {
        return;
    }

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  [Enter] Select for server  [f] Toggle favorite  [m] Full model management",
            Style::default().fg(Color::Cyan),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn section_header(title: &str) -> Line<'_> {
    Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

fn label_span(text: &str) -> Span<'_> {
    Span::styled(text, Style::default().fg(Color::Blue))
}

fn kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(Color::Blue)),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

fn hint_line(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::Cyan),
    ))
}

fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{:.1}G", gb)
    } else {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.0}M", mb)
    }
}

fn quant_color(quant: &str) -> Style {
    let q = quant.to_uppercase();
    if q.starts_with("F16") || q.starts_with("F32") || q.starts_with("BF16") || q.starts_with("Q8") {
        Style::default().fg(Color::Blue)
    } else if q.starts_with("Q6") {
        Style::default().fg(Color::Cyan)
    } else if q.starts_with("Q5") {
        Style::default().fg(Color::Green)
    } else if q.starts_with("Q4") {
        Style::default().fg(Color::Yellow)
    } else if q.starts_with("Q3") {
        Style::default().fg(Color::Magenta)
    } else if q.starts_with("Q2") || q.starts_with("Q1") {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::White)
    }
}

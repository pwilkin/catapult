use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use catapult_lib::config::AppConfig;

use crate::tui::app::{Action, Tab, TuiApp};
use crate::tui::server_ctl;

#[derive(Default)]
#[allow(dead_code)]
pub struct LogsTabState {
    pub lines: Vec<String>,
    pub follow: bool,
    pub scroll_offset: usize,
    pub file_position: u64,
}

pub fn on_tick(state: &mut LogsTabState, _config: &AppConfig) {
    if let Some(log_path) = server_ctl::log_file_path() {
        if let Ok(content) = std::fs::read_to_string(&log_path) {
            state.lines = content.lines().map(|l| l.to_string()).collect();
            if state.follow {
                state.scroll_offset = state.lines.len().saturating_sub(1);
            }
        }
    }
}

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('f') => {
            app.logs_tab.follow = !app.logs_tab.follow;
            if app.logs_tab.follow {
                app.logs_tab.scroll_offset = app.logs_tab.lines.len().saturating_sub(1);
            }
        }
        KeyCode::Up => {
            app.logs_tab.follow = false;
            if app.logs_tab.scroll_offset > 0 {
                app.logs_tab.scroll_offset -= 1;
            }
        }
        KeyCode::Down => {
            app.logs_tab.follow = false;
            if app.logs_tab.scroll_offset < app.logs_tab.lines.len().saturating_sub(1) {
                app.logs_tab.scroll_offset += 1;
            }
        }
        KeyCode::PageUp => {
            app.logs_tab.follow = false;
            app.logs_tab.scroll_offset = app.logs_tab.scroll_offset.saturating_sub(20);
        }
        KeyCode::PageDown => {
            app.logs_tab.follow = false;
            let max = app.logs_tab.lines.len().saturating_sub(1);
            app.logs_tab.scroll_offset = (app.logs_tab.scroll_offset + 20).min(max);
        }
        KeyCode::Home => {
            app.logs_tab.follow = false;
            app.logs_tab.scroll_offset = 0;
        }
        KeyCode::End => {
            app.logs_tab.follow = true;
            app.logs_tab.scroll_offset = app.logs_tab.lines.len().saturating_sub(1);
        }
        KeyCode::Esc => {
            app.active_tab = Tab::Dashboard;
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    // Check if logs are available
    // Always try to load logs (they persist across restarts)
    if app.logs_tab.lines.is_empty() {
        on_tick(&mut app.logs_tab, &app.config);
    }

    let header_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 2,
    };
    let log_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(4),
    };
    let footer_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2),
        width: area.width,
        height: 2,
    };

    // Header
    let log_path = server_ctl::log_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let follow_badge = if app.logs_tab.follow {
        Span::styled(" [FOLLOW] ", Style::default().fg(Color::Green))
    } else {
        Span::raw("")
    };

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " Server Logs ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            follow_badge,
        ]),
        Line::from(Span::styled(
            format!("  {}", log_path),
            Style::default().fg(Color::Blue),
        )),
    ]);
    frame.render_widget(header, header_area);

    // Check for external server warning
    if let Some(ref ds) = app.server {
        if ds.origin != server_ctl::ServerOrigin::Tui && app.logs_tab.lines.is_empty() {
            let warning = Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled(
                    "  Server was started externally. Log output is not available.",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    "  Use your own log monitoring for this server instance.",
                    Style::default().fg(Color::Blue),
                )),
            ]);
            frame.render_widget(warning, log_area);
            return;
        }
    }

    // Log lines
    if app.logs_tab.lines.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  No log data available.",
            Style::default().fg(Color::Blue),
        )));
        frame.render_widget(empty, log_area);
    } else {
        let visible_lines = log_area.height as usize;
        let start = app.logs_tab.scroll_offset;
        let end = (start + visible_lines).min(app.logs_tab.lines.len());

        let lines: Vec<Line> = app.logs_tab.lines[start..end]
            .iter()
            .map(|l| {
                let style = if l.contains("error") || l.contains("ERROR") {
                    Style::default().fg(Color::Red)
                } else if l.contains("warn") || l.contains("WARN") {
                    Style::default().fg(Color::Yellow)
                } else if l.starts_with("[stderr]") || l.starts_with("#") {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!(" {}", l), style))
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), log_area);
    }

    // Footer
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        " [f]Follow  [Up/Down]Scroll  [PgUp/PgDn]Page  [Home/End]Jump",
        Style::default().fg(Color::Cyan),
    )]));
    frame.render_widget(footer, footer_area);
}

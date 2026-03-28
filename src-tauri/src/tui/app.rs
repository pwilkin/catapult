use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
    Frame,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use catapult_lib::config::AppConfig;
use catapult_lib::runtime::DownloadProgress;

use super::event::TuiEvent;
use super::server_ctl::{DetectedServer, ServerOrigin};
use super::tabs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Runtime,
    Models,
    Server,
    Logs,
    Chat,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Dashboard,
        Tab::Runtime,
        Tab::Models,
        Tab::Server,
        Tab::Logs,
        Tab::Chat,
    ];

    pub fn key(&self) -> char {
        match self {
            Tab::Dashboard => 'd',
            Tab::Runtime => 'r',
            Tab::Models => 'm',
            Tab::Server => 's',
            Tab::Logs => 'l',
            Tab::Chat => 'c',
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Runtime => "Runtime",
            Tab::Models => "Models",
            Tab::Server => "Server",
            Tab::Logs => "Logs",
            Tab::Chat => "Chat",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Tab::Dashboard => 0,
            Tab::Runtime => 1,
            Tab::Models => 2,
            Tab::Server => 3,
            Tab::Logs => 4,
            Tab::Chat => 5,
        }
    }

    pub fn from_key(c: char) -> Option<Tab> {
        match c {
            'd' => Some(Tab::Dashboard),
            'r' => Some(Tab::Runtime),
            'm' => Some(Tab::Models),
            's' => Some(Tab::Server),
            'l' => Some(Tab::Logs),
            'c' => Some(Tab::Chat),
            _ => None,
        }
    }
}

pub enum Action {
    None,
    Quit,
    LaunchChat(PathBuf, Vec<String>), // binary_path, args
    Unhandled, // key not consumed by tab — try global shortcuts
}

pub struct ActiveDownload {
    pub progress: DownloadProgress,
    pub task_handle: JoinHandle<()>,
    /// Ticks remaining before auto-removing a completed/errored download.
    /// None = still active, Some(n) = remove after n ticks.
    pub dismiss_countdown: Option<u8>,
}

#[allow(dead_code)]
pub struct TuiApp {
    pub active_tab: Tab,
    pub config: AppConfig,
    pub http_client: reqwest::Client,
    pub downloads: HashMap<String, ActiveDownload>,
    pub server: Option<DetectedServer>,
    pub input_focused: bool,

    // Tab states
    pub dashboard: tabs::dashboard::DashboardState,
    pub runtime_tab: tabs::runtime::RuntimeTabState,
    pub models_tab: tabs::models::ModelsTabState,
    pub server_tab: tabs::server::ServerTabState,
    pub logs_tab: tabs::logs::LogsTabState,
    pub chat_tab: tabs::chat::ChatTabState,

    // Event sender for async callbacks
    pub event_tx: mpsc::UnboundedSender<TuiEvent>,
}

impl TuiApp {
    pub fn new(config: AppConfig, event_tx: mpsc::UnboundedSender<TuiEvent>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            active_tab: Tab::Dashboard,
            config,
            http_client,
            downloads: HashMap::new(),
            server: None,
            input_focused: false,
            dashboard: tabs::dashboard::DashboardState::default(),
            runtime_tab: tabs::runtime::RuntimeTabState::default(),
            models_tab: tabs::models::ModelsTabState::default(),
            server_tab: tabs::server::ServerTabState::new(),
            logs_tab: tabs::logs::LogsTabState::default(),
            chat_tab: tabs::chat::ChatTabState::default(),
            event_tx,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // Global: Ctrl-C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }

        // Global: Ctrl-X aborts the first active download
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('x') {
            if let Some(id) = self.downloads.keys().next().cloned() {
                self.abort_download(&id);
            }
            return Action::None;
        }

        // When input is focused, only Esc escapes; everything else goes to the tab
        if self.input_focused {
            if key.code == KeyCode::Esc {
                self.input_focused = false;
                return Action::None;
            }
            return self.dispatch_to_tab(key);
        }

        // Let the tab handle the key first
        let result = self.dispatch_to_tab(key);
        if !matches!(result, Action::Unhandled) {
            return result;
        }

        // Global shortcuts — only fire if the tab didn't consume the key
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char(c) => {
                if let Some(tab) = Tab::from_key(c) {
                    self.active_tab = tab;
                    Action::None
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        }
    }

    fn dispatch_to_tab(&mut self, key: KeyEvent) -> Action {
        match self.active_tab {
            Tab::Dashboard => tabs::dashboard::handle_key(self, key),
            Tab::Runtime => tabs::runtime::handle_key(self, key),
            Tab::Models => tabs::models::handle_key(self, key),
            Tab::Server => tabs::server::handle_key(self, key),
            Tab::Logs => tabs::logs::handle_key(self, key),
            Tab::Chat => tabs::chat::handle_key(self, key),
        }
    }

    pub fn on_tick(&mut self) {
        // Poll server status
        self.server = super::server_ctl::detect_server(&self.config);

        // Update logs if on logs tab
        if self.active_tab == Tab::Logs {
            tabs::logs::on_tick(&mut self.logs_tab, &self.config);
        }

        // Tick down completed/errored downloads
        let mut to_remove = Vec::new();
        for (id, dl) in &mut self.downloads {
            if let Some(ref mut count) = dl.dismiss_countdown {
                if *count == 0 {
                    to_remove.push(id.clone());
                } else {
                    *count -= 1;
                }
            }
        }
        for id in to_remove {
            self.downloads.remove(&id);
        }
    }

    pub fn on_download_progress(&mut self, progress: DownloadProgress) {
        let is_final = progress.status == "complete"
            || progress.status == "error"
            || progress.status.starts_with("error:");

        if let Some(dl) = self.downloads.get_mut(&progress.id) {
            dl.progress = progress;
            if is_final {
                // Show final state for a few ticks before removing
                dl.dismiss_countdown = Some(5); // ~10 seconds at 2s tick
                if dl.progress.status == "complete" {
                    self.models_tab.loaded = false;
                    self.dashboard.loaded = false;
                }
            }
        }
    }

    pub fn abort_download(&mut self, id: &str) {
        if let Some(dl) = self.downloads.remove(id) {
            dl.task_handle.abort();
            // Clean up temp file
            let _ = catapult_lib::models::abort_download(id, &self.config);
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let dl_rows = self.downloads.len() as u16;
        let bottom_h = 1 + dl_rows; // status bar + download bars
        let chunks = Layout::vertical([
            Constraint::Length(2),          // Tab bar
            Constraint::Min(1),            // Content
            Constraint::Length(bottom_h),   // Downloads + status bar
        ])
        .split(area);

        self.render_tab_bar(chunks[0], frame);
        self.render_content(chunks[1], frame);
        self.render_bottom(chunks[2], frame);
    }

    fn render_tab_bar(&self, area: Rect, frame: &mut Frame) {
        let titles: Vec<Line> = Tab::ALL
            .iter()
            .map(|tab| {
                Line::from(format!("[{}]{}", tab.key(), tab.label()))
            })
            .collect();

        let tabs = Tabs::new(titles)
            .select(self.active_tab.index())
            .style(Style::default().fg(Color::Blue))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" ")
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::Blue))
                    .title(Span::styled(
                        " Catapult ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )),
            );

        frame.render_widget(tabs, area);
    }

    fn render_content(&mut self, area: Rect, frame: &mut Frame) {
        match self.active_tab {
            Tab::Dashboard => tabs::dashboard::render(self, area, frame),
            Tab::Runtime => tabs::runtime::render(self, area, frame),
            Tab::Models => tabs::models::render(self, area, frame),
            Tab::Server => tabs::server::render(self, area, frame),
            Tab::Logs => tabs::logs::render(self, area, frame),
            Tab::Chat => tabs::chat::render(self, area, frame),
        }
    }

    fn render_bottom(&self, area: Rect, frame: &mut Frame) {
        let dl_count = self.downloads.len() as u16;
        let sections = Layout::vertical([
            Constraint::Length(dl_count), // Download bars
            Constraint::Length(1),        // Status bar
        ])
        .split(area);

        // Download bars
        if dl_count > 0 {
            let mut dl_lines: Vec<Line> = Vec::new();
            for (id, dl) in &self.downloads {
                let p = &dl.progress;
                let bar_w = 25usize;
                let filled = ((p.percent / 100.0) * bar_w as f64) as usize;
                let empty = bar_w.saturating_sub(filled);
                let bar = format!("[{}{}]", "=".repeat(filled), " ".repeat(empty));

                let size_str = if p.total_bytes > 0 {
                    let dl_mb = p.bytes_downloaded as f64 / (1024.0 * 1024.0);
                    let tot_mb = p.total_bytes as f64 / (1024.0 * 1024.0);
                    if tot_mb >= 1024.0 {
                        format!(
                            "{:.1}/{:.1} GB",
                            dl_mb / 1024.0,
                            tot_mb / 1024.0
                        )
                    } else {
                        format!("{:.0}/{:.0} MB", dl_mb, tot_mb)
                    }
                } else {
                    String::new()
                };

                let is_error = p.status.starts_with("error");
                let is_complete = p.status == "complete";
                let status_color = if is_error {
                    Color::Red
                } else if is_complete {
                    Color::Green
                } else {
                    match p.status.as_str() {
                        "downloading" => Color::Green,
                        "retrying" | "starting" => Color::Yellow,
                        _ => Color::White,
                    }
                };

                dl_lines.push(Line::from(vec![
                    Span::styled(" DL ", Style::default().fg(Color::Black).bg(Color::Cyan)),
                    Span::styled(
                        format!(" {}: ", truncate_name(id, 20)),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(bar, Style::default().fg(Color::Green)),
                    Span::styled(
                        format!(" {:.0}% ", p.percent),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(size_str, Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!("  {}", p.status),
                        Style::default().fg(status_color),
                    ),
                    if dl.dismiss_countdown.is_none() {
                        Span::styled(
                            "  [Ctrl-X:abort]",
                            Style::default().fg(Color::Blue),
                        )
                    } else {
                        Span::raw("")
                    },
                ]));
            }
            frame.render_widget(ratatui::widgets::Paragraph::new(dl_lines), sections[0]);
        }

        // Status bar
        let server_status = match &self.server {
            Some(ds) => {
                let origin_badge = match &ds.origin {
                    ServerOrigin::Tui => "",
                    ServerOrigin::External => " [external]",
                    ServerOrigin::ExternalUnknown => " [external, unknown]",
                };
                let runtime_info = match &ds.runtime_label {
                    Some(label) => format!(" via {}", label),
                    None => String::new(),
                };
                Span::styled(
                    format!(
                        "Server: Running :{} (PID {}){}{}",
                        ds.port, ds.pid, origin_badge, runtime_info
                    ),
                    Style::default().fg(Color::Green),
                )
            }
            None => Span::styled("Server: Stopped", Style::default().fg(Color::Red)),
        };

        let runtime_info = Span::styled(
            format!(" | Runtime: {}", self.runtime_label()),
            Style::default().fg(Color::Blue),
        );

        let quit_hint = Span::styled(" | q:Quit Esc:Back", Style::default().fg(Color::Blue));

        let line = Line::from(vec![
            Span::raw(" "),
            server_status,
            runtime_info,
            quit_hint,
        ]);

        let bar = ratatui::widgets::Paragraph::new(line)
            .style(Style::default().bg(Color::White).fg(Color::Black));
        frame.render_widget(bar, sections[1]);
    }

    fn runtime_label(&self) -> String {
        match &self.config.active_runtime {
            catapult_lib::config::ActiveRuntime::Managed { build } => {
                if let Some(rt) = self
                    .config
                    .managed_runtimes
                    .iter()
                    .find(|r| r.build == *build)
                {
                    format!("b{} {}", rt.build, rt.backend_label)
                } else {
                    format!("b{}", build)
                }
            }
            catapult_lib::config::ActiveRuntime::Custom { index } => {
                if let Some(rt) = self.config.custom_runtimes.get(*index) {
                    rt.label.clone()
                } else {
                    "Custom".to_string()
                }
            }
            catapult_lib::config::ActiveRuntime::None => "None".to_string(),
        }
    }
}

fn truncate_name(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}~", &s[..max - 1])
    }
}

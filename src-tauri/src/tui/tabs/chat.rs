use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use catapult_lib::runtime;

use crate::tui::app::{Action, Tab, TuiApp};

#[derive(Default)]
pub struct ChatTabState {
    pub extra_args: String,
    pub chat_binary: Option<PathBuf>,
    pub checked: bool,
}

pub fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Enter => {
            // Launch llama-cli
            if let Some(ref binary) = app.chat_tab.chat_binary {
                let mut args = Vec::new();

                // Model
                if !app.server_tab.config.model_path.is_empty() {
                    args.push("--model".to_string());
                    args.push(app.server_tab.config.model_path.clone());
                }

                // GPU layers
                args.push("--n-gpu-layers".to_string());
                args.push(app.server_tab.config.n_gpu_layers.to_string());

                // Context
                if app.server_tab.config.n_ctx > 0 {
                    args.push("--ctx-size".to_string());
                    args.push(app.server_tab.config.n_ctx.to_string());
                }

                // Flash attention
                args.push("--flash-attn".to_string());
                args.push(app.server_tab.config.flash_attn.clone());

                // Threads
                if let Some(threads) = app.server_tab.config.n_threads {
                    args.push("--threads".to_string());
                    args.push(threads.to_string());
                }

                // Conversation mode
                args.push("--conversation".to_string());

                // Extra args
                if !app.chat_tab.extra_args.is_empty() {
                    for arg in app.chat_tab.extra_args.split_whitespace() {
                        args.push(arg.to_string());
                    }
                }

                return Action::LaunchChat(binary.clone(), args);
            }
        }
        KeyCode::Backspace => {
            app.chat_tab.extra_args.pop();
            if app.chat_tab.extra_args.is_empty() {
                app.input_focused = false;
            }
        }
        KeyCode::Tab => {
            // Focus the extra args input
            app.input_focused = true;
        }
        KeyCode::Char(c) if app.input_focused => {
            app.chat_tab.extra_args.push(c);
        }
        KeyCode::Esc => {
            if app.input_focused {
                app.input_focused = false;
            } else {
                app.active_tab = Tab::Dashboard;
            }
        }
        _ => return Action::Unhandled,
    }
    Action::None
}

pub fn render(app: &mut TuiApp, area: Rect, frame: &mut Frame) {
    // Check for llama-cli binary
    if !app.chat_tab.checked {
        let config = app.config.clone();
        if let Ok(ri) = runtime::get_runtime_info(&config) {
            if let Some(ref path) = ri.path {
                app.chat_tab.chat_binary = runtime::find_chat_binary(path);
            }
        }
        app.chat_tab.checked = true;
    }

    let mut lines = vec![
        Line::from(Span::styled(
            " Chat ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];

    match &app.chat_tab.chat_binary {
        Some(binary) => {
            lines.push(Line::from(vec![
                Span::styled("  Binary   ", Style::default().fg(Color::Blue)),
                Span::styled(
                    binary.display().to_string(),
                    Style::default().fg(Color::White),
                ),
            ]));

            let model = if app.server_tab.config.model_path.is_empty() {
                "(none selected — set in Server tab)"
            } else {
                std::path::Path::new(&app.server_tab.config.model_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&app.server_tab.config.model_path)
            };
            lines.push(Line::from(vec![
                Span::styled("  Model    ", Style::default().fg(Color::Blue)),
                Span::styled(model, Style::default().fg(Color::White)),
            ]));

            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("  Extra args: ", Style::default().fg(Color::Blue)),
                Span::styled(&app.chat_tab.extra_args, Style::default().fg(Color::White)),
                Span::styled("_", Style::default().fg(Color::Cyan)),
            ]));

            lines.push(Line::raw(""));
            if app.server_tab.config.model_path.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  Select a model in the Server tab first.",
                    Style::default().fg(Color::Yellow),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  [Enter] Launch llama-cli",
                        Style::default().fg(Color::Green),
                    ),
                ]));
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled(
                    "  Note: llama-cli loads the model directly (separate from the server).",
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::from(Span::styled(
                    "  The TUI will resume when you exit llama-cli (Ctrl-C or /exit).",
                    Style::default().fg(Color::Cyan),
                )));
            }
        }
        None => {
            lines.push(Line::from(Span::styled(
                "  llama-cli binary not found in the active runtime.",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(Span::styled(
                "  Install or configure a runtime in the Runtime tab.",
                Style::default().fg(Color::Cyan),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

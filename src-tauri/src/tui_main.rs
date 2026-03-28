mod tui;

use std::io;
use std::time::Duration;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use catapult_lib::config::AppConfig;
use tui::app::{Action, TuiApp};
use tui::event::{create_event_handler, TuiEvent};

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let _ = restore_terminal();
        original_hook(panic);
    }));

    // Load config
    let config = AppConfig::load().unwrap_or_default();

    // Setup terminal
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create event handler
    let (mut events, event_tx) = create_event_handler(Duration::from_secs(2));

    // Create app
    let mut app = TuiApp::new(config, event_tx);

    // Main loop
    loop {
        terminal.draw(|frame| app.render(frame))?;

        match events.next().await {
            Some(TuiEvent::Key(key)) => match app.handle_key(key) {
                Action::Quit => break,
                Action::LaunchChat(binary, args) => {
                    // Exit raw mode for llama-cli
                    restore_terminal()?;
                    terminal.clear()?;

                    // Spawn llama-cli and wait
                    let status = std::process::Command::new(&binary)
                        .args(&args)
                        .status();

                    match status {
                        Ok(s) => {
                            if !s.success() {
                                eprintln!("\nllama-cli exited with status: {}", s);
                                eprintln!("Press Enter to return to Catapult...");
                                let mut buf = String::new();
                                let _ = io::stdin().read_line(&mut buf);
                            }
                        }
                        Err(e) => {
                            eprintln!("\nFailed to launch llama-cli: {}", e);
                            eprintln!("Press Enter to return to Catapult...");
                            let mut buf = String::new();
                            let _ = io::stdin().read_line(&mut buf);
                        }
                    }

                    // Re-enter raw mode
                    enable_raw_mode()?;
                    execute!(io::stdout(), EnterAlternateScreen)?;
                    terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                    terminal.clear()?;

                    // Reset chat binary check so it refreshes
                    app.chat_tab.checked = false;
                }
                Action::None | Action::Unhandled => {}
            },
            Some(TuiEvent::Tick) => {
                app.on_tick();
            }
            Some(TuiEvent::DownloadProgress(progress)) => {
                app.on_download_progress(progress);
            }
            Some(TuiEvent::HfSearchResults(results)) => {
                tui::tabs::models::on_search_results(&mut app, results);
            }
            Some(TuiEvent::HfRepoFiles(repo_id, files)) => {
                tui::tabs::models::on_repo_files(&mut app, &repo_id, files);
            }
            Some(TuiEvent::RuntimeRelease(result)) => {
                tui::tabs::runtime::on_release_result(&mut app, result);
            }
            Some(TuiEvent::ServerStopped) => {
                tui::tabs::server::on_server_stopped(&mut app);
            }
            Some(TuiEvent::RuntimeDownloaded(config)) => {
                // Apply config from the download task (has new runtime registered)
                app.config = config;
                let _ = app.config.save();
                app.dashboard.loaded = false;
                app.chat_tab.checked = false;
            }
            None => break,
        }
    }

    restore_terminal()?;
    Ok(())
}

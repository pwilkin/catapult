use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

use catapult_lib::huggingface::HfModel;
use catapult_lib::runtime::DownloadProgress;

#[allow(dead_code)]
pub enum TuiEvent {
    Key(KeyEvent),
    Tick,
    DownloadProgress(DownloadProgress),
    HfSearchResults(Result<Vec<HfModel>, String>),
    HfRepoFiles(String, Result<Vec<catapult_lib::huggingface::HfFile>, String>),
    RuntimeRelease(Result<catapult_lib::runtime::ReleaseInfo, String>),
    RuntimeDownloaded(catapult_lib::runtime::DownloadedRuntime),
    ServerStopped,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<TuiEvent>,
}

impl EventHandler {
    pub async fn next(&mut self) -> Option<TuiEvent> {
        self.rx.recv().await
    }
}

pub fn create_event_handler(tick_rate: Duration) -> (EventHandler, mpsc::UnboundedSender<TuiEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();

    let tx_input = tx.clone();
    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if tx_input.send(TuiEvent::Key(key)).is_err() {
                    break;
                }
            }
        }
    });

    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick_rate);
        loop {
            interval.tick().await;
            if tx_tick.send(TuiEvent::Tick).is_err() {
                break;
            }
        }
    });

    (EventHandler { rx }, tx)
}

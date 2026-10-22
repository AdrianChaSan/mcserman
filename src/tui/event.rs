use crate::core::progress::ProgressUpdate;
use crate::tui::app::ServerState;
use crossterm::event::{Event as CrosstermEvent, KeyEvent};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

#[allow(dead_code)]
#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    Progress(ProgressUpdate),
    InstanceStatusChanged { name: String, state: ServerState },
    LogLineReceived { instance: String, line: String },
    VersionsLoaded(Vec<String>),
    VersionsLoadError(String),
    ModpackResolved(Box<Result<crate::modrinth::modpack::ResolvedModpack, String>>),
    InstanceCreated(Result<String, String>),
    AddModCompleted(Result<String, String>),
    StatusMessage(String),
}

pub struct EventHandler {
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<AppEvent>,
}

impl EventHandler {
    pub fn new(bounded_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(bounded_size);
        Self { rx, tx }
    }

    pub fn sender(&self) -> mpsc::Sender<AppEvent> {
        self.tx.clone()
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}

pub fn spawn_event_loop(tx: mpsc::Sender<AppEvent>, tick_rate: Duration) {
    tokio::spawn(async move {
        let mut reader = crossterm::event::EventStream::new();
        let mut tick = tokio::time::interval(tick_rate);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if tx.send(AppEvent::Tick).await.is_err() {
                        break;
                    }
                }
                maybe_event = reader.next() => {
                    match maybe_event {
                        Some(Ok(CrosstermEvent::Key(key))) => {
                            if key.kind == crossterm::event::KeyEventKind::Press
                                && tx.send(AppEvent::Key(key)).await.is_err()
                            {
                                break;
                            }
                        }
                        Some(Ok(CrosstermEvent::Resize(w, h))) => {
                            if tx.send(AppEvent::Resize(w, h)).await.is_err() {
                                break;
                            }
                        }
                        Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
            }
        }
    });
}
